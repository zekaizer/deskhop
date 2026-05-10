# DeskHop

A two-board USB switch that shares one keyboard and mouse between two PCs. Each board sits between a peripheral set and one PC, and the two boards exchange state over UART so input follows the cursor across screens.

## Language

### Data flow direction (HID reports)

**Upstream**:
The peripheral side — keyboards and mice plugged into this board's USB Host port. Reports originate here.
_Avoid_: peripheral, frontend, input side

**Downstream**:
The PC side — the host computer this board presents to as a USB Device. Reports terminate here.
_Avoid_: pc, host, backend, output side. Note: the PC is the "host" in everyday speech but **never** in this codebase, since "USB host" already means the upstream-facing role.

**Peer**:
The other DeskHop board, reached over the inter-board UART link. Each board has exactly one peer.
_Avoid_: remote, other side, slave

### Roles

**USB Host role**:
This board's role on the upstream side — it enumerates and polls the connected keyboard/mouse.

**USB Device role**:
This board's role on the downstream side — it presents composite HID interfaces to its PC.

### Display layout

**Output**:
One of the two PCs the device serves. Always exactly two: `OUTPUT_A=0`, `OUTPUT_B=1`. Maps 1:1 to **board role**. Each output has its own `output_t` (border, OS, mouse speed, screensaver, screen list).

**Screen**:
A single monitor attached to an output. An output can have one or more screens (e.g. a 3-monitor Windows setup is one **output** with three **screens**). Counted by `output_t.screen_count`; `screen_index` tracks the active one.

**Border** (top / bottom):
Per-output pair of Y-coordinate offsets that say "when the cursor crosses *into* this output, place it at this Y" — needed because monitors may differ in size or vertical alignment. Calibrated implicitly: when the user crosses near the top of the source, the destination's `top` border is updated; near the bottom, `bottom` is updated. Synchronized to the **peer** with a `SyncBorders` **message**.

### Wire formats

**Packet**:
The UART transport unit between this board and its **peer** — one type byte plus an 8-byte payload. Everything that crosses the inter-board link is a packet. Type values are `PacketType` enum (`KeyboardReport`=1, … `DebugLog`=26).

A packet's payload is one of three kinds:

**Report**:
A packet payload that carries HID semantics. Direction-agnostic — covers both upstream→downstream input (`KeyboardReport`, `MouseReport`, `ConsumerControl`, `SystemControl`) and PC→peripheral output (`KbdSetReport`).

**Message**:
A packet payload that carries a one-shot, fire-and-forget state-mutation command. Examples: `MouseZoom`, `SwitchLock`, `GamingMode`, `SaveConfig`, `Reboot`, `FlashLed`, `Heartbeat`. Handled by `msg_handlers::handle_simple_msg`. No reply expected.

**Cfgmsg**:
A packet payload belonging to the paired config-API protocol — every request expects a matching reply. Covers `GetVal`/`SetVal`/`GetAllVals`/`RequestByte`/`ResponseByte`/`ProxyPacket`. Handled by the `config_api` service, not `msg_handlers`.

### Verbs

**Combine** (`combine_kbd_states`):
Merge per-device upstream keyboard states on this board with the peer's already-combined state into one union report (modifier OR'd, keys de-duplicated). Pure logic, no transport.

**Route** (`route_kbd`, `route_mouse`, `route_consumer`, `route_system`):
Given one report, decide where it goes *now*: if this board is the **active output**, push to the local downstream queue and update activity; otherwise send a packet over the **peer** link. Not idempotent and not source-aware — the caller must accept that a report received from peer can re-cross peer if active-output flipped during the call (a 1-frame race, treated as benign).

**Output switch**:
The act of changing `active_output`. Two flavors:

- **Local switch**: triggered on this board (hotkey, or cursor crossing a screen edge). The local side updates `active_output`, syncs indicators, releases keys, then sends an `OutputSelect` message so the peer can mirror.
- **Remote switch**: triggered by an `OutputSelect` message from the peer. The receiving side mirrors `active_output`, releases keys, and syncs indicators. No further notification — that would loop.

The two flavors are symmetric in side effects but asymmetric in who originates the message.

### Per-board state (domain partitions)

These are the conceptual buckets a board's runtime state divides into. Implementation today lumps **Identity + Mode + Liveness** into one C struct (`global_cfg` / `device_config_t`); see Flagged ambiguities.

**Identity**:
What this board *is* — fixed at boot. Includes **board role** and the persisted `config` blob.

**Board role** (`board_role`):
Which of the two physical boards this firmware instance is — A or B. Decides UART pin assignment and which PC is "this side's" PC. Distinct from the active output.

