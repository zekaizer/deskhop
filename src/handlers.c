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

/* =================================================== *
 * ============  Hotkey Handler Routines  ============ *
 * =================================================== */

extern void rust_output_toggle(device_t *dev);

void output_toggle_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_output_toggle(state);
};

extern void rust_get_border_position(int16_t pointer_y, int32_t *border_top, int32_t *border_bottom);

void _get_border_position(device_t *state, border_size_t *border) {
    rust_get_border_position(state->pointer_y, &border->top, &border->bottom);
}

extern void rust_screensaver_set(uint8_t value);
extern void rust_screen_border_hotkey(device_t *dev);

void _screensaver_set(device_t *state, uint8_t value) {
    rust_screensaver_set(value);
};

void screen_border_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_screen_border_hotkey(state);
};

extern void rust_fw_upgrade_a(void);
extern void rust_fw_upgrade_b(void);

void fw_upgrade_hotkey_handler_A(device_t *state, hid_keyboard_report_t *report) {
    rust_fw_upgrade_a();
};

void fw_upgrade_hotkey_handler_B(device_t *state, hid_keyboard_report_t *report) {
    rust_fw_upgrade_b();
};

extern void rust_switch_lock_toggle(void);
extern void rust_gaming_mode_toggle(void);

void switchlock_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_switch_lock_toggle();
}

void toggle_gaming_mode_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_gaming_mode_toggle();
};

extern void rust_screenlock_handler(device_t *dev);

void screenlock_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_screenlock_handler(state);
}

extern void rust_wipe_config_hotkey(device_t *dev);
extern void rust_mouse_zoom_toggle(void);
extern void rust_screensaver_pong_enable(void);
extern void rust_screensaver_jitter_enable(void);
extern void rust_screensaver_disable(void);

void wipe_config_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_wipe_config_hotkey(state);
}

void mouse_zoom_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_mouse_zoom_toggle();
};

void enable_screensaver_pong_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_screensaver_pong_enable();
}

void enable_screensaver_jitter_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_screensaver_jitter_enable();
}

void disable_screensaver_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    rust_screensaver_disable();
}

/* Put the device into a special configuration mode */
void config_enable_hotkey_handler(device_t *state, hid_keyboard_report_t *report) {
    /* If config mode is already active, skip this and reboot to return to normal mode */
    if (!state->config_mode_active) {
        watchdog_hw->scratch[5] = MAGIC_WORD_1;
        watchdog_hw->scratch[6] = MAGIC_WORD_2;
    }

    release_all_keys(state);
    state->reboot_requested = true;
};


/* ==================================================== *
 * ==========  UART Message Handling Routines  ======== *
 * ==================================================== */

extern void rust_handle_keyboard_uart_full(device_t *dev, const uint8_t *data);
extern void rust_handle_mouse_uart_full(device_t *dev, const uint8_t *data);
extern void rust_handle_output_select(device_t *dev, uint8_t output);

void handle_keyboard_uart_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_keyboard_uart_full(state, packet->data);
}

void handle_mouse_abs_uart_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_mouse_uart_full(state, packet->data);
}

void handle_output_select_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_output_select(state, packet->data[0]);
}

/* On firmware upgrade message, reboot into the BOOTSEL fw upgrade mode */
void handle_fw_upgrade_msg(uart_packet_t *packet, device_t *state) {
    reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0);
}

void handle_mouse_zoom_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_simple_msg(packet->type, packet->data);
}

extern void rust_handle_set_report(device_t *dev, uint8_t led_value);

void handle_set_report_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_set_report(state, packet->data[0]);
}

void handle_switch_lock_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_simple_msg(packet->type, packet->data);
}

extern void rust_handle_sync_borders(device_t *dev, const uint8_t *data);

void handle_sync_borders_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_sync_borders(state, packet->data);
}

void handle_flash_led_msg(uart_packet_t *packet, device_t *state) {
    hal_blink_led(state);
}

void handle_wipe_config_msg(uart_packet_t *packet, device_t *state) {
    hal_wipe_config();
    hal_load_config(state);
}

void handle_screensaver_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_simple_msg(packet->type, packet->data);
}

/* Process consumer control message */
void handle_consumer_control_msg(uart_packet_t *packet, device_t *state) {
    queue_cc_packet(packet->data, state);
}

void handle_save_config_msg(uart_packet_t *packet, device_t *state) {
    hal_save_config(state);
}

