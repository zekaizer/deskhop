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

print(f"\n{'Region':<10} {'Used':>10} {'Total':>10} {'%':>8}")
print(f"{'FLASH':<10} {fmt(flash_used):>10} {fmt(FLASH_SIZE):>10} {flash_used*100/FLASH_SIZE:>7.1f}%")
print(f"{'RAM':<10} {fmt(ram_used):>10} {fmt(RAM_SIZE):>10} {ram_used*100/RAM_SIZE:>7.1f}%")
