/* DeskHop handlers — hotkey wrappers (fn ptr refs) + HAL-only handlers. */
#include "main.h"

extern void rust_output_toggle(device_t *), rust_screen_border_hotkey(device_t *),
    rust_fw_upgrade_a(void), rust_fw_upgrade_b(void), rust_switch_lock_toggle(void),
    rust_gaming_mode_toggle(void), rust_screenlock_handler(device_t *),
    rust_wipe_config_hotkey(device_t *), rust_mouse_zoom_toggle(void),
    rust_screensaver_pong_enable(void), rust_screensaver_jitter_enable(void),
    rust_screensaver_disable(void), rust_config_enable(device_t *);

/* Hotkey handlers (referenced by hotkeys[] fn ptrs) */
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

/* HAL-only handlers (still called from process_packet) */
void handle_fw_upgrade_msg(uart_packet_t *p, device_t *s) { reset_usb_boot(1 << PICO_DEFAULT_LED_PIN, 0); }
void handle_consumer_control_msg(uart_packet_t *p, device_t *s) { queue_cc_packet(p->data, s); }
void handle_proxy_msg(uart_packet_t *p, device_t *s) { hal_queue_packet(&p->data[1], p->data[0], PACKET_DATA_LENGTH-1); }

void set_active_output(device_t *s, uint8_t o) {
    s->active_output=o; restore_leds(s); send_value(o, OUTPUT_SELECT_MSG); release_all_keys(s);
}
