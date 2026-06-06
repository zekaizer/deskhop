# Development & Debug Tooling

Reference for the debug-only features in this firmware: build flags, the
debug log and how to capture it, the serial (CDC) command interface, and how
binary/config versioning works. Everything here is gated behind a build flag
and compiled out of release images.

## Build flags

| Flag | Default | Enables |
|------|---------|---------|
| `DH_DEBUG` | `OFF` | Debug log (`dlog`), the `peer_log` scrollback ring, the boot config dump, CDC line-state logging, and the `logdump`/`ptr`/`pushfw`/`cc`/`kb` CDC commands. |
| `DH_DEBUG_CDC_FLASH` | `OFF` | The `flash` CDC command (jump to the UF2 bootloader). Independent of `DH_DEBUG`. |

Enable at configure time:

```sh
cmake -B build -DDH_DEBUG=ON          # full debug build (dev default)
cmake -B build -DDH_DEBUG_CDC_FLASH=ON # only the CDC flash command
```

`DH_DEBUG` also link-time-sizes the log ring to fill free RAM
(`--defsym=__log_ring_enabled=1`); a release build leaves the ring at size 0
(`PROVIDE` in `misc/memory_map.ld`). The ring is carved out of the linker
memory map, so a debug build has correspondingly less free RAM for the heap —
mind that budget before enlarging it.

The same UF2 runs on both boards; the role (A/B) is decided at runtime.

## Debug log

### Line format

`dlog` (`src-rust/src/service/dlog.rs`) emits one normalized line per event:

```
[A] 0012.345 I pt  : cap addr=01 inst=00 proto=01 len=3b n=03 ok
└┬┘ └──┬───┘ │ └┬─┘  └────────────────────┬─────────────────────┘
board  time  lvl subsys                  event / key=val pairs
```

- **board tag** `[A]`/`[B]` — prepended by `peer_log` at line start (identifies
  which board produced the line, even in a merged stream).
- **time** `SSSS.mmm` — seconds.milliseconds since boot.
- **lvl** — `I`nfo / `W`arn / `E`rror.
- **subsys** — 4-col subsystem tag (`pt`, `cfg`, `hk`, `hb`, `boot`, `dbg`, …).

### Subsystems

| Tag | Source | Notable lines |
|-----|--------|---------------|
| `boot` | setup | `boot: stage=N <Name>` — boot-stage progress (LED-mirrored). |
| `cfg` | config | Boot config dump (`ver role cfgmode active pt gm ss_ms`, `out0/out1 os`); `cfg set idx=N v=0xVV`, save results. |
| `hk` | hotkeys | `[hk] action=<Name>` and config-mode state transitions. |
| `hb` | heartbeat | `tud= kbd= mse= role= out= c0= c1= heap=used/free/total` — liveness + core loop counters + heap. |
| `pt` | passthrough | capture/activate/reconnect, SmartShift double-click, gesture remap (`gesture tap -> Alt+Tab`, `gesture hold -> app drawer`), discrete HID++ keys (`key fi= fn= c= a=`), and rate-limited stream summaries. |
| `dbg` | CDC commands | Echoes of injected `cc`/`kb` taps. |

### High-frequency stream summaries

Per-event logging of wheels and pointer motion would flood the ring, so
`passthrough_service` accumulates and flushes a summary instead:

- `pt scroll n=<count> sum=<delta>` / `pt thumb …` — HiRes wheel / thumbwheel,
  flushed at most once per 300 ms.
- `pt ptr n=<count> dx=<signed> dy=<signed>` — pointer motion, **off by
  default**, toggled by the `ptr` command, flushed at most once per 1 s.
- `pt btn=0x..` — mouse button byte, logged only on change.
- ReprogControls raw-XY (`fn=1`, ~70/s while a diverted button is held) is
  suppressed; its layout is documented at the suppression site.

### Capturing the log

The log lives in a non-destructive scrollback ring (`peer_log`) and is mirrored
to **both** boards' CDC ports over the inter-board UART, so either board's
serial shows the full `[A]`+`[B]` stream.

Two ways to get the boot history:

1. **Open the CDC port.** Asserting DTR on open triggers a scrollback replay
   after a ~500 ms settle, so a freshly opened terminal starts from the
   `boot role=` banner. (A terminal that auto-reconnects and holds DTR high may
   miss the edge — use method 2.)
2. **Type `logdump`.** Re-triggers a full replay without reconnecting.

The config-mode board presents as a mass-storage device (no CDC), so its logs
are forwarded to and surface on the peer's CDC.

## Serial (CDC) commands

Line-based (newline-terminated), dispatched by `rust_dbg_cmd`
(`src-rust/src/hal/ffi/callbacks.rs`). The `cc`/`kb` injectors route to the
**active output**, which lets you probe target-OS key behavior without
reflashing.

