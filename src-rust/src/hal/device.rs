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

    // ---- hid_interface_t (remaining C-dependent functions) ----
    pub fn hal_set_report_handler(iface: *mut c_void, report_id: u8, handler_type: u8);
    pub fn hal_handle_keyboard_descriptor(iface: *mut c_void, val: *const u8);
    pub fn hal_handle_consumer_control_values(iface: *mut c_void, val: *const u8);

    // ---- HID hotkey check ----
    /// Check hotkeys — returns -1 if no match, 0 if matched (and handler called).
    /// out_pass_to_os and out_acknowledge are set if matched.
    pub fn hal_check_all_hotkeys(
        report: *const u8, out_pass_to_os: *mut u8, out_acknowledge: *mut u8,
    ) -> i32;

    pub fn hal_toggle_led();
    pub fn hal_read_fw_running_u32(address: u32) -> u32;
    pub fn hal_api_read_field(offset: u32, len: u32, out: *mut u8);
    pub fn hal_api_write_field(offset: u32, len: u32, data: *const u8);
    pub fn hal_get_field_map(api_idx: u8, offset: *mut u32, len: *mut u32, readonly: *mut bool) -> i32;
    pub fn hal_get_field_map_length() -> u32;
    pub fn hal_get_field_map_idx(i: u32) -> u8;
    pub fn hal_kbd_queue_peek(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_kbd_queue_remove(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_mouse_queue_peek(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_mouse_queue_remove(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_uart_tx_queue_remove(dev: *mut c_void, out: *mut u8) -> bool;
    pub fn hal_set_config_mode_scratch();

    // ---- DMA ----
    pub fn hal_dma_channel_is_busy(dev: *mut c_void) -> bool;
    pub fn hal_dma_tx_send(dev: *mut c_void, buf: *const u8, len: u32);
    pub fn hal_dma_rx_remaining(dev: *mut c_void) -> u32;
    pub fn hal_is_start_of_packet(dev: *mut c_void) -> bool;
    pub fn hal_fetch_packet(dev: *mut c_void);
}
