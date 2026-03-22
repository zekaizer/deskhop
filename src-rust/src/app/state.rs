// Application state — Rust-owned, HAL-independent.
// This replaces the HAL-independent fields of C's device_t.
// C accesses this through a pointer provided by Rust.

use crate::app::constants::NUM_SCREENS;
use crate::app::hid_parser::ReportVal;
use crate::app::structs::{
    Config, FirmwareMetadata, FwUpgradeState, HidKeyboardReport, MouseReportC, UartPacketC,
    FLASH_PAGE_SIZE, MAX_DEVICES, MAX_CC_BUTTONS, MAX_KEYS, MAX_KEYBOARDS, MAX_SYS_BUTTONS,
};

/// Core application state — Rust owns this, C accesses via pointer.
/// All fields are HAL-independent (no queue_t, no DMA, no function pointers).
#[repr(C)]
pub struct AppState {
    // USB device addressing
    pub kbd_dev_addr: u8,
    pub kbd_instance: u8,

    // Per-screen state
    pub keyboard_leds: [u8; NUM_SCREENS],
    pub last_activity: [u64; NUM_SCREENS],
    pub core1_last_loop_pass: u64,

    // Output selection
    pub active_output: u8,
    pub board_role: u8,

    // Keyboard state (multi-device)
    pub local_kbd_states: [HidKeyboardReport; MAX_DEVICES],
    pub remote_kbd_state: HidKeyboardReport,
    pub max_kbd_idx: u8,

    // Mouse pointer
    pub pointer_x: i16,
    pub pointer_y: i16,
    pub mouse_buttons: i16,

    // Configuration (loaded from flash)
    pub config: Config,

    // Incoming UART packet buffer
    pub in_packet: UartPacketC,

    // DMA ring buffer pointer (used by packet_receiver_task)
    pub dma_ptr: u32,

    // Firmware upgrade state
    pub fw: FwUpgradeState,
    pub running_fw: FirmwareMetadata,
    pub reboot_requested: bool,
    pub config_mode_timer: u64,
    pub page_buffer: [u8; FLASH_PAGE_SIZE],

    // Connection status
    pub tud_connected: bool,
    pub keyboard_connected: bool,
    pub mouse_connected: bool,

    // Feature flags
    pub mouse_zoom: bool,
    pub switch_lock: bool,
    pub onboard_led_state: bool,
    pub relative_mouse: bool,
    pub gaming_mode: bool,
    pub config_mode_active: bool,
    pub digitizer_active: bool,

    // LED blinky feedback
    pub blinks_left: i32,
    pub last_led_change: i32,
}

impl AppState {
    /// Zero-initialize all fields
    pub const fn new() -> Self {
        Self {
            kbd_dev_addr: 0,
            kbd_instance: 0,
            keyboard_leds: [0; NUM_SCREENS],
            last_activity: [0; NUM_SCREENS],
            core1_last_loop_pass: 0,
            active_output: 0,
            board_role: 0,
            local_kbd_states: [HidKeyboardReport {
                modifier: 0,
                reserved: 0,
                keycode: [0; 6],
            }; MAX_DEVICES],
            remote_kbd_state: HidKeyboardReport {
                modifier: 0,
                reserved: 0,
                keycode: [0; 6],
            },
            max_kbd_idx: 0,
            pointer_x: 0,
            pointer_y: 0,
            mouse_buttons: 0,
            config: Config {
                magic_header: 0,
                version: 0,
                force_mouse_boot_mode: 0,
                force_kbd_boot_protocol: 0,
                kbd_led_as_indicator: 0,
                hotkey_toggle: 0,
                enable_acceleration: 0,
                enforce_ports: 0,
                jump_threshold: 0,
                output: [crate::app::structs::Output {
                    number: 0,
                    screen_count: 0,
                    screen_index: 0,
                    speed_x: 0,
                    speed_y: 0,
                    border: crate::app::structs::BorderSize { top: 0, bottom: 0 },
                    os: 0,
                    pos: 0,
                    mouse_park_pos: 0,
                    screensaver: crate::app::structs::Screensaver {
                        mode: 0,
                        only_if_inactive: 0,
                        idle_time_us: 0,
                        max_time_us: 0,
                    },
                }; NUM_SCREENS],
                _reserved: 0,
                checksum: 0,
            },
            in_packet: UartPacketC {
                ptype: 0,
                data: [0; 8],
                checksum: 0,
            },
            dma_ptr: 0,
            fw: FwUpgradeState {
                address: 0,
                checksum: 0,
                version: 0,
                byte_done: false,
                upgrade_in_progress: false,
            },
            running_fw: FirmwareMetadata {
                magic: 0,
                version: 0,
                checksum: 0,
            },
            reboot_requested: false,
            config_mode_timer: 0,
            page_buffer: [0; FLASH_PAGE_SIZE],
            tud_connected: false,
            keyboard_connected: false,
            mouse_connected: false,
            mouse_zoom: false,
            switch_lock: false,
            onboard_led_state: false,
            relative_mouse: false,
            gaming_mode: false,
            config_mode_active: false,
            digitizer_active: false,
            blinks_left: 0,
            last_led_change: 0,
        }
    }

    /// Check if current board is the active output
    pub fn is_active_output(&self) -> bool {
        self.active_output == self.board_role
    }
}

/// Global application state — Rust-owned.
static mut APP_STATE: AppState = AppState::new();

/// Get mutable pointer to the global app state.
/// Exported to C via hal/ffi/state.rs.
pub fn rust_get_app_state() -> *mut AppState {
    unsafe { &raw mut APP_STATE }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem;

    #[test]
    fn test_app_state_new_zeroed() {
        let state = AppState::new();
        assert_eq!(state.active_output, 0);
        assert_eq!(state.pointer_x, 0);
        assert!(!state.tud_connected);
        assert!(!state.gaming_mode);
    }

    #[test]
    fn test_is_active_output() {
        let mut state = AppState::new();
        state.board_role = 0;
        state.active_output = 0;
        assert!(state.is_active_output());

        state.active_output = 1;
        assert!(!state.is_active_output());
    }

    #[test]
    fn test_app_state_size_reasonable() {
        // AppState should be smaller than the full device_t since we removed
        // queue_t (4×16=64), hid_interface_t array (huge), DMA channels (12)
        let size = mem::size_of::<AppState>();
        assert!(size > 100, "Too small: {}", size);
        assert!(size < 2048, "Too large: {} — may include unwanted fields", size);
    }

    #[test]
    fn test_config_field_access() {
        let mut state = AppState::new();
        state.config.jump_threshold = 500;
        state.config.enable_acceleration = 1;
        state.config.output[0].speed_x = 16;
        state.config.output[0].speed_y = 28;

        assert_eq!(state.config.jump_threshold, 500);
        assert_eq!(state.config.output[0].speed_x, 16);
    }

    #[test]
    fn test_keyboard_state_multi_device() {
        let mut state = AppState::new();
        state.local_kbd_states[0].modifier = 0x01;
        state.local_kbd_states[0].keycode[0] = 0x04;
        state.local_kbd_states[1].modifier = 0x02;

        assert_eq!(state.local_kbd_states[0].modifier, 0x01);
        assert_eq!(state.local_kbd_states[1].modifier, 0x02);
    }
}
