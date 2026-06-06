// HAL device interface — all hardware-dependent operations.
// Rust calls these thin C wrappers for Pico SDK / TinyUSB / GPIO access.

extern "C" {
    // ---- Timestamp ----
    pub fn hal_time_us_64() -> u64;
    pub fn hal_time_us_32() -> u32;

    // ---- Queue operations ----
    pub fn hal_queue_mouse_report(report: *const u8);
    pub fn hal_queue_kbd_report(report: *const u8);
    pub fn hal_queue_uart_packet(packet: *const u8);
    pub fn hal_queue_try_add_uart(data: *const u8) -> bool;

    // ---- HID queue helpers ----
    pub fn hal_queue_cc_packet(payload: *const u8);
    pub fn hal_queue_system_packet(payload: *const u8);

    // ---- UART send helpers (direct C functions) ----
    pub fn send_value(value: u8, packet_type: u8);
    pub fn queue_packet(data: *const u8, packet_type: u8, length: i32);
    /// Free slots in the UART TX queue (for throttling the peer-log forwarder).
    pub fn hal_uart_tx_free() -> u32;

    // ---- Config / Flash (direct C functions) ----
    pub fn save_config();
    pub fn load_config();
    pub fn wipe_config();

    // ---- Output switching / LEDs (direct C functions) ----
    pub fn set_active_output(output: u8);
    pub fn restore_leds();
    // release_all_keys is now a Rust #[export_name] in keyboard.rs

    // ---- Hardware ----
    pub fn hal_watchdog_update();
    pub fn blink_led();
    pub fn reboot();
    pub fn hal_reset_usb_boot();

    // ---- TinyUSB (via hal_shim.c — TinyUSB functions are inline/macro) ----
    pub fn hal_tud_ready() -> bool;
    pub fn hal_tud_suspended() -> bool;
    pub fn hal_tud_remote_wakeup();
    pub fn hal_tud_hid_n_ready(instance: u8) -> bool;
    pub fn hal_tud_hid_keyboard_report(report_id: u8, modifier: u8, keycode: *const u8) -> bool;
    pub fn hal_tud_mouse_report(mode: u8, buttons: u8, x: i16, y: i16, wheel: i8, pan: i8) -> bool;

    // ---- TinyUSB host (via hal_shim.c — TinyUSB host functions are inline/macro) ----
    pub fn hal_tuh_hid_interface_protocol(dev_addr: u8, instance: u8) -> u8;
    pub fn hal_tuh_hid_get_protocol(dev_addr: u8, instance: u8) -> u8;
    pub fn hal_tuh_hid_set_protocol(dev_addr: u8, instance: u8, protocol: u8);
    pub fn hal_tuh_hid_receive_report(dev_addr: u8, instance: u8) -> bool;

    // ---- Flash config (via hal_shim.c) ----
    pub fn hal_flash_read_config(buf: *mut u8, len: u32);
    pub fn hal_flash_write_config(buf: *const u8);

    // ---- LED / HID host (via hal_shim.c) ----
    pub fn hal_gpio_put_led(state: bool);
    pub fn hal_gpio_get_led() -> bool;
    pub fn hal_tuh_hid_set_report(dev_addr: u8, instance: u8, data: *const u8, len: u8);
    pub fn hal_is_core1() -> bool;

    // ---- hid_interface_t (remaining C-dependent function) ----
    /// Assigns C function pointers (process_*_report) to report_handler array
    pub fn hal_set_report_handler(iface: *mut core::ffi::c_void, report_id: u8, handler_type: u8);

    pub fn hal_toggle_led() -> u8;
    pub fn hal_is_bootsel_pressed() -> bool;
    pub fn set_keyboard_leds(leds: u8);
    pub fn hal_read_fw_running_u32(address: u32) -> u32;
    pub fn hal_kbd_queue_peek(out: *mut u8) -> bool;
    pub fn hal_kbd_queue_remove(out: *mut u8) -> bool;
    pub fn hal_mouse_queue_peek(out: *mut u8) -> bool;
    pub fn hal_mouse_queue_remove(out: *mut u8) -> bool;
    pub fn hal_uart_tx_queue_remove(out: *mut u8) -> bool;
    pub fn hal_set_config_mode_scratch();

    // ---- Debug ----
    pub fn hal_debug_blink(count: i32, delay_ms: i32);
    pub fn hal_debug_dump_state();
    /// Heap arena (peak bytes sbrk'd) and currently in-use bytes — for sizing
    /// the log ring reserve.
    pub fn hal_heap_arena() -> u32;
    pub fn hal_heap_inuse() -> u32;
    /// Heap ceiling (bytes available for malloc above the log ring) — the cap
    /// the ring's reserve leaves. Used to early-warn on heap exhaustion.
    pub fn hal_heap_limit() -> u32;

    // ---- Pico SDK direct ----
    // Unused from Rust (PicoHal::kick() calls hal_watchdog_update instead),
    // but kept for potential C linkage.
    #[allow(dead_code)]
    pub fn watchdog_update();
    pub fn hal_queue_cfg_packet(packet: *const u8);

    // ---- HID output queue ----
    pub fn hal_hid_queue_peek(out: *mut u8) -> bool;
    pub fn hal_hid_queue_remove(out: *mut u8) -> bool;
    pub fn hal_tud_hid_n_report(instance: u8, report_id: u8, data: *const u8, len: u8) -> bool;
    pub fn hal_queue_hid_report(instance: u8, report_id: u8, data: *const u8, len: u8);

    // ---- DMA ----
    pub fn hal_dma_channel_is_busy() -> bool;
    pub fn hal_dma_tx_send(buf: *const u8, len: u32);
    pub fn hal_dma_rx_remaining() -> u32;
    pub fn hal_dma_advance_one();
    pub fn hal_dma_read_pos() -> u32;
    pub fn hal_get_in_packet_ptr() -> *const u8;
    pub fn hal_is_start_of_packet() -> bool;
    pub fn hal_fetch_packet();

    // ---- Passthrough (Semi-DDM) ----
    pub fn hal_tud_disconnect();
    pub fn hal_tud_connect();
    pub fn hal_tuh_vid_pid_get(dev_addr: u8, vid: *mut u16, pid: *mut u16);
    pub fn hal_tuh_set_report(
        dev_addr: u8, itf_num: u8, report_id: u8, report_type: u8,
        data: *const u8, len: u16,
    ) -> bool;
    pub fn hal_passthrough_build_config_desc(
        config_desc: *mut u8, buf_size: u16,
        config_desc_len: *mut u16, iface_count: u8,
    );
}
