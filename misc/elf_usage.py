#!/usr/bin/env python3
"""Print actual firmware resource usage from ELF linker symbols."""

import subprocess, sys

elf = sys.argv[1]

# Extract linker symbols for precise measurement
r = subprocess.run(["arm-none-eabi-nm", elf], capture_output=True, text=True)
if r.returncode != 0:
    sys.exit(1)

syms = {}
for line in r.stdout.strip().split("\n"):
    parts = line.split()
    if len(parts) >= 3:
        syms[parts[2]] = int(parts[0], 16)

# FLASH: from 0x10000000 to __flash_binary_end (code + .data init values)
flash_base = 0x10000000
flash_end = syms.get("__flash_binary_end", flash_base)
flash_used = flash_end - flash_base

# RAM: from __data_start__ to __bss_end__ (data + bss, excludes heap/stack)
r2 = subprocess.run(["arm-none-eabi-size", elf], capture_output=True, text=True)
_, data, bss = (int(x) for x in r2.stdout.strip().split("\n")[1].split()[:3])
ram_used = data + bss

FLASH_SIZE = 188 * 1024
RAM_SIZE = 256 * 1024

def fmt(b):
    return f"{b/1024:.1f} KB" if b >= 1024 else f"{b} B"

# Debug-only peer_log history ring, sized at link time to fill free RAM.
# Use linker symbols (authoritative, matches --print-memory-usage) rather than
# `size`, whose Berkeley data+bss over-counts under this copy-to-RAM layout.
RAM_ORIGIN = 0x20000000
ring_start = syms.get("__log_ring_start")
ring_end = syms.get("__log_ring_end")
ring = (ring_end - ring_start) if (ring_start is not None and ring_end is not None) else 0
if ring:
    static_ram = ring_start - RAM_ORIGIN  # vector table + .data + .bss span
    ram_total = ring_end - RAM_ORIGIN     # committed RAM (heap reserve excluded)
else:
    static_ram = ram_used
    ram_total = ram_used

print(f"\n{'Region':<12} {'Used':>10} {'Total':>10} {'%':>8}")
print(f"{'FLASH':<12} {fmt(flash_used):>10} {fmt(FLASH_SIZE):>10} {flash_used*100/FLASH_SIZE:>7.1f}%")
print(f"{'RAM static':<12} {fmt(static_ram):>10} {fmt(RAM_SIZE):>10} {static_ram*100/RAM_SIZE:>7.1f}%")
if ring:
    print(f"{'log_ring':<12} {fmt(ring):>10}   (debug history; link-time fill of free RAM)")
    print(f"{'RAM total':<12} {fmt(ram_total):>10} {fmt(RAM_SIZE):>10} {ram_total*100/RAM_SIZE:>7.1f}%")
