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

 #include "structs.h"

 /*==============================================================================
  *  Firmware Update Functions
  *  Functions for managing firmware updates, CRC calculation, and related tasks.
  *==============================================================================*/

 uint32_t calculate_firmware_crc32(void);
 void     reboot(void);
 void     write_flash_page(uint32_t, uint8_t *);

 /*==============================================================================
  *  Firmware Upgrade Helpers (tasks.c)
  *==============================================================================*/
 void     request_byte(device_t *, uint32_t);

 /* fetch_packet, is_start_of_packet — now in hal_shim.c (hal_ prefixed)
    get_ptr_delta — Rust #[export_name] */

 /*==============================================================================
  *  Button Interaction
  *  Functions interacting with the button, e.g. checking if pressed.
  *==============================================================================*/

 bool is_bootsel_pressed(void);
