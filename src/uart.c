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

#include "main.h"

/* ================================================== *
 * ===============  Sending Packets  ================ *
 * ================================================== */

/* Now implemented in Rust (src-rust/src/app/packet.rs) */
extern void rust_write_raw_packet(uint8_t *dst, const uint8_t *packet);

void write_raw_packet(uint8_t *dst, uart_packet_t *packet) {
    rust_write_raw_packet(dst, (const uint8_t *)packet);
}

/* Schedule packet for sending to the other box */
void queue_packet(const uint8_t *data, enum packet_type_e packet_type, int length) {
    uart_packet_t packet = {.type = packet_type};
    memcpy(packet.data, data, length);

    queue_try_add(&global_state.uart_tx_queue, &packet);
}

/* Sends just one byte of a certain packet type to the other box. */
void send_value(const uint8_t value, enum packet_type_e packet_type) {
    queue_packet(&value, packet_type, sizeof(uint8_t));
}

/* Process outgoing config report messages. */
void process_uart_tx_task(device_t *state) {
    uart_packet_t packet = {0};

    if (dma_channel_is_busy(state->dma_tx_channel))
        return;

    if (!queue_try_remove(&state->uart_tx_queue, &packet))
        return;

    write_raw_packet(uart_txbuf, &packet);
    dma_channel_transfer_from_buffer_now(state->dma_tx_channel, uart_txbuf, RAW_PACKET_LENGTH);
}

/* ================================================== *
 * ===============  Parsing Packets  ================ *
 * ================================================== */

const uart_handler_t uart_handler[] = {
    /* Core functions */
    {.type = KEYBOARD_REPORT_MSG, .handler = handle_keyboard_uart_msg},
    {.type = MOUSE_REPORT_MSG, .handler = handle_mouse_abs_uart_msg},
    {.type = OUTPUT_SELECT_MSG, .handler = handle_output_select_msg},

    /* Box control */
    {.type = MOUSE_ZOOM_MSG, .handler = handle_mouse_zoom_msg},
    {.type = KBD_SET_REPORT_MSG, .handler = handle_set_report_msg},
    {.type = SWITCH_LOCK_MSG, .handler = handle_switch_lock_msg},
    {.type = SYNC_BORDERS_MSG, .handler = handle_sync_borders_msg},
    {.type = FLASH_LED_MSG, .handler = handle_flash_led_msg},
    {.type = GAMING_MODE_MSG, .handler = handle_toggle_gaming_msg},
    {.type = CONSUMER_CONTROL_MSG, .handler = handle_consumer_control_msg},
    {.type = SCREENSAVER_MSG, .handler = handle_screensaver_msg},

    /* Config */
    {.type = WIPE_CONFIG_MSG, .handler = handle_wipe_config_msg},
    {.type = SAVE_CONFIG_MSG, .handler = handle_save_config_msg},
    {.type = REBOOT_MSG, .handler = handle_reboot_msg},
    {.type = GET_VAL_MSG, .handler = handle_api_msgs},
    {.type = GET_ALL_VALS_MSG, .handler = handle_api_read_all_msg},
    {.type = SET_VAL_MSG, .handler = handle_api_msgs},

    /* Firmware */
    {.type = REQUEST_BYTE_MSG, .handler = handle_request_byte_msg},
    {.type = RESPONSE_BYTE_MSG, .handler = handle_response_byte_msg},
    {.type = FIRMWARE_UPGRADE_MSG, .handler = handle_fw_upgrade_msg},

    {.type = HEARTBEAT_MSG, .handler = handle_heartbeat_msg},
    {.type = PROXY_PACKET_MSG, .handler = handle_proxy_msg},
};

/* Rust handles state mutations for simple messages */
extern uint8_t rust_handle_simple_msg(uint8_t ptype, const uint8_t *data);
extern void rust_handle_mouse_uart(const uint8_t *data);
extern void rust_handle_keyboard_uart(const uint8_t *data);

void process_packet(uart_packet_t *packet, device_t *state) {
    if (!verify_checksum(packet))
        return;

    /* Messages that need HAL access beyond what Rust can do */
    switch (packet->type) {
        /* These need queue/TinyUSB/flash/GPIO access */
        case CONSUMER_CONTROL_MSG:
            handle_consumer_control_msg(packet, state);
            return;
        case SYNC_BORDERS_MSG:
            handle_sync_borders_msg(packet, state);
            return;
        case GET_VAL_MSG:
        case SET_VAL_MSG:
            handle_api_msgs(packet, state);
            return;
        case GET_ALL_VALS_MSG:
            handle_api_read_all_msg(packet, state);
            return;
        case REQUEST_BYTE_MSG:
            handle_request_byte_msg(packet, state);
            return;
        case RESPONSE_BYTE_MSG:
            handle_response_byte_msg(packet, state);
            return;
        case FIRMWARE_UPGRADE_MSG:
            handle_fw_upgrade_msg(packet, state);
            return;
        case PROXY_PACKET_MSG:
            handle_proxy_msg(packet, state);
            return;
    }

    /* Keyboard and mouse reports — Rust handles state, C handles queuing */
    if (packet->type == KEYBOARD_REPORT_MSG) {
        rust_handle_keyboard_uart(packet->data);
        /* Still need C for combine + queue */
        handle_keyboard_uart_msg(packet, state);
        return;
    }

    if (packet->type == MOUSE_REPORT_MSG) {
        rust_handle_mouse_uart(packet->data);
        handle_mouse_abs_uart_msg(packet, state);
        return;
    }

    /* Simple state-setting messages — Rust handles all state mutation */
    uint8_t needs_hal = rust_handle_simple_msg(packet->type, packet->data);

    /* Handle HAL side-effects */
    if (needs_hal) {
        switch (packet->type) {
            case OUTPUT_SELECT_MSG:
                handle_output_select_msg(packet, state);
                break;
            case KBD_SET_REPORT_MSG:
                handle_set_report_msg(packet, state);
                break;
            case FLASH_LED_MSG:
                blink_led(state);
                break;
            case WIPE_CONFIG_MSG:
                wipe_config();
                load_config(state);
                break;
            case SAVE_CONFIG_MSG:
                save_config(state);
                break;
            case REBOOT_MSG:
                reboot();
                break;
            case HEARTBEAT_MSG:
                /* FW upgrade state already set by Rust */
                break;
        }
    }
}
