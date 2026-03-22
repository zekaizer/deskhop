/*
 * This file is part of DeskHop (https://github.com/hrvach/deskhop).
 * Copyright (c) 2025 Hrvoje Cavrak
 * Most logic in Rust. C wrappers + HAL-dependent handlers.
 */
#include "main.h"

extern void rust_output_toggle(device_t *), rust_screen_border_hotkey(device_t *),
    rust_fw_upgrade_a(void), rust_fw_upgrade_b(void), rust_switch_lock_toggle(void),
    rust_gaming_mode_toggle(void), rust_screenlock_handler(device_t *),
    rust_wipe_config_hotkey(device_t *), rust_mouse_zoom_toggle(void),
    rust_screensaver_pong_enable(void), rust_screensaver_jitter_enable(void),
    rust_screensaver_disable(void), rust_config_enable(device_t *),
    rust_handle_keyboard_uart_full(device_t *, const uint8_t *),
    rust_handle_mouse_uart_full(device_t *, const uint8_t *),
    rust_handle_output_select(device_t *, uint8_t),
    rust_handle_set_report(device_t *, uint8_t),
    rust_handle_sync_borders(device_t *, const uint8_t *),
    rust_handle_response_byte(const uint8_t *);
extern uint8_t rust_handle_simple_msg(uint8_t, const uint8_t *);

/* Hotkey wrappers */
void output_toggle_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_output_toggle(s); }
void screen_border_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_screen_border_hotkey(s); }
void fw_upgrade_hotkey_handler_A(device_t *s, hid_keyboard_report_t *r) { rust_fw_upgrade_a(); }
void fw_upgrade_hotkey_handler_B(device_t *s, hid_keyboard_report_t *r) { rust_fw_upgrade_b(); }
void switchlock_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_switch_lock_toggle(); }
void toggle_gaming_mode_handler(device_t *s, hid_keyboard_report_t *r) { rust_gaming_mode_toggle(); }
void screenlock_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_screenlock_handler(s); }
void wipe_config_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_wipe_config_hotkey(s); }
void mouse_zoom_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_mouse_zoom_toggle(); }
void enable_screensaver_pong_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_screensaver_pong_enable(); }
void enable_screensaver_jitter_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_screensaver_jitter_enable(); }
void disable_screensaver_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_screensaver_disable(); }
void config_enable_hotkey_handler(device_t *s, hid_keyboard_report_t *r) { rust_config_enable(s); }

/* UART handlers */
void handle_keyboard_uart_msg(uart_packet_t *p, device_t *s) { rust_handle_keyboard_uart_full(s, p->data); }
void handle_mouse_abs_uart_msg(uart_packet_t *p, device_t *s) { rust_handle_mouse_uart_full(s, p->data); }
void handle_output_select_msg(uart_packet_t *p, device_t *s) { rust_handle_output_select(s, p->data[0]); }
void handle_fw_upgrade_msg(uart_packet_t *p, device_t *s) { reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0); }
void handle_mouse_zoom_msg(uart_packet_t *p, device_t *s) { rust_handle_simple_msg(p->type, p->data); }
void handle_set_report_msg(uart_packet_t *p, device_t *s) { rust_handle_set_report(s, p->data[0]); }
void handle_switch_lock_msg(uart_packet_t *p, device_t *s) { rust_handle_simple_msg(p->type, p->data); }
void handle_sync_borders_msg(uart_packet_t *p, device_t *s) { rust_handle_sync_borders(s, p->data); }
void handle_flash_led_msg(uart_packet_t *p, device_t *s) { hal_blink_led(s); }
void handle_wipe_config_msg(uart_packet_t *p, device_t *s) { hal_wipe_config(); hal_load_config(s); }
void handle_screensaver_msg(uart_packet_t *p, device_t *s) { rust_handle_simple_msg(p->type, p->data); }
void handle_consumer_control_msg(uart_packet_t *p, device_t *s) { queue_cc_packet(p->data, s); }
void handle_save_config_msg(uart_packet_t *p, device_t *s) { hal_save_config(s); }
void handle_reboot_msg(uart_packet_t *p, device_t *s) { hal_reboot(); }
void handle_proxy_msg(uart_packet_t *p, device_t *s) { hal_queue_packet(&p->data[1], p->data[0], PACKET_DATA_LENGTH - 1); }
void handle_toggle_gaming_msg(uart_packet_t *p, device_t *s) { rust_handle_simple_msg(p->type, p->data); }
void handle_heartbeat_msg(uart_packet_t *p, device_t *s) { rust_handle_simple_msg(p->type, p->data); }
void handle_response_byte_msg(uart_packet_t *p, device_t *s) { rust_handle_response_byte(p->data); }

/* HAL-dependent: offsetof + queue */
void handle_api_msgs(uart_packet_t *packet, device_t *state) {
    uint8_t idx = packet->data[0];
    const field_map_t *map = get_field_map_entry(idx);
    if (!map) return;
    uint8_t *ptr = ((uint8_t *)&global_state) + map->offset;
    if (packet->type == SET_VAL_MSG) { if (map->readonly) return; memcpy(ptr, &packet->data[1], map->len); }
    else if (packet->type == GET_VAL_MSG) {
        uart_packet_t r = {.type=GET_VAL_MSG, .data={[0]=idx}};
        memcpy(&r.data[1], ptr, map->len);
        queue_cfg_packet(&r, state);
    }
    reset_config_timer(state);
}

void handle_api_read_all_msg(uart_packet_t *p, device_t *s) {
    uart_packet_t r = {.type=GET_VAL_MSG};
    for (int i = 0; i < get_field_map_length(); i++) { r.data[0] = get_field_map_index(i)->idx; handle_api_msgs(&r, s); }
}

/* HAL: flash address + queue */
void handle_request_byte_msg(uart_packet_t *p, device_t *s) {
    uint32_t a = p->data32[0]; if (a > STAGING_IMAGE_SIZE) return;
    p->data32[1] = *(uint32_t *)&ADDR_FW_RUNNING[a];
    queue_packet(p->data, RESPONSE_BYTE_MSG, PACKET_DATA_LENGTH);
}

/* HAL: restore_leds + send_value + release_all_keys */
void set_active_output(device_t *s, uint8_t o) {
    s->active_output = o; restore_leds(s); send_value(o, OUTPUT_SELECT_MSG); release_all_keys(s);
}
