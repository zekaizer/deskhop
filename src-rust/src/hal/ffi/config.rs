// Config mode API FFI — default config + thin FFI wrappers.
// Business logic (field map, read/write, API handling) lives in service::config_api.

use crate::domain::structs::*;
use crate::domain::constants::*;
use crate::service::config_api;

// ============================================================
// Default config (replaces C defaults.c)
// ============================================================

#[export_name = "default_config"]
pub static DEFAULT_CONFIG: Config = Config {
    magic_header: 0xB00B1E5,
    version: 8, // CURRENT_CONFIG_VERSION
    force_mouse_boot_mode: 0,
    force_kbd_boot_protocol: 0, // ENFORCE_KEYBOARD_BOOT_PROTOCOL
    kbd_led_as_indicator: 0,    // KBD_LED_AS_INDICATOR
    hotkey_toggle: HID_KEY_CAPS_LOCK, // HOTKEY_TOGGLE
    enable_acceleration: 1,     // ENABLE_ACCELERATION
    enforce_ports: 0,           // ENFORCE_PORTS
    jump_threshold: 0,          // JUMP_THRESHOLD
    output: [
        Output {
            number: 0, // OUTPUT_A
            speed_x: 16, // MOUSE_SPEED_A_FACTOR_X
            speed_y: 28, // MOUSE_SPEED_A_FACTOR_Y
            border: BorderSize { top: 0, bottom: MAX_SCREEN_COORD as i32 },
            screen_count: 1,
            screen_index: 1,
            os: OS_MACOS, // OUTPUT_A_OS
            pos: 2, // RIGHT
            mouse_park_pos: 0,
            screensaver: Screensaver {
                mode: 0, // DISABLED
                only_if_inactive: 0,
                idle_time_us: 240 * 1_000_000, // SCREENSAVER_A_IDLE_TIME_SEC
                max_time_us: 0,
            },
        },
        Output {
            number: 1, // OUTPUT_B
            speed_x: 16, // MOUSE_SPEED_B_FACTOR_X
            speed_y: 28, // MOUSE_SPEED_B_FACTOR_Y
            border: BorderSize { top: 0, bottom: MAX_SCREEN_COORD as i32 },
            screen_count: 1,
            screen_index: 1,
            os: OS_LINUX, // OUTPUT_B_OS
            pos: 1, // LEFT
            mouse_park_pos: 0,
            screensaver: Screensaver {
                mode: 0,
                only_if_inactive: 0,
                idle_time_us: 240 * 1_000_000,
                max_time_us: 0,
            },
        },
    ],
    _reserved: 0,
    checksum: 0,
};

// Thin FFI wrappers — create PicoHal and delegate to service layer.
// Unsafe boundary: convert raw C pointers to slices/references here.
pub unsafe fn handle_api_msg(ptype: u8, api_idx: u8, data: *const u8, dev: *mut core::ffi::c_void) {
    let state = crate::domain::structs::device_from_ptr(dev);
    let hal = crate::hal::pico::PicoHal::new(dev);
    let data_slice = core::slice::from_raw_parts(data, 8);
    config_api::handle_api_msg(state, &hal, ptype, api_idx, data_slice);
}

pub unsafe fn handle_api_read_all(dev: *mut core::ffi::c_void) {
    let state = crate::domain::structs::device_from_ptr(dev);
    let hal = crate::hal::pico::PicoHal::new(dev);
    config_api::handle_api_read_all(state, &hal);
}
