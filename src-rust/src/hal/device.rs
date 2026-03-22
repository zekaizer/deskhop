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

    // ---- UART send helpers ----
    pub fn hal_send_value(value: u8, packet_type: u8);
    pub fn hal_queue_packet(data: *const u8, packet_type: u8, length: i32);

    // ---- Config / Flash ----
    pub fn hal_save_config(dev: *mut c_void);
    pub fn hal_load_config(dev: *mut c_void);
    pub fn hal_wipe_config();

    // ---- Output switching / LEDs ----
    pub fn hal_set_active_output(dev: *mut c_void, output: u8);
    pub fn hal_restore_leds(dev: *mut c_void);
    pub fn hal_release_all_keys(dev: *mut c_void);

    // ---- Hardware ----
    pub fn hal_watchdog_update();
    pub fn hal_blink_led(dev: *mut c_void);
    pub fn hal_reboot();
    pub fn hal_reset_usb_boot();

    // ---- TinyUSB ----
    pub fn hal_tud_ready() -> bool;
    pub fn hal_tud_connected() -> bool;
    pub fn hal_tud_suspended() -> bool;
    pub fn hal_tud_remote_wakeup();
    pub fn hal_tud_hid_n_ready(instance: u8) -> bool;
    pub fn hal_tud_hid_keyboard_report(report_id: u8, modifier: u8, keycode: *const u8) -> bool;
    pub fn hal_tud_mouse_report(mode: u8, buttons: u8, x: i16, y: i16, wheel: i8, pan: i8) -> bool;

    // ---- HID interface manipulation ----
    /// Call C extract_data to populate hid_interface_t from a ReportVal
    pub fn hal_extract_data(iface: *mut c_void, val: *const u8);
    /// Set uses_report_id on hid_interface_t
    pub fn hal_iface_set_uses_report_id(iface: *mut c_void, val: bool);

    // ---- Mouse descriptor fields ----
    pub fn hal_get_mouse_move_x_val(iface: *mut c_void) -> *const u8;
    pub fn hal_get_mouse_move_y_val(iface: *mut c_void) -> *const u8;
    pub fn hal_get_mouse_wheel_val(iface: *mut c_void) -> *const u8;
    pub fn hal_get_mouse_pan_val(iface: *mut c_void) -> *const u8;
    pub fn hal_get_mouse_buttons_val(iface: *mut c_void) -> *const u8;
    pub fn hal_get_mouse_buttons_report_id(iface: *mut c_void) -> u8;

    // ---- Keyboard descriptor fields ----
    pub fn hal_get_kbd_modifier_offset_idx(iface: *mut c_void, report_id: u8) -> u16;
    pub fn hal_get_kbd_modifier_size(iface: *mut c_void, report_id: u8) -> u16;
    pub fn hal_get_kbd_key_array(iface: *mut c_void, report_id: u8, index: i32) -> bool;
    pub fn hal_get_kbd_is_nkro(iface: *mut c_void, report_id: u8) -> bool;
    pub fn hal_get_kbd_nkro_offset_idx(iface: *mut c_void, report_id: u8) -> u16;
    pub fn hal_get_kbd_nkro_usage_min(iface: *mut c_void, report_id: u8) -> i32;
    pub fn hal_get_kbd_nkro_usage_max(iface: *mut c_void, report_id: u8) -> i32;
    pub fn hal_get_kbd_nkro_size(iface: *mut c_void, report_id: u8) -> u16;
    pub fn hal_get_iface_uses_report_id(iface: *mut c_void) -> bool;
    pub fn hal_get_iface_protocol(iface: *mut c_void) -> u8;

    // ---- Consumer control ----
    pub fn hal_get_consumer_is_variable(iface: *mut c_void) -> bool;
    pub fn hal_get_cc_array_value(iface: *mut c_void, report_id: u8, index: i32) -> u16;

    // ---- Mouse report extraction ----
    /// Extract mouse values from raw HID report. out is 5×i32.
    pub fn hal_extract_report_values(
        raw_report: *mut u8, len: i32, dev: *mut c_void,
        iface: *mut c_void, out: *mut i32,
    );

    // ---- HID report extraction ----
    pub fn hal_extract_kbd_data(
        raw_report: *mut u8, len: i32, itf: u8,
        iface: *mut c_void, out_report: *mut u8,
    ) -> i32;

    /// Check hotkeys — returns -1 if no match, 0 if matched (and handler called).
    /// out_pass_to_os and out_acknowledge are set if matched.
    pub fn hal_check_all_hotkeys(
        report: *const u8, out_pass_to_os: *mut u8, out_acknowledge: *mut u8,
    ) -> i32;

    pub fn hal_toggle_led();
    pub fn hal_set_config_mode_scratch();

    // ---- DMA ----
    pub fn hal_dma_channel_is_busy(dev: *mut c_void) -> bool;
    pub fn hal_dma_tx_send(dev: *mut c_void, buf: *const u8, len: u32);
    pub fn hal_dma_rx_remaining(dev: *mut c_void) -> u32;
    pub fn hal_is_start_of_packet(dev: *mut c_void) -> bool;
    pub fn hal_fetch_packet(dev: *mut c_void);
}
