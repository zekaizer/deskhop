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

extern device_t global_state;

extern device_hid_t    global_hid;
extern device_config_t global_cfg;
extern device_fw_t     global_fw;
extern device_led_t    global_led;
extern device_hw_t     global_hw;

/*==============================================================================
 *  LED Control (blink_led in hal_shim.c, others in led.c)
 *==============================================================================*/

void    blink_led(device_t *);
void    restore_leds(device_t *);
uint8_t toggle_led(void);
