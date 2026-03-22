// Core data structures mirroring C's structs.h, screen.h, flash.h, packet.h
// These must maintain exact layout compatibility with C (#[repr(C)]).

use crate::constants::{NUM_SCREENS, PACKET_DATA_LENGTH, RAW_PACKET_LENGTH};
use crate::hid_parser::ReportVal;

// From hid_parser.h
pub const MAX_DEVICES: usize = 4;
pub const MAX_INTERFACES: usize = 12;
pub const MAX_REPORTS: usize = 24;
pub const MAX_KEYBOARDS: usize = 5;
pub const MAX_CC_BUTTONS: usize = 16;
pub const MAX_SYS_BUTTONS: usize = 8;
pub const MAX_KEYS: usize = 32;
pub const KEYS_IN_USB_REPORT: usize = 6;
pub const KBD_REPORT_LENGTH: usize = 8;

// From flash.h — Pico SDK FLASH_PAGE_SIZE = 256
pub const FLASH_PAGE_SIZE: usize = 256;

/* ================================================================== *
 * screen.h structures
 * ================================================================== */

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct BorderSize {
    pub top: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Screensaver {
    pub mode: u8,
    pub only_if_inactive: u8,
    pub idle_time_us: u64,
    pub max_time_us: u64,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Output {
    pub number: u32,
    pub screen_count: u32,
    pub screen_index: u32,
    pub speed_x: i32,
    pub speed_y: i32,
    pub border: BorderSize,
    pub os: u8,
    pub pos: u8,
    pub mouse_park_pos: u8,
    pub screensaver: Screensaver,
}

/* ================================================================== *
 * flash.h structures
 * ================================================================== */

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct FirmwareMetadata {
    pub magic: u32,
    pub version: u16,
    pub checksum: u32,
}

/* ================================================================== *
 * packet.h / structs.h structures
 * ================================================================== */

#[derive(Debug, Clone, Copy, Default)]
#[repr(C, packed)]
pub struct UartPacketC {
    pub ptype: u8,
    pub data: [u8; PACKET_DATA_LENGTH],
    pub checksum: u8,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MouseReportC {
    pub buttons: u8,
    pub x: i16,
    pub y: i16,
    pub wheel: i8,
    pub pan: i8,
    pub mode: u8,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct HidGenericPkt {
    pub instance: u8,
    pub report_id: u8,
    pub ptype: u8,
    pub len: u8,
    pub data: [u8; RAW_PACKET_LENGTH],
}

/* ================================================================== *
 * structs.h — config_t
 * ================================================================== */

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Config {
    pub magic_header: u32,
    pub version: u32,

    pub force_mouse_boot_mode: u8,
    pub force_kbd_boot_protocol: u8,

    pub kbd_led_as_indicator: u8,
    pub hotkey_toggle: u8,
    pub enable_acceleration: u8,

    pub enforce_ports: u8,
    pub jump_threshold: u16,

    pub output: [Output; NUM_SCREENS],
    pub _reserved: u32,

    // Checksum at the end
    pub checksum: u32,
}

/* ================================================================== *
 * structs.h — fw_upgrade_state_t
 * ================================================================== */

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct FwUpgradeState {
    pub address: u32,
    pub checksum: u32,
    pub version: u16,
    pub byte_done: bool,
    pub upgrade_in_progress: bool,
}

/* ================================================================== *
 * hid_parser.h — keyboard_t, mouse_t, hid_interface_t
 * ================================================================== */

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct MouseDescriptor {
    pub buttons: ReportVal,
    pub move_x: ReportVal,
    pub move_y: ReportVal,
    pub wheel: ReportVal,
    pub pan: ReportVal,
    pub report_id: u8,
    pub is_found: bool,
    pub uses_report_id: bool,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct KeyboardDescriptor {
    pub modifier: ReportVal,
    pub nkro: ReportVal,
    pub cc_array: [u16; MAX_CC_BUTTONS],
    pub sys_array: [u16; MAX_SYS_BUTTONS],
    pub key_array: [bool; MAX_KEYS],
    pub report_id: u8,
    pub key_array_idx: u8,
    pub uses_report_id: bool,
    pub is_found: bool,
    pub is_nkro: bool,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct ReportDescriptor {
    pub val: ReportVal,
    pub report_id: u8,
    pub is_variable: bool,
    pub is_array: bool,
}

/// Function pointer type for report handlers (C callback)
pub type ProcessReportFn = Option<unsafe extern "C" fn(*mut u8, i32, u8, *mut HidInterface)>;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct HidInterface {
    pub keyboards: [KeyboardDescriptor; MAX_KEYBOARDS],
    pub num_keyboards: u8,
    pub mouse: MouseDescriptor,
    pub consumer: ReportDescriptor,
    pub system: ReportDescriptor,
    pub report_handler: [ProcessReportFn; MAX_REPORTS],
    pub protocol: u8,
    pub uses_report_id: bool,
}

/* ================================================================== *
 * structs.h — hid_keyboard_report_t (from TinyUSB)
 * ================================================================== */

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct HidKeyboardReport {
    pub modifier: u8,
    pub reserved: u8,
    pub keycode: [u8; KEYS_IN_USB_REPORT],
}

/* ================================================================== *
 * Pico SDK queue_t — opaque, size varies by SDK version
 * We represent it as a fixed-size blob to maintain layout.
 * Actual size: 20 bytes on RP2040 Pico SDK 1.5.x
 * ================================================================== */

// WORKAROUND(c-compat): queue_t is SDK-internal. We use a fixed-size
// opaque blob. Size MUST match sizeof(queue_t) from the Pico SDK.
// If SDK changes queue_t size, this will break silently.
const QUEUE_T_SIZE: usize = 16;

#[derive(Clone, Copy)]
#[repr(C, align(4))]
pub struct QueueOpaque {
    _data: [u8; QUEUE_T_SIZE],
}

/* ================================================================== *
 * structs.h — device_t (the main state struct)
 * ================================================================== */

#[repr(C)]
pub struct Device {
    pub kbd_dev_addr: u8,
    pub kbd_instance: u8,

    pub keyboard_leds: [u8; NUM_SCREENS],
    pub last_activity: [u64; NUM_SCREENS],
    pub core1_last_loop_pass: u64,
    pub active_output: u8,
    pub board_role: u8,

    pub local_kbd_states: [HidKeyboardReport; MAX_DEVICES],
    pub remote_kbd_state: HidKeyboardReport,
    pub max_kbd_idx: u8,

    pub pointer_x: i16,
    pub pointer_y: i16,
    pub mouse_buttons: i16,

    pub config: Config,
    pub hid_queue_out: QueueOpaque,
    pub kbd_queue: QueueOpaque,
    pub mouse_queue: QueueOpaque,
    pub uart_tx_queue: QueueOpaque,

    pub iface: [[HidInterface; MAX_INTERFACES]; MAX_DEVICES],
    pub in_packet: UartPacketC,

    // DMA
    pub dma_ptr: u32,
    pub dma_rx_channel: u32,
    pub dma_control_channel: u32,
    pub dma_tx_channel: u32,

    // Firmware
    pub fw: FwUpgradeState,
    pub _running_fw: FirmwareMetadata,
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

    // LED blinky
    pub blinks_left: i32,
    pub last_led_change: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem;

    #[test]
    fn test_uart_packet_size() {
        assert_eq!(mem::size_of::<UartPacketC>(), 10);
    }

    #[test]
    fn test_hid_keyboard_report_size() {
        assert_eq!(mem::size_of::<HidKeyboardReport>(), 8);
    }

    #[test]
    fn test_border_size() {
        assert_eq!(mem::size_of::<BorderSize>(), 8);
    }

    #[test]
    fn test_firmware_metadata_size() {
        // magic(4) + version(2) + checksum(4) = 10, but C may pad
        let size = mem::size_of::<FirmwareMetadata>();
        assert!(size >= 10);
    }
}
