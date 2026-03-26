// Config mode API FFI — default config + thin FFI wrappers.
// Business logic (field map, read/write, API handling) lives in service::config_api.

use crate::domain::structs::*;
use crate::domain::constants::*;
use crate::service::config_api::{self, FIELDS, find_field};

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

// ============================================================
// FFI exports — thin wrappers over service::config_api
// ============================================================

#[export_name = "get_field_map_length"]
pub extern "C" fn rust_get_field_map_length() -> u32 {
    FIELDS.len() as u32
}

#[export_name = "hal_get_field_map_length"]
pub extern "C" fn rust_hal_get_field_map_length() -> u32 {
    FIELDS.len() as u32
}

#[export_name = "hal_get_field_map_idx"]
pub extern "C" fn rust_hal_get_field_map_idx(i: u32) -> u8 {
    let idx = if (i as usize) >= FIELDS.len() { FIELDS.len() - 1 } else { i as usize };
    FIELDS[idx].idx
}

#[export_name = "hal_get_field_map"]
pub unsafe extern "C" fn rust_hal_get_field_map(
    api_idx: u8, offset: *mut u32, len: *mut u32, readonly: *mut bool,
) -> i32 {
    match find_field(api_idx) {
        Some(f) => {
            // offset not used by Rust callers -- set to 0
            *offset = 0;
            *len = f.len as u32;
            *readonly = f.readonly;
            0
        }
        None => -1,
    }
}

#[export_name = "hal_api_read_field"]
pub unsafe extern "C" fn rust_hal_api_read_field(_offset: u32, _len: u32, _out: *mut u8) {
    // Legacy -- not used by Rust api_config path
}

#[export_name = "hal_api_write_field"]
pub unsafe extern "C" fn rust_hal_api_write_field(_offset: u32, _len: u32, _data: *const u8) {
    // Legacy -- not used by Rust api_config path
}

// Thin FFI wrappers — create PicoHal and delegate to service layer
pub unsafe fn handle_api_msg(ptype: u8, api_idx: u8, data: *const u8, dev: *mut core::ffi::c_void) {
    let state = crate::domain::structs::device_from_ptr(dev);
    let hal = crate::hal::pico::PicoHal::new(dev);
    config_api::handle_api_msg(state, &hal, ptype, api_idx, data);
}

pub unsafe fn handle_api_read_all(dev: *mut core::ffi::c_void) {
    let state = crate::domain::structs::device_from_ptr(dev);
    let hal = crate::hal::pico::PicoHal::new(dev);
    config_api::handle_api_read_all(state, &hal);
}
