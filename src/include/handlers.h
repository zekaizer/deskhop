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
 *  UART Message Handlers (remaining C implementations)
 *==============================================================================*/

void handle_consumer_control_msg(uart_packet_t *, device_t *);
void handle_fw_upgrade_msg(uart_packet_t *, device_t *);
void handle_proxy_msg(uart_packet_t *, device_t *);

/*==============================================================================
 *  Output Control
 *==============================================================================*/

void set_active_output(device_t *, uint8_t);