| Command | Build flag | Action |
|---------|-----------|--------|
| `logdump` | `DH_DEBUG` | Replay the log scrollback from the oldest retained line. |
| `ptr` | `DH_DEBUG` | Toggle the pointer-motion summary log (off at boot). |
| `pushfw` | `DH_DEBUG` | Force the **peer** to pull this board's running image (auto-sync), bypassing the version/crc direction check. See note below. |
| `cc<hex>` | `DH_DEBUG` | Send a Consumer Control tap (press+release) to the active output. `<hex>` = 16-bit usage. |
| `kb<hex>` | `DH_DEBUG` | Send a keyboard tap to the active output. `<hex>` = 16-bit, high byte = modifier bitmap, low byte = keycode. |
| `flash` | `DH_DEBUG_CDC_FLASH` | Reboot into the RP2040 UF2 bootloader (`reset_usb_boot`). |

### Examples

```text
logdump        # replay history
ptr            # start/stop pointer dx/dy summaries
pushfw         # force the peer to adopt this board's firmware (dev only)
cc1a2          # Consumer 0x01A2 (AC Desktop Show All Applications) -> app drawer on Android
kb042b         # modifier 0x04 (LeftAlt) + keycode 0x2B (Tab) = Alt+Tab
kb0008         # no modifier + keycode 0x08, etc.
flash          # enter bootloader (DH_DEBUG_CDC_FLASH builds)
```

Modifier bits: `0x01` LCtrl, `0x02` LShift, `0x04` LAlt, `0x08` LGUI/Meta
(shift left by 4 for the right-hand variants). HID is stateful: a `kb`/`cc`
command sends a press then an immediate release, i.e. a single tap.

### `pushfw` — forced firmware auto-sync

Normal auto-sync only flows from a **newer** board to an older one (a board pulls
firmware when the peer's heartbeat reports a higher version). When both boards sit
at the same — or a *higher* — version than a freshly-built image (common in
development: you bump nothing, or you flashed a test build with a higher version),
auto-sync never fires.

`pushfw` works around this: the board it is typed on emits a single heartbeat
reporting a sentinel version of `0xFFFF` (the `u16` maximum, always greater than any
real version). The peer accepts it as "newer" and starts an auto-sync, pulling this
board's **real** running image (the byte server streams real flash, not the fake
version) into its staging slot, verifying the CRC, and promoting it. After the peer
reboots it runs this board's real image and reports its real version. The fake
`0xFFFF` lives only inside that one heartbeat packet — it is never written to flash.

Typical flow: flash board A over USB (`flash` + drag the UF2), then type `pushfw` on
A's CDC to propagate the same image to board B without touching B's USB.

## Version management

Two independent version schemes; neither is bumped automatically.

### Binary (firmware) version

Defined in `CMakeLists.txt`:

```
VERSION_MAJOR=0  VERSION_MINOR=77  DDM_VERSION=1
FIRMWARE_VERSION = (MAJOR*1000 + MINOR + 100)*100 + DDM   # 0.77-ddm1 -> 17701
```

A post-build step (`misc/crc32.py`) computes the image CRC32 and packs
`firmware_metadata_t { magic=0xf00d, version=FIRMWARE_VERSION, crc32 }`
(`struct.pack('<IHI', …)`); `objcopy --update-section .section_metadata`
writes it into the ELF. The `.version = 0x0001` literal in `src/main.c` is only
a placeholder so the section exists — at runtime it holds the real
`FIRMWARE_VERSION`.

Used for **board-to-board firmware auto-propagation** (`should_start_fw_upgrade`,
`src-rust/src/domain/actions.rs`): boards exchange `version` + `crc16` in the
heartbeat. A board upgrades from its peer when the peer's `version` is higher;
in `DH_DEBUG` builds, Board B also accepts a same-version-but-different-CRC image
from Board A, so only Board A needs flashing during development. The firmware
version is also readable over the config API (field 78). The USB `bcdDevice`
(`0x0100`) is hardcoded and unrelated to `FIRMWARE_VERSION`.

> Bump `VERSION_MINOR`/`DDM_VERSION` in `CMakeLists.txt` to make a release
> distinguishable and to trigger peer auto-upgrade.

### Config data version

Defined in `src-rust/src/domain/config.rs`:

```
MAGIC_HEADER = 0xB00B1E5   CURRENT_CONFIG_VERSION = 8
```

`validate_config` accepts a stored config only if **all three** hold:
`magic_header == 0xB00B1E5`, `version == CURRENT_CONFIG_VERSION`, and the CRC32
matches. The CRC covers the bytes **before** the `checksum` field
(`offset_of!(Config, checksum)`, not `size − 4` — `Config` is 8-byte aligned and
has trailing padding after `checksum`). On any mismatch the firmware **falls
back to `default_config`** — there is no migration.

Implications:

- Adding a field makes an older (shorter) stored config fail the length check →
  safe fallback. Reordering same-size fields changes the CRC → fallback. Bump
  `CURRENT_CONFIG_VERSION` on any layout change as a belt-and-suspenders guard.
- The config version is independent of the firmware version; config persists
  across firmware updates as long as magic+version+CRC still match.
- **Changing `DEFAULT_CONFIG` does not retroactively change a board that already
  has a valid stored config** — defaults apply only on fallback (fresh or
  invalidated config). To force new defaults onto an existing board, reset it
  from config mode or bump `CURRENT_CONFIG_VERSION` to invalidate the stored
  copy.
