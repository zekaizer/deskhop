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

/*==============================================================================
 *  Board Roles
 *==============================================================================*/

/* BOARD_ROLE, OTHER_ROLE removed — board_role is Rust-only (global_cfg) */

/*==============================================================================
 *  GPIO Pins (LED, USB)
 *==============================================================================*/

#define GPIO_LED_PIN   25 // LED is connected to pin 25 on a PICO
#define PIO_USB_DP_PIN 14 // D+ is pin 14, D- is pin 15

/*==============================================================================
 *  Serial Pins
 *==============================================================================*/

/* GP12 / GP13, Pins 16 (TX), 17 (RX) on the Pico board */
#define BOARD_A_RX 13
#define BOARD_A_TX 12

/* GP16 / GP17, Pins 21 (TX), 22 (RX) on the Pico board */
#define BOARD_B_RX 17
#define BOARD_B_TX 16

/* SERIAL_*_PIN removed — pins computed locally in serial_init() */