void handle_reboot_msg(uart_packet_t *packet, device_t *state) {
    hal_reboot();
}

void handle_proxy_msg(uart_packet_t *packet, device_t *state) {
    hal_queue_packet(&packet->data[1], packet->data[0], PACKET_DATA_LENGTH - 1);
}

void handle_toggle_gaming_msg(uart_packet_t *packet, device_t *state) {
    rust_handle_simple_msg(packet->type, packet->data);
}

/* Process api communication messages */
void handle_api_msgs(uart_packet_t *packet, device_t *state) {
    uint8_t value_idx = packet->data[0];
    const field_map_t *map = get_field_map_entry(value_idx);

    /* If we don't have a valid map entry, return immediately */
    if (map == NULL)
        return;

    /* Create a pointer to the offset into the structure we need to access */
    uint8_t *ptr = (((uint8_t *)&global_state) + map->offset);

    if (packet->type == SET_VAL_MSG) {
        /* Not allowing writes to objects defined as read-only */
        if (map->readonly)
            return;

        memcpy(ptr, &packet->data[1], map->len);
    }
    else if (packet->type == GET_VAL_MSG) {
        uart_packet_t response = {.type=GET_VAL_MSG, .data={[0] = value_idx}};
        memcpy(&response.data[1], ptr, map->len);
        queue_cfg_packet(&response, state);
    }

    /* With each GET/SET message, we reset the configuration mode timeout */
    reset_config_timer(state);
}

/* Handle the "read all" message by calling our "read one" handler for each type */
void handle_api_read_all_msg(uart_packet_t *packet, device_t *state) {
    uart_packet_t result = {.type=GET_VAL_MSG};

    for (int i = 0; i < get_field_map_length(); i++) {
        result.data[0] = get_field_map_index(i)->idx;
        handle_api_msgs(&result, state);
    }
}

/* Process request packet and create a response */
void handle_request_byte_msg(uart_packet_t *packet, device_t *state) {
    uint32_t address = packet->data32[0];

    if (address > STAGING_IMAGE_SIZE)
        return;

    /* Add requested data to bytes 4-7 in the packet and return it with a different type */
    uint32_t data = *(uint32_t *)&ADDR_FW_RUNNING[address];
    packet->data32[1] = data;

    queue_packet(packet->data, RESPONSE_BYTE_MSG, PACKET_DATA_LENGTH);
}

/* Process response message following a request we sent to read a byte */
/* state->page_offset and state->page_number are kept locally and compared to returned values */
void handle_response_byte_msg(uart_packet_t *packet, device_t *state) {
    uint16_t offset = packet->data[0];
    uint32_t address = packet->data32[0];

    if (address != state->fw.address) {
        state->fw.upgrade_in_progress = false;
        state->fw.address = 0;
        return;
    }
    else {
        /* Provide visual feedback of the ongoing copy by toggling LED for every sector */
        if((address & 0xfff) == 0x000)
            toggle_led();
    }

    /* Update checksum as we receive each byte */
    if (address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE)
        for (int i=0; i<4; i++)
            state->fw.checksum = crc32_iter(state->fw.checksum, packet->data[4 + i]);

    memcpy(state->page_buffer + offset, &packet->data32[1], sizeof(uint32_t));

    /* Neeeeeeext byte, please! */
    state->fw.address += sizeof(uint32_t);
    state->fw.byte_done = true;
}

/* Process a request to read a firmware package from flash */
void handle_heartbeat_msg(uart_packet_t *packet, device_t *state) {
    uint16_t other_running_version = packet->data16[0];

    if (state->fw.upgrade_in_progress)
        return;

    /* If the other board isn't running a newer version, we are done */
    if (other_running_version <= state->_running_fw.version)
        return;

    /* It is? Ok, kick off the firmware upgrade */
    state->fw = (fw_upgrade_state_t) {
        .upgrade_in_progress = true,
        .byte_done = true,
        .address = 0,
        .checksum = 0xffffffff,
    };
}


/* ==================================================== *
 * ==============  Output Switch Routines  ============ *
 * ==================================================== */

/* Now partially in Rust — state mutation done via AppState,
   but HAL calls (restore_leds, send_value, release_all_keys) stay here.
   This is called from hal_set_active_output() in hal_shim.c. */
void set_active_output(device_t *state, uint8_t new_output) {
    state->active_output = new_output;
    restore_leds(state);
    send_value(new_output, OUTPUT_SELECT_MSG);
    release_all_keys(state);
}
