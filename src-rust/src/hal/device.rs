// HAL device interface — only hardware-dependent operations.
// Application state fields are in app::state::AppState (Rust-owned).
// This module contains only extern "C" functions for hardware access
// that cannot be done from pure Rust.

use core::ffi::c_void;

extern "C" {
    // Pico SDK timestamp
    pub fn hal_time_us_64() -> u64;

    // Queue operations (Pico SDK queue_t)
    pub fn hal_queue_mouse_report(dev: *mut c_void, report: *const u8);
    pub fn hal_queue_kbd_report(dev: *mut c_void, report: *const u8);
    pub fn hal_queue_uart_packet(dev: *mut c_void, packet: *const u8);

    // Hardware
    pub fn hal_watchdog_update();
    pub fn hal_blink_led(dev: *mut c_void);
}
