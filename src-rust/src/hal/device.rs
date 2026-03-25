// HAL device interface — all hardware-dependent operations.
// Rust calls these thin C wrappers for Pico SDK / TinyUSB / GPIO access.

use core::ffi::c_void;

extern "C" {
    // ---- Timestamp ----
    pub fn hal_time_us_64() -> u64;
    pub fn hal_time_us_32() -> u32;

    // ---- Queue operations ----
    pub fn hal_queue_mouse_report(dev: *mut c_void, report: *const u8);
    pub fn hal_queue_kbd_report(dev: *mut c_void, report: *const u8);
    pub fn hal_queue_uart_packet(dev: *mut c_void, packet: *const u8);
    pub fn hal_queue_try_add_uart(dev: *mut c_void, data: *const u8) -> bool;

    // ---- HID queue helpers ----
    pub fn hal_queue_cc_packet(dev: *mut c_void, payload: *const u8);
    pub fn hal_queue_system_packet(dev: *mut c_void, payload: *const u8);

    // ---- UART send helpers (direct C functions) ----
    pub fn send_value(value: u8, packet_type: u8);
    pub fn queue_packet(data: *const u8, packet_type: u8, length: i32);

    // ---- Config / Flash (direct C functions) ----
    pub fn save_config(dev: *mut c_void);
    pub fn load_config(dev: *mut c_void);
    pub fn wipe_config();

    // ---- Output switching / LEDs (direct C functions) ----
    pub fn set_active_output(dev: *mut c_void, output: u8);
    pub fn restore_leds(dev: *mut c_void);
    // release_all_keys is now a Rust #[export_name] in keyboard.rs

    // ---- Hardware ----
    pub fn hal_watchdog_update();
    pub fn blink_led(dev: *mut c_void);
    pub fn reboot();
    pub fn hal_reset_usb_boot();

    // ---- TinyUSB (via hal_shim.c — TinyUSB functions are inline/macro) ----
    pub fn hal_tud_ready() -> bool;
    pub fn hal_tud_suspended() -> bool;
    pub fn hal_tud_remote_wakeup();
    pub fn hal_tud_hid_n_ready(instance: u8) -> bool;
    pub fn hal_tud_hid_keyboard_report(report_id: u8, modifier: u8, keycode: *const u8) -> bool;
    pub fn hal_tud_mouse_report(mode: u8, buttons: u8, x: i16, y: i16, wheel: i8, pan: i8) -> bool;

    // ---- hid_interface_t (remaining C-dependent function) ----
    /// Assigns C function pointers (process_*_report) to report_handler array
    pub fn hal_set_report_handler(iface: *mut c_void, report_id: u8, handler_type: u8);

    pub fn hal_toggle_led();
    pub fn hal_read_fw_running_u32(address: u32) -> u32;
    pub fn hal_kbd_queue_peek(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_kbd_queue_remove(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_mouse_queue_peek(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_mouse_queue_remove(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_uart_tx_queue_remove(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_set_config_mode_scratch();

    // ---- Debug ----
    pub fn hal_debug_blink(count: i32, delay_ms: i32);
    pub fn hal_debug_dump_state(dev: *mut c_void);

    // ---- Pico SDK direct ----
    pub fn watchdog_update();
    pub fn hal_queue_cfg_packet(dev: *mut c_void, packet: *const u8);

    // ---- DMA ----
    pub fn hal_dma_channel_is_busy(dev: *mut c_void) -> bool;
    pub fn hal_dma_tx_send(dev: *mut c_void, buf: *const u8, len: u32);
    pub fn hal_dma_rx_remaining(dev: *mut c_void) -> u32;
    pub fn hal_is_start_of_packet(dev: *mut c_void) -> bool;
    pub fn hal_fetch_packet(dev: *mut c_void);
}