**Mode**:
How this board is currently *behaving* — runtime, user-influenced. Includes **active output**, config-mode flag, gaming mode, relative-mouse mode, mouse zoom, switch lock, digitizer-active.

**Config mode**:
A temporary maintenance state entered by a hotkey (default: `LCtrl+RShift+C+O`). While active, the device exposes a USB Mass Storage interface (DESKHOP drive) for firmware UF2 uploads, and the config-API surface is widened. Auto-exits after 300 s via `config_mode_timer`.

**Gaming mode**:
A **preset** that means "behave as if `relative_mouse` and `switch_lock` are both on" — coordinates are RELATIVE and edge-switching is blocked. No effect beyond the OR of those two flags. Stored as an independent flag (port artifact from C); ideally derived.

**Active output**:
Which board currently owns the keyboard/mouse focus, i.e. where reports are routed downstream right now. Either board can be the active output regardless of which board the peripherals are plugged into.

**Liveness**:
Who/what is currently *alive* — connection and activity timestamps. Includes `tud_connected`, `keyboard_connected`, `mouse_connected`, per-role `last_activity`, core1 watchdog timestamp.

**HID state**:
Accumulated input from the upstream peripherals — combined keyboard report, pointer position, mouse buttons.

**FW state**:
Firmware-upgrade progress — page buffer, target firmware metadata, reboot-requested flag.

**Running firmware**:
The image in flash slot 0 — what's executing right now.

**Staging firmware**:
The image in flash slot 1 — written during an upgrade, swapped to running on reboot once verified. Always 256 KB (1024 × 256-byte pages).

**Auto-propagation**:
After a board upgrades itself, its first heartbeat to the **peer** advertises the new version; if the peer's version is older, the peer requests page-by-page transfer (`RequestByte`/`ResponseByte`) and writes into its own staging. See [ADR-0003](docs/adr/0003-two-stage-firmware-upgrade-with-peer-auto-propagation.md).

**LED state**:
Onboard LED blink counter and last-change timestamp.

**HW state** (C-only):
SDK-bound handles — inter-core queues, DMA channels, USB interface tables. Not crossable to Rust.

### Hotkey vocabulary

**Hotkey**:
A `(modifier, keys)` combo on the **upstream** keyboard that triggers a `HotkeyAction`. Matched **simultaneously** (all required modifiers and all required keys present in the same HID report); **extra modifiers are allowed** (e.g. holding RShift while triggering a RAlt+RCtrl hotkey is fine). Empty `keys` means modifier-only — used for ambient toggles like `MouseZoomToggle`.

