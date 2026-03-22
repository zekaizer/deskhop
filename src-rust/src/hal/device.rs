// WORKAROUND(c-compat): device_t contains Pico SDK types (queue_t) and
// TinyUSB types (hid_interface_t) that cannot be represented in Rust.
// We use an opaque pointer with C getter/setter FFI functions instead of
// mirroring the full struct. This avoids layout sync issues.

use core::ffi::c_void;

/// Opaque handle to the C device_t struct.
/// All access goes through extern "C" getter/setter functions.
#[repr(transparent)]
pub struct DeviceHandle {
    ptr: *mut c_void,
}

impl DeviceHandle {
    /// Create from a raw C pointer. Caller must ensure the pointer is valid
    /// for the lifetime of this handle.
    pub unsafe fn from_raw(ptr: *mut c_void) -> Self {
        Self { ptr }
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.ptr
    }
}

// Device state accessors — these will be implemented as extern "C" in the
// C HAL shim layer. Each function takes a device_t* and returns/sets a value.

extern "C" {
    // Read-only state
    pub fn hal_get_active_output(dev: *mut c_void) -> u8;
    pub fn hal_get_board_role(dev: *mut c_void) -> u8;
    pub fn hal_get_pointer_x(dev: *mut c_void) -> i16;
    pub fn hal_get_pointer_y(dev: *mut c_void) -> i16;
    pub fn hal_get_mouse_buttons(dev: *mut c_void) -> i16;
    pub fn hal_get_core1_last_loop_pass(dev: *mut c_void) -> u64;

    // Writable state
    pub fn hal_set_active_output(dev: *mut c_void, output: u8);
    pub fn hal_set_pointer_x(dev: *mut c_void, x: i16);
    pub fn hal_set_pointer_y(dev: *mut c_void, y: i16);
    pub fn hal_set_mouse_buttons(dev: *mut c_void, buttons: i16);
    pub fn hal_set_core1_last_loop_pass(dev: *mut c_void, timestamp: u64);

    // Feature flags
    pub fn hal_get_mouse_zoom(dev: *mut c_void) -> bool;
    pub fn hal_get_switch_lock(dev: *mut c_void) -> bool;
    pub fn hal_get_gaming_mode(dev: *mut c_void) -> bool;
    pub fn hal_get_relative_mouse(dev: *mut c_void) -> bool;
    pub fn hal_get_tud_connected(dev: *mut c_void) -> bool;
    pub fn hal_get_reboot_requested(dev: *mut c_void) -> bool;
    pub fn hal_get_config_mode_active(dev: *mut c_void) -> bool;

    // Config access
    pub fn hal_get_jump_threshold(dev: *mut c_void) -> u16;
    pub fn hal_get_enable_acceleration(dev: *mut c_void) -> bool;
    pub fn hal_get_speed_x(dev: *mut c_void, output: u8) -> i32;
    pub fn hal_get_speed_y(dev: *mut c_void, output: u8) -> i32;

    // Timestamp
    pub fn hal_time_us_64() -> u64;

    // Queue operations
    pub fn hal_queue_mouse_report(dev: *mut c_void, report: *const u8);
    pub fn hal_queue_kbd_report(dev: *mut c_void, report: *const u8);
    pub fn hal_queue_uart_packet(dev: *mut c_void, packet: *const u8);

    // Hardware
    pub fn hal_watchdog_update();
    pub fn hal_blink_led(dev: *mut c_void);
}

/// Configuration subset that Rust needs — read from C via FFI at the
/// point of use, not cached.
pub struct OutputConfig {
    pub speed_x: i32,
    pub speed_y: i32,
    pub border_top: i32,
    pub border_bottom: i32,
    pub os: u8,
    pub pos: u8,
    pub screen_count: u32,
    pub screen_index: u32,
    pub mouse_park_pos: u8,
}
