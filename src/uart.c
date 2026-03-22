/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 *
 * Packet encoding ported to Rust. This file contains HAL-dependent
 * queue/DMA functions and the process_packet dispatch.
 */
#include "main.h"

extern void rust_write_raw_packet(uint8_t *dst, const uint8_t *packet);
extern uint8_t rust_handle_simple_msg(uint8_t ptype, const uint8_t *data);

/* Rust wrapper */
void write_raw_packet(uint8_t *dst, uart_packet_t *packet) {
    rust_write_raw_packet(dst, (const uint8_t *)packet);
}

/* HAL-dependent: queue_t */
void queue_packet(const uint8_t *data, enum packet_type_e packet_type, int length) {
    uart_packet_t packet = {.type = packet_type};
    memcpy(packet.data, data, length);
    queue_try_add(&global_state.uart_tx_queue, &packet);
}

void send_value(const uint8_t value, enum packet_type_e packet_type) {
    queue_packet(&value, packet_type, sizeof(uint8_t));
}

/* HAL-dependent: DMA + queue_t */
void process_uart_tx_task(device_t *state) {
    uart_packet_t packet = {0};
    if (dma_channel_is_busy(state->dma_tx_channel)) return;
    if (!queue_try_remove(&state->uart_tx_queue, &packet)) return;
    write_raw_packet(uart_txbuf, &packet);
    dma_channel_transfer_from_buffer_now(state->dma_tx_channel, uart_txbuf, RAW_PACKET_LENGTH);
}

/* Packet dispatch — routes to Rust handlers or HAL-dependent C handlers */
void process_packet(uart_packet_t *packet, device_t *state) {
    if (!verify_checksum(packet)) return;

    switch (packet->type) {
        case CONSUMER_CONTROL_MSG: handle_consumer_control_msg(packet, state); return;
        case SYNC_BORDERS_MSG:     handle_sync_borders_msg(packet, state); return;
        case GET_VAL_MSG:
        case SET_VAL_MSG:          handle_api_msgs(packet, state); return;
        case GET_ALL_VALS_MSG:     handle_api_read_all_msg(packet, state); return;
        case REQUEST_BYTE_MSG:     handle_request_byte_msg(packet, state); return;
        case RESPONSE_BYTE_MSG:    handle_response_byte_msg(packet, state); return;
        case FIRMWARE_UPGRADE_MSG: handle_fw_upgrade_msg(packet, state); return;
        case PROXY_PACKET_MSG:     handle_proxy_msg(packet, state); return;
        case KEYBOARD_REPORT_MSG:  handle_keyboard_uart_msg(packet, state); return;
        case MOUSE_REPORT_MSG:     handle_mouse_abs_uart_msg(packet, state); return;
    }

    uint8_t needs_hal = rust_handle_simple_msg(packet->type, packet->data);
    if (needs_hal) {
        switch (packet->type) {
            case OUTPUT_SELECT_MSG:  handle_output_select_msg(packet, state); break;
            case KBD_SET_REPORT_MSG: handle_set_report_msg(packet, state); break;
            case FLASH_LED_MSG:      hal_blink_led(state); break;
            case WIPE_CONFIG_MSG:    hal_wipe_config(); hal_load_config(state); break;
            case SAVE_CONFIG_MSG:    hal_save_config(state); break;
            case REBOOT_MSG:         hal_reboot(); break;
            case HEARTBEAT_MSG:      break;
        }
    }
}
