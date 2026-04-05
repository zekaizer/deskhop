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

/* Task scheduling is now in Rust (lib.rs + hal/scheduler.rs) */

/*==============================================================================
 *  C Task Functions (remaining in tasks.c)
 *==============================================================================*/

void firmware_upgrade_task(void);
void usb_device_task(void);
void usb_host_task(void);

/* Rust #[export_name] tasks — declared for lib.rs scheduler, no C prototype needed:
   heartbeat_output_task, kick_watchdog_task, led_blinking_task,
   packet_receiver_task, process_hid_queue_task, process_kbd_queue_task,
   process_mouse_queue_task, process_uart_tx_task, screensaver_task */
