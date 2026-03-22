# DeskHop Semi-DDM Firmware Project

## Project Overview

DeskHop is an open-source USB KVM switch using two Raspberry Pi Pico (RP2040) boards.
This fork extends it with **Semi-DDM USB passthrough**, **HID++ Always-On** bidirectional communication,
and a **key remapping engine** for Logitech Bolt receiver on DeskHop PCB v1.1.

## Build

```bash
mkdir build && cd build
cmake .. && make -j$(nproc)
```

- Toolchain: `arm-none-eabi-gcc`, CMake 3.6+
- Target: RP2040 (Cortex-M0+, `thumbv6m-none-eabi`)
- Flags: `-Ofast -Wall -mcpu=cortex-m0plus -mtune=cortex-m0plus`
- Output: `deskhop.uf2` (single binary for both Pico A and B, role auto-detected)
- Debug: `-DDH_DEBUG=ON` for debug build

## Architecture

- **Language**: C11 / C++17
- **USB Stack**: TinyUSB (device on native USB RHPORT 0, host on PIO-USB RHPORT 1)
- **PIO-USB**: Pico-PIO-USB (sekigon-gonnoc) — Full-Speed / Low-Speed only
- **Inter-board comm**: UART0 @ 3,686,400 baud, 8N1, DMA-based, galvanic isolated (TI ISO7721DR)
- **Dual-core**: Core 0 = TinyUSB device+host tasks, Core 1 = PIO-USB low-level

### Key Source Files

| File | Role |
|------|------|
| `src/main.c` | Main loop, task scheduling |
| `src/usb.c` | TinyUSB device/host callbacks |
| `src/usb_descriptors.c` | HID report/configuration descriptors |
| `src/keyboard.c` | Keyboard hotkey processing, report queuing |
| `src/mouse.c` | Mouse coordinate conversion, screen switching |
| `src/handlers.c` | UART message handlers |
| `src/hid_parser.c` | HID descriptor parser |
| `src/hid_report.c` | HID report interpretation/conversion |
| `src/tasks.c` | Periodic tasks (LED, screensaver, etc.) |
| `src/setup.c` | Initialization (clock, UART, DMA, USB, watchdog) |
| `src/protocol.c` | UART packet protocol (fixed-length) |

### Semi-DDM Added Files

| File | Role |
|------|------|
| `src/passthrough.c` | Passthrough state, descriptor capture/dynamic generation, mode branching |
| `src/key_remap.c` | Key remapping engine (SIMPLE, TAP_HOLD, etc.) |
| `src/include/passthrough.h` | Passthrough structs and function declarations |
| `src/include/key_remap.h` | Remap type definitions and config structs |

## Specification Documents

Project specifications live in `spec/` and are **read-only** by default.
A PreToolUse hook injects a hint requiring explicit user confirmation before modifying any file in `spec/`.

- `spec/TECHSTACK.md` — Hardware overview, software stack, Semi-DDM architecture design, key remap engine, Rust porting analysis
- `spec/SSR.md` — Software requirements specification (functional/non-functional requirements, test plan, traceability matrix)

## Branching Strategy

- **`semi-ddm`** — Main development branch (GitHub default). Feature branches merge here.
- **`main`** — Upstream tracking only (`hrvach/deskhop`). No project changes.
- Commits must be **atomic** — one logical change per commit.
- Use **Conventional Commits** format: `type(scope): description` (e.g., `feat(passthrough):`, `fix(remap):`, `docs:`, `chore:`).

## Test

```bash
cmake -S tests -B build-test -DCMAKE_C_COMPILER=clang
cmake --build build-test
cd build-test && ctest --output-on-failure
```

- Framework: Unity (ThrowTheSwitch), submodule at `tests/unity/`
- Host-only build (`clang`, x86) — NOT cross-compiled for RP2040
- Stubs in `tests/stubs/` for Pico SDK types
- Scope: new modules + changed/added functions in existing files
- Waiver: skip test if it requires invasive changes to upstream code
- Coverage: `cmake -DCOVERAGE=ON`, then after `ctest`:
  ```bash
  LLVM_PROFILE_FILE=cov.profraw ctest --output-on-failure
  llvm-profdata merge -sparse cov.profraw -o cov.profdata
  llvm-cov report ./test_passthrough -instr-profile=cov.profdata -ignore-filename-regex='(unity|stubs)'
  ```

## Lint

- **clang-format**: `.clang-format` at project root (LLVM-based, 110 col, 4-space indent)
  - `git clang-format --extensions c,h origin/semi-ddm` — format changed lines only
- **clang-tidy**: `.clang-tidy` at project root (`bugprone-*`, `clang-analyzer-*`)
  - Uses `build-test/compile_commands.json`
  - Scope: changed `src/` files only

