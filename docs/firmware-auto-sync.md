# Board-to-Board Firmware Auto-Sync

DeskHop is a dual-RP2040 USB KVM: two identical boards (the **same UF2** runs on
both) joined by an inter-board UART. The role A/B is decided at runtime
(`board_role`, 0 = A, 1 = B).

Auto-sync lets one board copy its running firmware image into the other over the
UART — no host PC, no manual flashing of the second board. In outline:

- Each board broadcasts a ~1 Hz **Heartbeat** carrying its `{version: u16,
  crc16: u16}` identity (`heartbeat_tick`, `src-rust/src/service/tasks.rs`).
- A receiver that decides the peer is **newer** (or, in `dh_debug`, same-version
  / different-crc as Board B) pulls the peer's image word-by-word.
- The receiver writes the pulled image into the **STAGING** flash slot (never
  RUNNING), accumulates a CRC32, and on completion verifies and promotes
  STAGING → RUNNING with an SRAM-resident copy, then watchdog-reboots.

**Direction is decided by version, not by role.** Either board can be the
*source* or the *receiver*; the trigger rule (`should_start_fw_upgrade`,
`src-rust/src/domain/actions.rs`) keys off versions/CRCs. The only role-specific
behavior is the `dh_debug` same-version path, restricted to Board B
(`board_role == 1`) so that in development only Board A is flashed and B
auto-adopts A's build.

## Flash layout

From `misc/memory_map.ld`:

| Region              | Origin                         | Length | Purpose                                   |
|---------------------|--------------------------------|--------|-------------------------------------------|
| `FLASH` (RUNNING)   | `0x10000000`                   | 188k   | Executable code (`ADDR_FW_RUNNING`).      |
| `DISK_IMAGE`        | `0x10000000 + 188k`            | 64k    | FAT / web-config disk image.              |
| `FW_METADATA`       | `0x10000000 + (256k − 4k)`     | 4k     | Running-image metadata (`ADDR_FW_METADATA`). |
| `FW_STAGING`        | `0x10040000` (`+256k`)         | 256k   | Staging slot (`ADDR_FW_STAGING`).         |
| `FLASH_CONFIG`      | `0x10000000 + (2048k − 4k)`    | 4k     | Persisted config sector (`ADDR_CONFIG`).  |

A firmware image is **256 KB total** (188k code + 64k disk + 4k metadata). RUNNING
sits at `0x10000000`; STAGING is one full image-length higher at `0x10040000`.
**The last 4 KB of each image is the metadata sector** holding
`firmware_metadata_t { u32 magic=0xf00d; u16 version; u32 checksum; }`
(`src/include/flash.h`); this sector is deliberately **excluded from the CRC**.

Constants (must agree on both sides): `STAGING_IMAGE_SIZE = 262144`
(`STAGING_PAGES_CNT(1024) * FLASH_PAGE_SIZE(256)`, `flash.h`),
`FLASH_SECTOR_SIZE = 4096`, `FLASH_PAGE_SIZE = 256`. The Rust mirror lives in
`src-rust/src/service/fw_upgrade.rs`; `sdk_verify.h` asserts the C side matches
the SDK's `<hardware/flash.h>`.

## Packet types and data layouts

All inter-board packets are a fixed 10-byte raw frame
(`START1 START2 | type(1) | data(8) | checksum(1)`). Only the type byte and the
8-byte `data` payload matter here.

### Heartbeat (`HEARTBEAT_MSG = 12`)
Built by `build_heartbeat_payload` (`src-rust/src/domain/packet.rs`, shared by
`heartbeat_tick` and `pushfw`); parsed by the Heartbeat handler
(`src-rust/src/domain/msg_handlers.rs`).

| Offset | Field     | Type                                            |
|--------|-----------|-------------------------------------------------|
| 0–1    | `version` | u16 LE                                          |
| 2–3    | `crc16`   | u16 LE (low 16 bits of the running-image CRC32) |
| 4      | `output`  | u8 (`active_output`)                            |
| 5–7    | unused    | zero                                            |

`output` is **not** part of the upgrade decision — only `version`, `crc16`, and
the receiver's own `board_role` feed `should_start_fw_upgrade`.

### RequestByte (`REQUEST_BYTE_MSG = 24`)
Emitted by the receiver via `request_byte` (`src/tasks.c`).

| Offset | Field     | Type                          |
|--------|-----------|-------------------------------|
| 0–3    | `address` | u32 LE (image offset to fetch) |
| 4–7    | unused    |                               |

### ResponseByte (`RESPONSE_BYTE_MSG = 25`)
Built by the source in `send_fw_byte` (`src-rust/src/service/fw_upgrade.rs`).

| Offset | Field     | Type                                                  |
|--------|-----------|-------------------------------------------------------|
| 0–3    | `address` | u32 LE (echoes the requested offset)                  |
| 4–7    | `word`    | 4 raw bytes — LE u32 read from the source's RUNNING image |

