/*
 * C HAL shim layer for Rust FFI access to device_t and hardware functions.
 * Each function provides controlled access to a specific device_t field,
 * avoiding the need to mirror the full struct layout in Rust.
 */

#include "main.h"

/* ==================================================== *
 * Layout verification — Rust struct sizes must match C
 * ==================================================== */

// Rust structs.rs defines QUEUE_T_SIZE — must match sizeof(queue_t)
_Static_assert(sizeof(queue_t) == 16,
    "queue_t size changed — update QUEUE_T_SIZE in src-rust/src/app/structs.rs");

// AppState is Rust-owned. C accesses it via rust_get_app_state().
// AppState does NOT mirror device_t — it's a separate Rust struct
// containing only HAL-independent fields.

/* ==================================================== *
 * Read-only state accessors
 * ==================================================== */

uint8_t hal_get_active_output(device_t *dev) { return dev->active_output; }
uint8_t hal_get_board_role(device_t *dev) { return dev->board_role; }
int16_t hal_get_pointer_x(device_t *dev) { return dev->pointer_x; }
int16_t hal_get_pointer_y(device_t *dev) { return dev->pointer_y; }
int16_t hal_get_mouse_buttons(device_t *dev) { return dev->mouse_buttons; }
uint64_t hal_get_core1_last_loop_pass(device_t *dev) { return dev->core1_last_loop_pass; }

/* ==================================================== *
 * Writable state accessors
 * ==================================================== */

void hal_set_active_output(device_t *dev, uint8_t output) { dev->active_output = output; }
void hal_set_pointer_x(device_t *dev, int16_t x) { dev->pointer_x = x; }
void hal_set_pointer_y(device_t *dev, int16_t y) { dev->pointer_y = y; }
void hal_set_mouse_buttons(device_t *dev, int16_t buttons) { dev->mouse_buttons = buttons; }
void hal_set_core1_last_loop_pass(device_t *dev, uint64_t ts) { dev->core1_last_loop_pass = ts; }

/* ==================================================== *
 * Feature flags
 * ==================================================== */

bool hal_get_mouse_zoom(device_t *dev) { return dev->mouse_zoom; }
bool hal_get_switch_lock(device_t *dev) { return dev->switch_lock; }
bool hal_get_gaming_mode(device_t *dev) { return dev->gaming_mode; }
bool hal_get_relative_mouse(device_t *dev) { return dev->relative_mouse; }
bool hal_get_tud_connected(device_t *dev) { return dev->tud_connected; }
bool hal_get_reboot_requested(device_t *dev) { return dev->reboot_requested; }
bool hal_get_config_mode_active(device_t *dev) { return dev->config_mode_active; }

/* ==================================================== *
 * Config access
 * ==================================================== */

uint16_t hal_get_jump_threshold(device_t *dev) { return dev->config.jump_threshold; }
bool hal_get_enable_acceleration(device_t *dev) { return dev->config.enable_acceleration; }
int32_t hal_get_speed_x(device_t *dev, uint8_t output) { return dev->config.output[output].speed_x; }
int32_t hal_get_speed_y(device_t *dev, uint8_t output) { return dev->config.output[output].speed_y; }

/* ==================================================== *
 * Timestamp
 * ==================================================== */

uint64_t hal_time_us_64(void) { return time_us_64(); }

/* ==================================================== *
 * Queue operations
 * ==================================================== */

void hal_queue_mouse_report(device_t *dev, const uint8_t *report) {
    queue_mouse_report((mouse_report_t *)report, dev);
}

void hal_queue_kbd_report(device_t *dev, const uint8_t *report) {
    queue_kbd_report((hid_keyboard_report_t *)report, dev);
}

void hal_queue_uart_packet(device_t *dev, const uint8_t *packet) {
    queue_try_add(&dev->uart_tx_queue, packet);
}

/* ==================================================== *
 * Hardware
 * ==================================================== */

void hal_watchdog_update(void) { watchdog_update(); }
void hal_blink_led(device_t *dev) { blink_led(dev); }

/* ==================================================== *
 * Trace output (used by Rust trace!() macro)
 * ==================================================== */

#ifdef DH_DEBUG
void hal_trace_write(const uint8_t *buf, uint32_t len) {
    dh_debug_printf("%.*s", (int)len, (const char *)buf);
}
#else
void hal_trace_write(const uint8_t *buf, uint32_t len) {
    (void)buf; (void)len;
}
#endif