**Pass-to-OS** (vs **Consumed**):
Per-hotkey flag that decides whether the matched report still reaches the active **downstream** PC. Most hotkeys are consumed (the user doesn't want LCtrl+RShift+G reaching the OS). Modifier-only hotkeys typically pass through, since the modifier alone is too ambiguous to swallow.

**Acknowledge**:
Per-hotkey flag asking for a visible confirmation (`hal.blink()`) when the hotkey fires. `OutputToggle` sets this to false because the indicator change *is* the confirmation; destructive ones (`WipeConfig`) set it to true.

### Indicators

**Onboard LED**:
The board's own status LED, lit when `active_output == board_role` — at-a-glance "is this side currently in focus".

**Keyboard LED state cache** (`keyboard_leds[2]`):
Per-**output** cache of the last caps/num/scroll bitmap each PC asked for via USB SET_REPORT on its keyboard interface. The PC drives this independently of where its keyboard is physically plugged in.

**LED forwarding**:
The physical keyboard always reflects the **active output** PC's cached LED state, even when the keyboard is plugged into the inactive board. The active board receives SET_REPORT from its PC, caches it, and — if this board does not host the keyboard — sends a `KbdSetReport` **message** to the **peer**; the peer then writes the LEDs to the locally-attached keyboard via `sync_leds`.

### Time-based behaviors

**Last activity** (`last_activity[role]`):
Per-**board-role** timestamp of the last upstream input observed. Updated by `route_*` when this board pushes into its own downstream queue, and by `handle_kbd_from_peer`/`handle_mouse_from_peer` when this board is the inactive output (so the source side's `last_activity` advances even when its peripheral input is being routed away). Drives the **screensaver**.

**Heartbeat**:
A 1 Hz packet sent core1 → peer. Carries running firmware `version` + `crc16` so the peer can detect a version mismatch and trigger **auto-propagation**. Also drives `config_mode_timer` expiry-check (auto-reboot) and the visible LED blink while in **config mode**. Skipped while a firmware upgrade is in progress.

**Screensaver**:
Per-**output** movement (`Pong` 5 ms tick / `Jitter` 10 s tick / `Disabled`) injected into the local downstream queue when its output has been idle longer than `idle_time_us`. Runs only on the **inactive output** if `only_if_inactive` is set. Stops after `idle_time_us + max_time_us` (intentionally yields the screen back to the OS).

### Architecture layers (Rust)

**Domain**:
Pure logic. `domain/mod.rs` forbids `use crate::hal`. Returns *decisions* — typically enums or values describing what should happen, never doing it.

**HAL**:
Hardware-side surface. ~13 narrow traits in `hal/traits.rs` (one capability each), plus `hal/pico.rs` (real Pico SDK impl), `hal/mock.rs` (host-test impl), and `hal/ffi/` (C bridge). Service functions take only the traits they need: `fn f(state, hal: &impl ReportQueue + Timer)`.

**Service**:
Orchestration. Takes a domain decision + the minimal HAL trait subset, executes side effects. Most domain modules have a paired service module (`domain/dispatch` ↔ `service/packet_dispatch`, `domain/hid_routing` ↔ `service/router`, `domain/keyboard` ↔ `service/frontend/kbd_pipeline`).

See [ADR-0001](docs/adr/0001-decision-action-split-with-segregated-hal.md).

### Core split (RP2040 dual-core)

**Core0** runs **consumer-side** tasks: drain inter-core queues, send HID reports to **downstream**, send packets to **peer** over UART, kick the watchdog.

**Core1** runs **producer-side** tasks: poll **upstream** USB Host, receive UART packets from **peer**, plus periodic jobs (screensaver, firmware upgrade, heartbeat, LED blink).

Every cross-core inter-core queue (`kbd_queue`, `mouse_queue`, `hid_queue_out`, `uart_tx_queue`) has exactly one producer core (core1) and one consumer core (core0) — no contention. See [ADR-0002](docs/adr/0002-core-split-consumer-on-core0-producer-on-core1.md).

## Relationships

- An **Upstream** report on either board is routed to the **Downstream** of whichever board is the **Active output** — possibly via the **Peer** link.
- A **Board role** is fixed at boot (auto-probed); the **Active output** changes at runtime via hotkey or cursor edge.
- **Peer** traffic carries combined HID state and active-output changes, not raw upstream reports one-for-one.

## Example dialogue

> **Dev:** "If the keyboard is plugged into board A but the active output is board B, where does the keystroke physically travel?"
> **Domain expert:** "Upstream on A → Peer link to B → Downstream on B. A's downstream USB Device sees nothing for that keystroke."

> **Dev:** "Does the active output follow the board role?"
> **Domain expert:** "No. Board role is which physical board you are. Active output is which PC currently has focus. They're independent — that's the whole point of the device."

## Flagged ambiguities

- `service/frontend/` and `service/backend/` in the Rust tree predate this glossary and use the wrong axis. Canonical names are `service/upstream/` (kbd_pipeline, mouse_pipeline) and `service/downstream/` (host_link). `service/msg_bridge.rs` belongs under `service/peer/`. Rename pending.
- "host" is overloaded in the wider USB world (host = the side that enumerates) and in casual speech (host = the PC). In this codebase **only the USB-spec meaning is allowed**; the PC is always **downstream**.
- `device_config_t` / `global_cfg` lumps **Identity + Mode + Liveness** into a single C struct. This is a porting artifact from the original monolithic `device_t`, not a domain boundary. Three structurally separate concepts share one container today.
- On a **remote switch**, `active_output` is written twice — first by `msg_handlers::apply_action(SetActiveOutput)`, then again by `msg_bridge::handle_output_select`. Same value, harmless, but redundant — handle_output_select is only ever reached after apply_action.
- `NUM_SCREENS` (`src/include/constants.h`) is set to 2 but actually counts **outputs**, not screens. The right name is `NUM_OUTPUTS`. Rename pending; for now the macro lies.
- **Gaming mode** is stored as an independent flag despite being a preset for `relative_mouse | switch_lock`. Inconsistent states like `gaming_mode=true, relative_mouse=false, switch_lock=false` are representable but meaningless — gaming_mode wins at every branch site. Cleanup candidate: derive from the two underlying flags.
- `digitizer_active: bool` in `DeviceConfig` is a **dead field** — declared but never read or written in either C or Rust source. Inherited from the original C `device_t` and carried through every refactor without anyone adding a use. The HID descriptor exposes a digitizer (`REPORT_ID_DIGITIZER=7`) unconditionally, so the flag would have no effect even if wired up. Removable.
