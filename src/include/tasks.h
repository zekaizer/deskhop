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

void firmware_upgrade_task_c(void);
void usb_device_task_c(void);
void usb_host_task_c(void);

/* All task scheduling is in Rust (lib.rs + hal/scheduler.rs).
   C tasks above are wrapped by Rust thin functions in lib.rs. */