Despite the name, one Request/Response exchange transfers **4 bytes (one 32-bit
word)**. `send_fw_byte` reads the word with `read_running_fw(address)` (HAL →
`hal_read_fw_running_u32`) and serializes it little-endian.

## Trigger rule (`should_start_fw_upgrade`)

The decision is pure and unit-tested. Given the peer's `(other_version,
other_crc16)`, our `(our_version = _running_fw.version, our_crc16 =
_running_fw.checksum as u16)`, our `board_role`, and whether an upgrade is
already in progress:

```
if already_upgrading { return None; }          // never re-enter mid-transfer

start = other_version > our_version            // (1) peer is NEWER — always pull
     || ( cfg!(feature = "dh_debug")            // (2) dh_debug-only path
          && other_version == our_version
          && other_crc16 != our_crc16
          && board_role == 1 )                  // OUTPUT_B only
```

- **(1) Newer always wins**, regardless of role or build flag. Both operands are
  `u16`, so the unsigned compare has no overflow footgun: an older peer can never
  trigger it (no accidental downgrade in release).
- **(2)** Compiled only in `dh_debug`; fires on same version + different crc16 +
  Board B. In release `cfg!(...)` folds to `false` and only the newer-version
  path remains.

On a positive decision the initial receiver state is `{upgrade_in_progress=true,
byte_done=true, address=0, checksum=0xFFFF_FFFF}` (the CRC32 seed), copied into
`state.fw.fw` by `apply_action`.

A `version` of `0xFFFF` always satisfies `other_version > our_version` — this is
exactly what the `pushfw` dev command exploits (below), with no special-casing
anywhere in the trigger.

## Receiver state machine

`firmware_upgrade_task_c` (`src/tasks.c`) runs each tick while
`upgrade_in_progress && byte_done && tx-queue-not-full`. It delegates the
page/sector/terminal arithmetic to the unit-tested Rust step machine
`next_step(address)` (`src-rust/src/service/fw_upgrade.rs`) and performs only the
flash/SDK side-effects in C.

### `next_step(address)` (pure)
For the current *expected* `address`:

- **Page write**: if `address != 0 && address % 256 == 0`, flush the just-completed
  256-byte page. `page_offset = (address − 1) & !0xFF`. The `address != 0` guard
  is load-bearing — at address 0 no page is complete, and `(0−1) & !0xFF =
  0xFFFFFF00` would be a wild flash address (a latent bug in the original C).
- **Sector erase**: `erase_sector = (page_offset % 4096 == 0)` — only on the first
  page of each sector, so each 4 KB sector is erased exactly once before its first
  page is programmed.
- **Finalize**: if `address >= STAGING_IMAGE_SIZE`, set `finalize` and request
  nothing more. `>=` (not `>`) is critical: the source serves no word past the end,
  so `address` caps at `SIZE`; a strict `>` never fired and the upgrade wedged
  forever. The final page is flushed in the **same** tick, before finalize, so the
  last 256 bytes reach flash.
- Otherwise request the next word at `address`.

C side writes `write_flash_page(ADDR_FW_STAGING + page_offset − XIP_BASE, …)` —
**STAGING, never RUNNING**. Because the receiver runs on Core1, `write_flash_page`
parks Core0 via `multicore_lockout_start_blocking()` during the XIP-offline window
(the original auto-sync freeze fix).

### `receive_fw_byte` (per ResponseByte word)
1. **Sequence guard**: if `address != state.fw.fw.address`, log `rx ABORT`, clear
   `upgrade_in_progress`, reset `address = 0`, return false.
2. **Progress** every 4 KB (`address & 0xfff == 0`): `hal.toggle()` + an
   `fw rx N/SIZE` line (64 lines for a full image).
3. **CRC accumulate**: only for `address < STAGING_IMAGE_SIZE − FLASH_SECTOR_SIZE`
   (the first 258048 bytes); the metadata sector is excluded. Each of the 4 bytes
   is folded with `crc32_iter`.
4. **Page buffer**: copies the 4 bytes into `page_buffer[address & 0xFF ..]`.
5. Advances `address += 4`, sets `byte_done = true`.

### Source terminal symmetry
The receiver's `>=` finalize works **only because the source refuses to serve the
out-of-range word**: `send_fw_byte` returns `None` for `address >=
STAGING_IMAGE_SIZE` (logging `tx end`), so no ResponseByte is sent and the
receiver's `address` caps at exactly `SIZE`. The receiver then finalizes on the
next tick.

### Flow control (backpressure)
`request_byte(addr)` enqueues a RequestByte and clears `byte_done` **only if the
enqueue succeeded** — a transiently-full tx queue is a harmless no-op retried next
tick, not a wedge. The task entry gate (`upgrade_in_progress && byte_done`) plus
`receive_fw_byte` setting `byte_done = true` form the loop:
`request → byte_done=false → (await ResponseByte) → byte_done=true → request next`.

### Finalize
`firmware_upgrade_task_c` clears `upgrade_in_progress`, inverts the accumulated CRC
(`checksum = ~checksum`), and if `calculate_staging_crc32() == checksum` calls
`promote_staging_to_running()`. On mismatch it simply returns — RUNNING untouched.

## CRC contract

A single CRC32 (polynomial `0xEDB88320`, table-driven, seed `0xFFFFFFFF`, final
invert) ties together the build-time metadata, the receiver's streaming
accumulation, and the staging verification. All must produce the **same value
over the same bytes** or finalize fails.

`calc_crc32` (`src-rust/src/domain/crc.rs`) is bit-identical to Python's
`binascii.crc32` / zlib CRC-32: same polynomial, same `0xFFFFFFFF` seed, same
final XOR. (Pinned by tests: `calc_crc32(b"123456789") == 0xCBF43926` and
`calc_crc32(bytes(0..256)) == 0x29058C73`, the latter matching `binascii.crc32`.)

**Range:** the CRC covers the **first 258048 bytes** = `STAGING_IMAGE_SIZE −
FLASH_SECTOR_SIZE`. The last 4 KB metadata sector — which *contains* the CRC — is
excluded.

Three computations must agree:

1. **Build-time metadata** (`misc/crc32.py`): `binascii.crc32(image[:-4096])`,
   packed as `struct.pack('<IHxxI', 0xf00d, version, crc32)`. The `xx`
   (2 padding bytes) places `checksum` at offset 8 to match the C struct's u32
   alignment.
2. **Source RUNNING CRC** (`calculate_firmware_crc32`, `src/hal_util.c`):
   `calc_crc32(ADDR_FW_RUNNING, STAGING_IMAGE_SIZE − FLASH_SECTOR_SIZE)` — same
   range. Its low 16 bits become the heartbeat `crc16`.
3. **Receiver streaming CRC**: seeded `0xFFFFFFFF` at trigger, folded per word in
   `receive_fw_byte` (skipping the last sector), inverted at finalize, and compared
   to `calculate_staging_crc32()` = `calc_crc32(ADDR_FW_STAGING, …)`.

Because the streamed bytes are the source's RUNNING image and STAGING is programmed
with those same bytes, the streaming CRC and `calculate_staging_crc32()` compute
the same value two ways — a self-consistency check that also catches a corrupted
UART transfer.

> Note: the metadata `magic` (`0xf00d`) is a build-time marker injected by
> `crc32.py`; it is **not** read or validated at runtime. Boot integrity rests on
> the CRC-match-before-promote and the boot2-last write ordering (below), not on a
> magic-word check.

## Promote: SRAM-resident STAGING → RUNNING copy

Once STAGING is verified, `promote_staging_to_running()` (`src/hal_util.c`)
overwrites the RUNNING slot — the flash the CPU normally executes from — so the
copy must run entirely from SRAM. Invariants in `promote_staging_cb`:

- **`__not_in_flash_func`**: the callback is linked into SRAM and keeps executing
  while RUNNING (flash) is erased/reprogrammed.
- **Inline word copy, no flash-resident helpers**: a hand-rolled 32-bit
  read/write loop, *not* `memcpy` (which lives in flash). `flash_range_erase` /
  `flash_range_program` are SDK `__not_in_flash_func`, so they are RAM-safe.
- **Cache-bypass source read**: STAGING is read through
  `XIP_NOCACHE_NOALLOC_BASE + (ADDR_FW_STAGING − XIP_BASE)` so it never sees stale
  XIP-cache data for the just-written staging image.
- **`flash_safe_execute` parks the other core** in its RAM-resident lockout
  handler. It returns only if the other core could not be parked (timeout) — and
  since RUNNING was untouched in that case, returning is safe.
- **Interrupts disabled** for the whole copy, so no flash-resident ISR runs
  mid-overwrite.
- **Watchdog disabled first**: the copy runs interrupts-off for seconds and cannot
  kick, so `promote_staging_to_running` clears `WATCHDOG_CTRL_ENABLE_BITS` before
  the copy.
- **boot2 (page 0) written LAST**: erase the whole slot, program pages 1..N, then
  program page 0. So "boot2 valid" ⇔ "promote completed".
- **Watchdog reset, not AIRCR**: the reboot sets `watchdog_hw->scratch[4] = 0`
  (tells the bootrom to do a normal flash boot) and `WATCHDOG_CTRL_TRIGGER_BITS`.
  On RP2040 an SCB AIRCR `SYSRESETREQ` did **not** reboot here (it left the board
  hung) because it does not re-run the bootrom. `watchdog_reboot()` itself is not
  called — it lives in flash at a possibly-different address in the just-written
  image — only inline register writes are used.

## Recovery and safety

One invariant underpins the design: **RUNNING is never touched until a
fully-received, CRC-verified image is ready to promote.**

- **Staging keeps RUNNING bootable for the whole transfer.** A board mid-transfer
  is still running and bootable on its OLD image.
- **Stalled / aborted / CRC-fail transfers need no recovery.** A sequence mismatch
  aborts cleanly; a CRC mismatch at finalize just returns without promoting. In all
  cases RUNNING is intact, and the peer's next ~1 Hz heartbeat re-triggers
  `should_start_fw_upgrade` and the pull restarts from address 0.
- **No retry/timeout logic — by design.** There is intentionally no stall watchdog
  or per-word re-request. Because a wedged transfer leaves RUNNING fully bootable,
  the ROI of retry logic was judged not worth it; recovery is the **BOOTSEL
  USB-flash fall-back** (and, in `dh_debug`, `heartbeat_tick` reboots to the
  bootloader on a BOOTSEL press).
- **Interrupted promote → bootrom auto-BOOTSEL.** boot2 (page 0) is programmed
  last, so a power loss during the promote leaves boot2 invalid and the RP2040
  bootrom drops into BOOTSEL (mass storage), from which a normal UF2 drag
  re-flashes RUNNING.
- **Heartbeat suppression is receiver-only.** `heartbeat_tick` early-returns while
  `upgrade_in_progress`, but only the **receiver** has that flag set, so the
  **source keeps heartbeating normally** during a transfer; the pulling board goes
  quiet until it reboots.

## Observability

The only transfer tracing (all `DH_DEBUG`, drained to CDC — see
[debugging.md](debugging.md)):

- `fw hb peer ver=.. crc=.. mine ver=.. crc=.. role=.. up=N` — the firmware
  identity exchange, logged **on change only** (de-dup key
  `peer_ver<<32 | peer_crc<<16 | up`). `up=1` marks an in-progress pull. This is
  the primary "did auto-sync trigger?" signal.
- `fw rx N/SIZE` — receiver progress, one line per 4 KB (64 lines per full image),
  so a stall's location is visible.
- `fw tx end addr=..` — source reached the image end (the terminal marker).
- `fw rx ABORT addr got=.. want=..` — a sequence mismatch aborted the pull.

## The `pushfw` dev command

`pushfw` (a `DH_DEBUG` CDC command) forces a peer to pull *this* board's running
image even when the normal version/crc direction check would not fire. It sends
**one** Heartbeat with a sentinel `version = 0xFFFF` (u16 max, always greater than
any real version) but the board's **real** `crc16` and `output`:

```
data = build_heartbeat_payload(0xFFFF, real_crc16, active_output)
     = [0xFF, 0xFF, crc_lo, crc_hi, output, 0, 0, 0]
