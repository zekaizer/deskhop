// Core data structures mirroring C's structs.h, screen.h, flash.h, packet.h
// These must maintain exact layout compatibility with C (#[repr(C)]).

use crate::domain::constants::{NUM_SCREENS, PACKET_DATA_LENGTH, RAW_PACKET_LENGTH};
use crate::domain::hid_parser::ReportVal;

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

#[derive(Clone, Copy, Default)]
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

#[derive(Clone, Copy, Default)]
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

#[derive(Clone, Copy, Default)]
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
 * Pico SDK queue_t — opaque, size varies by SDK version.
 * We represent it as a fixed-size blob to maintain layout.
 * Validated by _Static_assert in sdk_verify.h and build.rs (bindgen).
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
 *
 * # Core Ownership Model (RP2040 dual-core)
 *
 * Core0 runs USB device tasks + main loop (keyboard/mouse pipelines,
 * hotkey dispatch, config API, UART TX).
 * Core1 runs USB host tasks + peripheral loop (packet RX, LED, screensaver,
 * firmware upgrade, heartbeat).
 *
 * Field annotations:
 *   [C0]     = Core0-exclusive write (USB callbacks, kbd/mouse pipeline)
 *   [C1]     = Core1-exclusive write (host tasks, LED task)
 *   [Shared] = Cross-core read; writer listed first
 *   [Init]   = Written once at startup, then read-only
 *
 * Cross-core u8/bool/i16 fields are naturally atomic on Cortex-M0+
 * (aligned single-byte or halfword access). u64 fields are NOT atomic
 * — torn reads are possible but tolerable (see SAFETY comments in
 * lib.rs and service/tasks.rs).
 * ================================================================== */

#[repr(C)]
pub struct Device {
    pub kbd_dev_addr: u8,                                       // [C0] USB host mount
    pub kbd_instance: u8,                                       // [C0] USB host mount

    pub keyboard_leds: [u8; NUM_SCREENS],                       // [C0] msg_bridge
    pub last_activity: [u64; NUM_SCREENS],                      // [Shared] C0 writes, C1 reads (screensaver)
    pub core1_last_loop_pass: u64,                              // [Shared] C1 writes, C0 reads (health check)
    pub active_output: u8,                                      // [Shared] C0 writes, C1 reads (atomic u8)
    pub board_role: u8,                                         // [Init]

    pub local_kbd_states: [HidKeyboardReport; MAX_DEVICES],     // [C0] kbd_pipeline
    pub remote_kbd_state: HidKeyboardReport,                    // [C0] msg_bridge
    pub max_kbd_idx: u8,                                        // [C0] kbd_pipeline

    pub pointer_x: i16,                                         // [Shared] C0 writes, C1 reads (atomic i16)
    pub pointer_y: i16,                                         // [Shared] C0 writes, C1 reads (atomic i16)
    pub mouse_buttons: i16,                                     // [C0] mouse_pipeline

    pub config: Config,                                         // [C0] config_api, hotkey_dispatch
    pub hid_queue_out: QueueOpaque,                             // [C0] queue ops (Pico SDK thread-safe)
    pub kbd_queue: QueueOpaque,                                 // [C0] queue ops (Pico SDK thread-safe)
    pub mouse_queue: QueueOpaque,                               // [C0] queue ops (Pico SDK thread-safe)
    pub uart_tx_queue: QueueOpaque,                             // [C0] queue ops (Pico SDK thread-safe)

    pub iface: [[HidInterface; MAX_INTERFACES]; MAX_DEVICES],   // [C0] HID parser, extract_data
    pub in_packet: UartPacketC,                                 // [C1] packet_receiver_task

    // DMA — accessed only by C code on respective cores
    pub dma_ptr: u32,                                           // [C1] DMA ring buffer
    pub dma_rx_channel: u32,                                    // [Init]
    pub dma_control_channel: u32,                               // [Init]
    pub dma_tx_channel: u32,                                    // [Init]

    // Firmware
    pub fw: FwUpgradeState,                                     // [C0] fw_upgrade service
    pub running_fw: FirmwareMetadata,                           // [Init]
    pub reboot_requested: bool,                                 // [C0] hotkey_dispatch
    pub config_mode_timer: u64,                                 // [C0] heartbeat task

    pub page_buffer: [u8; FLASH_PAGE_SIZE],                     // [C0] fw_upgrade

    // Connection status
    pub usb_connected: bool,                                    // [C0] USB callbacks (atomic bool)
    pub keyboard_connected: bool,                               // [C0] USB callbacks
    pub mouse_connected: bool,                                  // [C0] USB callbacks

    // Feature flags
    pub mouse_zoom: bool,                                       // [C0] hotkey_dispatch
    pub switch_lock: bool,                                      // [C0] hotkey_dispatch
    pub onboard_led_state: bool,                                // [C1] LED task
    pub relative_mouse: bool,                                   // [C0] hotkey_dispatch
    pub gaming_mode: bool,                                      // [C0] hotkey_dispatch
    pub config_mode_active: bool,                               // [C0] hotkey_dispatch
    pub digitizer_active: bool,                                 // [C0] hotkey_dispatch

    // LED blinky
    pub blinks_left: i32,                                       // [C1] LED task
    pub last_led_change: i32,                                   // [C1] LED task
}

impl Device {
    pub fn is_active_output(&self) -> bool {
        self.active_output == self.board_role
    }