## Code Style

- Follow DeskHop upstream coding style (C11, English comments, TinyUSB callback patterns)
- Keep comments concise — no change history in comments
- No excessive `TODO` placeholders in implementation code
- Prefer bulk find-and-replace when simple patterns need changing across the codebase

## Webconfig (config.htm)

The on-device configuration page is a compressed HTML file embedded in a FAT12 disk image.

### Build pipeline

```
webconfig/form.py          — Field definitions (FormField → api_field_map index mapping)
webconfig/render.py        — Jinja2 template rendering + zlib compression
webconfig/templates/       — main.html, form.html, script.js, style.css
         ↓
webconfig/config.htm       — Compressed self-extracting HTML (shipped in firmware)
webconfig/config-unpacked.htm — Uncompressed version (for local preview)
         ↓
disk/disk.img              — FAT12 image containing config.htm (embedded in firmware via disk.S)
```

### Regenerating config.htm

```bash
cd webconfig && make render
```

Requires: `python3`, `jinja2` (`pip install jinja2`)

### Rebuilding disk.img

`disk/create.sh` uses `sudo mount` which requires root. Use this Python alternative:

```bash
cd /path/to/deskhop
python3 -c "
import struct
with open('webconfig/config.htm', 'rb') as f: data = f.read()
sz = len(data)
img = bytearray(2*1024*1024)
# Boot sector (FAT12)
boot = bytearray(512)
boot[0:3]=b'\xEB\x3C\x90'; boot[3:11]=b'mkfs.fat'
struct.pack_into('<H',boot,11,512); boot[13]=4
struct.pack_into('<H',boot,14,1); boot[16]=1
struct.pack_into('<H',boot,17,512); struct.pack_into('<H',boot,19,4096)
boot[21]=0xF8; struct.pack_into('<H',boot,22,3)
struct.pack_into('<H',boot,24,16); struct.pack_into('<H',boot,26,1)
boot[38]=0x29; boot[43:54]=b'DESKHOP    '; boot[54:62]=b'FAT12   '
boot[510:512]=b'\x55\xAA'; img[0:512]=boot
# FAT
fo=512; img[fo]=0xF8; img[fo+1]=0xFF; img[fo+2]=0xFF
# Root dir (sector 4)
ro=4*512
le=bytearray(32); le[0:11]=b'DESKHOP    '; le[11]=0x08
img[ro:ro+32]=le
# File data (after root: sector 36)
do_=36*512; img[do_:do_+sz]=data
cs=2048; cn=(sz+cs-1)//cs
for i in range(cn):
    c=2+i; eo=fo+(c*3)//2; nxt=(c+1) if i<cn-1 else 0xFFF
    if c%2==0: img[eo]=nxt&0xFF; img[eo+1]=(img[eo+1]&0xF0)|((nxt>>8)&0x0F)
    else: img[eo]=(img[eo]&0x0F)|((nxt<<4)&0xF0); img[eo+1]=(nxt>>4)&0xFF
fe=bytearray(32); fe[0:11]=b'CONFIG  HTM'; fe[11]=0x20
struct.pack_into('<H',fe,26,2); struct.pack_into('<I',fe,28,sz)
img[ro+32:ro+64]=fe
with open('disk/disk.img','wb') as f: f.write(img[:128*512])
print(f'disk.img: {sz}B config.htm, {cn} clusters')
"
```

### Linking disk.img into firmware

`disk/disk.S` uses `.incbin` to embed `disk/disk.img` into the `.section_disk` flash region.
CMake does NOT track `disk.img` as a dependency — after updating it, touch the assembly file:

```bash
touch disk/disk.S
cd build && make -j$(nproc)
```

### Adding new webconfig fields

1. Add field to `config_t` in `src/include/structs.h` (before `checksum`)
2. Add default value in `src/defaults.c`
3. Add `api_field_map[]` entry in `src/protocol.c` (pick unused index)
4. Add `FormField()` in `webconfig/form.py`
5. If new section: add template variable in `webconfig/render.py` and loop in `webconfig/templates/main.html`
6. Bump `CURRENT_CONFIG_VERSION` in `src/include/config.h`
7. Regenerate: `cd webconfig && make render` → rebuild disk.img → `touch disk/disk.S` → firmware build

## Hardware Constraints

- RP2040: 264 KB SRAM, 2 MB Flash, 16 USB endpoints, 120 MHz system clock
- PIO-USB consumes 1 PIO block (3 SMs + 32 instruction words)
- UART single isolated channel at 3.6864 Mbps
- Target receivers: Logitech Bolt (VID=0x046D, PID=0xC548) and Unifying (PID=0xC52B), 3 HID interfaces
- Receiver detection: VID-based only (0x046D), no PID-specific logic