```

The peer's `should_start_fw_upgrade` sees `0xFFFF > our_version`, accepts it as
"newer," and runs the **identical** auto-sync path as a real newer-version sync —
there is no special-case for `0xFFFF` anywhere. The transfer serves the source's
**real** RUNNING image (`send_fw_byte` reads real flash, not the fake version), so
the peer pulls the genuine bytes, verifies the CRC, promotes, and after reboot
reports the **real** version. The fake `0xFFFF` exists only inside that one
heartbeat packet — it is never written to flash or metadata.

Typical use: flash Board A over USB, then type `pushfw` on A's CDC to propagate the
same image to Board B without touching B's USB. Useful when both boards are at the
same version, or when a board sits at a *higher* version than a freshly-built
image and normal auto-sync (newer → older) would not fire.

## Known limitations

- **Power loss during the promote window can brick.** The SRAM-resident copy runs
  interrupts-off for ~seconds with the watchdog disabled. boot2-last ordering means
  an interrupted promote *usually* drops to BOOTSEL (recoverable), but a power loss
  at the wrong instant can still require a manual USB re-flash. True zero-brick
  would need a dedicated bootloader (out of scope).
- **crc16 collision on the debug same-version path.** The heartbeat carries only
  the low 16 bits of the CRC32, so two genuinely-different images sharing those 16
  bits (~1/65536) would not trigger the `dh_debug` same-version sync. The
  newer-version path does not depend on crc16, and the full 32-bit CRC is always
  verified at finalize before promote, so a wrong image is never adopted.
- **Version is u16** (max 65534; `0xFFFF` reserved as the `pushfw` sentinel).
  `CMakeLists.txt` fails the configure if `FIRMWARE_VERSION` would exceed 65534.
- **No retry/timeout** (see Recovery — a deliberate design decision, not a defect):
  a lost ResponseByte stalls the transfer; recovery relies on the next heartbeat
  re-triggering or on BOOTSEL.
