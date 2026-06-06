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
 *  Initialization Functions
 *==============================================================================*/

void initial_setup(void);
void serial_init(uint8_t tx_pin, uint8_t rx_pin);
void core1_main(void);

/* Programmatic entry into the RP2040 UF2 bootloader (single source of truth for
 * every reset-to-bootloader path: CDC "flash" command, fw-upgrade ROM recovery,
 * local UF2 write recovery). Halts Core1 first — see hal_shim.c. */
void dh_enter_bootloader(void);
