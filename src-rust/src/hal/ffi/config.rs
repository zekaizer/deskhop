// Config mode API FFI — default config + thin FFI wrappers.
// Business logic (field map, read/write, API handling) lives in service::config_api.

use crate::domain::structs::*;
use crate::domain::constants::*;

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
    passthrough_enabled: 1,
    gaming_mode_default: 0,
    _reserved: 0,
    smartshift_double_click_ms: 500,
    checksum: 0,
};

