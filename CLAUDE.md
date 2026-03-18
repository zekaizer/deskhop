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

### Planned New Files (Semi-DDM)

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

## Hardware Constraints

- RP2040: 264 KB SRAM, 2 MB Flash, 16 USB endpoints, 120 MHz system clock
- PIO-USB consumes 1 PIO block (3 SMs + 32 instruction words)
- UART single isolated channel at 3.6864 Mbps
- Target receiver: Logitech Bolt (VID=0x046D, PID=0xC548), 3 HID interfaces