    /// Create a zeroed Device for testing. All fields zero/false/null.
    #[cfg(test)]
    pub fn zeroed() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

/// Cast a C device_t* pointer to a Rust Device reference.
/// SAFETY: caller must ensure ptr is valid and layout matches.
pub unsafe fn device_from_ptr<'a>(dev: *mut core::ffi::c_void) -> &'a mut Device {
    &mut *(dev as *mut Device)
}

/// Cast a C hid_interface_t* pointer to a Rust HidInterface reference.
/// SAFETY: caller must ensure ptr is valid and layout matches.
pub unsafe fn iface_from_ptr<'a>(iface: *mut core::ffi::c_void) -> &'a mut HidInterface {
    &mut *(iface as *mut HidInterface)
}

/// Look up keyboard descriptor by report_id (mirrors C get_keyboard).
pub fn get_keyboard(iface: &HidInterface, rid: u8) -> &KeyboardDescriptor {
    if iface.num_keyboards == 1 || !iface.uses_report_id {
        return &iface.keyboards[0];
    }
    for n in 0..iface.num_keyboards as usize {
        if n < MAX_KEYBOARDS && iface.keyboards[n].report_id == rid {
            return &iface.keyboards[n];
        }
    }
    &iface.keyboards[0]
}

// Global device pointer — set once during rust_main_loop entry.
// Uses AtomicPtr for Rust 2024 edition compatibility (static mut deprecated).
use core::sync::atomic::{AtomicPtr, Ordering};
static GLOBAL_DEVICE_PTR: AtomicPtr<Device> = AtomicPtr::new(core::ptr::null_mut());

/// Store the device pointer for functions that can't receive it as a parameter.
/// Must be called exactly once with a valid device_t* at startup.
pub fn set_global_device(dev: *mut core::ffi::c_void) {
    GLOBAL_DEVICE_PTR.store(dev as *mut Device, Ordering::Release);
}

/// Get device reference from the stored global pointer.
///
/// # Safety
/// - `set_global_device` must have been called first with a valid device_t*.
/// - **Core0 only.** All current call sites are USB callbacks on Core0.
///   Core1 receives its device pointer via the `dev` parameter in rust_core1_loop.
///   Calling from Core1 would create aliased `&mut Device`, which is UB.
pub unsafe fn get_global_device<'a>() -> &'a mut Device {
    &mut *GLOBAL_DEVICE_PTR.load(Ordering::Acquire)
}

// Compile-time layout verification — C offsets generated by build.rs (bindgen).
// Source of truth: src/include/structs.h. Build fails if Rust struct doesn't match.
#[cfg(target_arch = "arm")]
mod layout_verify {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/device_offsets.rs"));
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

    #[test]
    fn test_intermediate_struct_sizes() {
        use crate::domain::hid_parser::ReportVal;

        // ReportVal must be packed = 23 bytes
        assert_eq!(mem::size_of::<ReportVal>(), 23, "ReportVal");

        // Structs containing packed ReportVal
        assert_eq!(mem::size_of::<MouseDescriptor>(), 118, "MouseDescriptor");
        assert_eq!(mem::size_of::<KeyboardDescriptor>(), 132, "KeyboardDescriptor");
        assert_eq!(mem::size_of::<ReportDescriptor>(), 26, "ReportDescriptor");

        // HidInterface (contains fn ptrs — size differs 32-bit vs 64-bit)
        // On x86_64 test: ptrs are 8 bytes so HidInterface will be larger than ARM
        // On ARM32: ptrs are 4 bytes, HidInterface should be 932
        let ptr_size = mem::size_of::<usize>();
        let hid_iface_size = mem::size_of::<HidInterface>();
        let fn_ptr_size = mem::size_of::<ProcessReportFn>();
        assert_eq!(fn_ptr_size, ptr_size, "ProcessReportFn should be pointer-sized");
        // On ARM32 (4-byte ptrs): HidInterface = 932
        if ptr_size == 4 {
            assert_eq!(hid_iface_size, 932, "HidInterface on 32-bit");
        }
    }

    #[test]
    fn test_config_struct_sizes() {
        // Screensaver: mode(1) + only_if_inactive(1) + pad(6) + idle(8) + max(8) = 24
        assert_eq!(mem::size_of::<Screensaver>(), 24, "Screensaver");

        // FwUpgradeState: address(4) + checksum(4) + version(2) + byte_done(1) + upgrade(1) = 12
        assert_eq!(mem::size_of::<FwUpgradeState>(), 12, "FwUpgradeState");

        // FirmwareMetadata: magic(4) + version(2) + pad(2) + checksum(4) = 12
        assert_eq!(mem::size_of::<FirmwareMetadata>(), 12, "FirmwareMetadata");

        // QueueOpaque: 16 bytes
        assert_eq!(mem::size_of::<QueueOpaque>(), 16, "QueueOpaque");
    }

    #[test]
    fn test_report_val_field_offsets() {
        use crate::domain::hid_parser::ReportVal;
        // Verify packed layout matches C report_val_t
        assert_eq!(mem::offset_of!(ReportVal, offset), 0);
        assert_eq!(mem::offset_of!(ReportVal, offset_idx), 2);
        assert_eq!(mem::offset_of!(ReportVal, size), 4);
        assert_eq!(mem::offset_of!(ReportVal, usage_min), 6);
        assert_eq!(mem::offset_of!(ReportVal, usage_max), 10);
        assert_eq!(mem::offset_of!(ReportVal, item_type), 14);
        assert_eq!(mem::offset_of!(ReportVal, data_type), 15);
        assert_eq!(mem::offset_of!(ReportVal, report_id), 16);
        assert_eq!(mem::offset_of!(ReportVal, global_usage), 17);
        assert_eq!(mem::offset_of!(ReportVal, usage_page), 19);
        assert_eq!(mem::offset_of!(ReportVal, usage), 21);
    }
}
