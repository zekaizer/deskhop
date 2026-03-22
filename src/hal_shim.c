/*
 * C HAL shim layer — only hardware-dependent operations.
 * Application state is now in Rust's AppState (src-rust/src/app/state.rs).
 * This file contains only functions that require Pico SDK / TinyUSB / GPIO.
 */

#include "main.h"

/* ==================================================== *
 * Layout verification
 * ==================================================== */

_Static_assert(sizeof(queue_t) == 16,
    "queue_t size changed — update QUEUE_T_SIZE in src-rust/src/app/structs.rs");

/* ==================================================== *
 * Pico SDK timestamp
 * ==================================================== */

uint64_t hal_time_us_64(void) { return time_us_64(); }

/* ==================================================== *
 * Queue operations (Pico SDK queue_t in device_t)
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
 * Hardware (GPIO, watchdog)
 * ==================================================== */

void hal_watchdog_update(void) { watchdog_update(); }
void hal_blink_led(device_t *dev) { blink_led(dev); }

/* ==================================================== *
 * Trace output (Rust trace!() macro backend)
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
