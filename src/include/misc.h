/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, version 3.
 *
 * See the file LICENSE for the full license text.
 */
#pragma once

#include <stdint.h>
#include "structs.h"

/*==============================================================================
 *  Checksum Functions (Rust #[export_name] — declarations kept for C callers)
 *==============================================================================*/

uint8_t  calc_checksum(const uint8_t *, int);
uint32_t crc32(const uint8_t *, size_t);
uint32_t crc32_iter(uint32_t, const uint8_t);
bool     verify_checksum(const uart_packet_t *);

/*==============================================================================
 *  Global State
 *==============================================================================*/

/* global_hid is Rust-owned — no C extern needed */
/* global_cfg is Rust-owned — no C extern needed */
extern device_fw_t     global_fw;
/* global_led is Rust-owned — no C extern needed */
extern device_hw_t     global_hw;

/*==============================================================================
 *  LED Control (blink_led in hal_shim.c, others in led.c)
 *==============================================================================*/

void    blink_led(void);
void    restore_leds(void);
uint8_t toggle_led(void);
