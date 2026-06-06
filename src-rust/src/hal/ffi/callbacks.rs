// USB/UART callback FFI — all functions called from usb.c/uart.c callbacks.
// Thin wrappers: pointer conversion + delegation to domain/service layer.

use core::ffi::c_void;
use crate::domain::hid_routing;
use crate::domain::keyboard::HotkeyAction;
use crate::domain::mouse_logic;
use crate::domain::hid_parser::{self, ReportVal};
use crate::domain::config::ConfigFlash;
use crate::domain::structs::{self, iface_from_ptr, get_keyboard, KBD_REPORT_LENGTH, HidInterface};
use crate::hal::device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

// ============================================================
// Keyboard report processing (from kbd_process.rs)
// ============================================================

/// Full keyboard report processing pipeline.
#[export_name = "process_keyboard_report"]
pub unsafe extern "C" fn rust_process_keyboard_report(
    raw_report: *mut u8,
    length: i32,
    itf: u8,
    iface: *mut c_void,
) {
    if raw_report.is_null() || iface.is_null() { crate::traceln!("kbd: null ptr"); return; }
    if length < KBD_REPORT_LENGTH as i32 { crate::traceln!("kbd: short report"); return; }

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();

    // Extract keyboard data (unsafe pointer work stays in ffi)
    let mut new_report = [0u8; 8];
    rust_extract_kbd_data(raw_report, length, itf, iface, new_report.as_mut_ptr());

    // CapsLock remap (hotkey-first): skip the remap when this report is a
    // consumed hotkey, so combos like LCtrl+CapsLock (OutputToggle) still fire on
    // the original report. Otherwise drive the remap state machine, which strips
    // the trigger from the live report; the tap (LANG1 / Shift+Space) and hold
    // (CapsLock) outputs are emitted separately by remap_engine_tick_task. No-op
    // when the engine has no entries (e.g. macOS on both outputs).
    let orig = crate::domain::structs::HidKeyboardReport {
        modifier: new_report[0],
        reserved: new_report[1],
        keycode: [new_report[2], new_report[3], new_report[4],
                  new_report[5], new_report[6], new_report[7]],
    };
    let consumed_hotkey = matches!(
        crate::domain::keyboard::check_all_hotkeys(&orig),
        Some(m) if !m.pass_to_os
    );
    if !consumed_hotkey {
        let engine = super::tasks::get_remap_engine();
        let mut kr = orig;
        crate::domain::key_remap::remap_engine_process(
            engine, &mut kr, state.cfg.active_output, hal.now_us_64());
        new_report = [
            kr.modifier, kr.reserved,
            kr.keycode[0], kr.keycode[1], kr.keycode[2],
            kr.keycode[3], kr.keycode[4], kr.keycode[5],
        ];
    }

    // Delegate to service
    use crate::service::frontend::kbd_pipeline::{self, KbdAction};
    match kbd_pipeline::process_report(state, &new_report, itf) {
        KbdAction::HotkeyConsumed { action, acknowledge } => {
            execute_hotkey_action(state, &hal, action);
            if acknowledge { hal.blink(); }
            return;
        }
        KbdAction::HotkeyPassthrough { action, acknowledge } => {
            execute_hotkey_action(state, &hal, action);
            if acknowledge { hal.blink(); }
            // Fall through to route
        }
        KbdAction::Dropped => return,
        KbdAction::Route => {}
    }

    kbd_pipeline::route_combined(state, &hal);
}

/// Rust implementation of process_consumer_report
#[export_name = "process_consumer_report"]
pub unsafe extern "C" fn rust_process_consumer_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    iface: *mut c_void,
) {
    if raw_report.is_null() || iface.is_null() || length < 2 { crate::traceln!("cc: null ptr"); return; }
    let ifc = iface_from_ptr(iface);

    let raw = core::slice::from_raw_parts(raw_report, length as usize);
    let report_id = *raw_report;
    let kbd = get_keyboard(ifc, report_id);
    let new_report = hid_routing::parse_consumer_report(
        raw, ifc.consumer.is_variable, &kbd.cc_array,
    );

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();
    hal.route_consumer(state, &new_report);
}

#[export_name = "process_system_report"]
pub unsafe extern "C" fn rust_process_system_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    _iface: *mut c_void,
) {
    if raw_report.is_null() || length < 2 { crate::traceln!("sys: null ptr"); return; }

    let report = [*raw_report.add(1), 0];

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();
    hal.route_system(state, &report);
}

// ============================================================
// Mouse report processing (from mouse_process.rs)
// ============================================================

/// Full mouse report processing pipeline.
#[export_name = "process_mouse_report"]
pub unsafe extern "C" fn rust_process_mouse_report(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,
) {
    if raw_report.is_null() || iface_ptr.is_null() { crate::traceln!("mouse: null ptr"); return; }

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();
    let iface = iface_from_ptr(iface_ptr);

    // Extract raw HID values (unsafe pointer work stays in ffi)
    let values = extract_mouse_values(raw_report, len, iface, state.hid.mouse_buttons);

    // Delegate to service
    crate::service::frontend::mouse_pipeline::process_report(state, &hal, &values);
}

/// Extract mouse values from raw HID report (boot protocol or descriptor-based).
unsafe fn extract_mouse_values(
    raw_report: *mut u8,
    len: i32,
    iface: &crate::domain::structs::HidInterface,
    fallback_buttons: i16,
) -> mouse_logic::MouseValues {
    const HID_PROTOCOL_BOOT: u8 = 0;
    let mut v = [0i32; 5];

    if iface.protocol == HID_PROTOCOL_BOOT {
        v[0] = *raw_report.add(1) as i8 as i32;
        v[1] = *raw_report.add(2) as i8 as i32;
        v[2] = *raw_report.add(3) as i8 as i32;
        v[4] = *raw_report as i32;
    } else {
        let uses_id = iface.uses_report_id;
        let report_slice = core::slice::from_raw_parts(raw_report, len as usize);

        fn extract_val(report: &[u8], uses_id: bool, rv: &ReportVal) -> Option<i32> {
            let src = if uses_id {
                if report[0] != rv.report_id { return None; }
                &report[1..]
            } else { report };
            Some(crate::domain::hid_report::get_report_value(src, rv.offset, rv.size))
        }

        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.move_x) { v[0] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.move_y) { v[1] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.wheel) { v[2] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.pan) { v[3] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.buttons) {
            v[4] = val;
        } else {
            v[4] = fallback_buttons as i32;
        }
    }

    mouse_logic::MouseValues { move_x: v[0], move_y: v[1], wheel: v[2], pan: v[3], buttons: v[4] }
}

// ============================================================
// UART packet dispatch — unified entry point replacing C's process_packet()
// ============================================================

/// Process a UART packet. Called from C's packet_receiver_task after fetch_packet.
/// Replaces the entire process_packet() switch in uart.c.
#[export_name = "process_packet"]
pub unsafe extern "C" fn rust_process_uart_packet(packet_ptr: *const u8) {
    if packet_ptr.is_null() { crate::traceln!("uart: null pkt"); return; }
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;

    let pkt = crate::domain::packet::UartPacket {
        ptype: *packet_ptr,
        data: {
            let mut d = [0u8; 8];
            core::ptr::copy_nonoverlapping(packet_ptr.add(1), d.as_mut_ptr(), 8);
            d
        },
        checksum: *packet_ptr.add(9),
    };

    crate::service::packet_dispatch::dispatch_packet(state, &hal, &pkt);
}

// ============================================================
// Hotkey dispatch (from hotkey_dispatch.rs)
// ============================================================

/// Execute a hotkey action by its enum variant. Called from kbd_process.
pub unsafe fn execute_hotkey_action(
    state: &mut crate::domain::structs::DeviceState<'_>,
    hal: &crate::hal::pico::PicoHal,
    action: HotkeyAction,
) {
    crate::service::hotkey_dispatch::execute_action(state, hal, action);
}

// ============================================================
// HID report + parser FFI (from hid.rs)
// ============================================================

#[export_name = "get_report_value"]
pub unsafe extern "C" fn rust_get_report_value(report: *const u8, len: i32, val: *const u8) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 { crate::traceln!("grv: bad input"); return 0; }
    let slice = core::slice::from_raw_parts(report, len as usize);
    let rv = core::ptr::read_unaligned(val as *const ReportVal);
    crate::domain::hid_report::get_report_value(slice, rv.offset, rv.size)
}

// ============================================================
// HID parser (from hid_parser_ffi.rs)
// ============================================================

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls extract_data for each
/// parsed INPUT item to populate hid_interface_t.
#[export_name = "parse_report_descriptor"]
pub unsafe extern "C" fn rust_parse_report_descriptor(
    iface_ptr: *mut c_void,  // hid_interface_t*
    report: *const u8,
    desc_len: i32,
) {
    if iface_ptr.is_null() || report.is_null() || desc_len <= 0 {
        crate::traceln!("hid: null desc");
        return;
    }

    let desc = core::slice::from_raw_parts(report, desc_len as usize);
    let (_parser, results) = hid_parser::parse_descriptor(desc);

    let iface = iface_from_ptr(iface_ptr);

    for input in results.iter() {
        if input.uses_report_id {
            iface.uses_report_id = true;
        }

        for i in 0..input.count {
            let val = &input.vals[i];
            rust_extract_data(iface_ptr, val as *const _ as *const u8);
        }
    }
}

// ============================================================
// Keyboard data extraction — thin FFI wrapper over domain::kbd_extract
// ============================================================

/// Full extract_kbd_data -- FFI entry point.
/// Converts raw pointers to slices and delegates to domain::kbd_extract.
#[export_name = "extract_kbd_data"]
pub unsafe extern "C" fn rust_extract_kbd_data(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,
    out_report: *mut u8,
) -> i32 {
    if raw_report.is_null() || iface_ptr.is_null() || out_report.is_null() || len < 8 {
        crate::traceln!("ekd: bad input");
        return 0;
    }

    let report_slice = core::slice::from_raw_parts(raw_report, len as usize);
    let iface = iface_from_ptr(iface_ptr);
    let report_id = *raw_report;
    let kbd = get_keyboard(iface, report_id);

    let (result, rc) = crate::domain::kbd_extract::extract_kbd_data(report_slice, iface, kbd);
    core::ptr::copy_nonoverlapping(result.as_ptr(), out_report, result.len());
    rc
}

// ============================================================
// extract_data (from extract_data.rs)
// ============================================================

/// Rust implementation of extract_data -- thin FFI wrapper.
/// Delegates classification + population to domain::hid_classify::populate_interface_field,
/// then calls HAL to register the report handler if needed.
#[export_name = "extract_data"]
pub unsafe extern "C" fn rust_extract_data(iface_ptr: *mut c_void, val_ptr: *const u8) {
    if iface_ptr.is_null() || val_ptr.is_null() { crate::traceln!("ed: null ptr"); return; }

    let val = core::ptr::read_unaligned(val_ptr as *const ReportVal);
    let rid = val.report_id;
    let iface = iface_from_ptr(iface_ptr);

    use crate::domain::hid_classify::populate_interface_field;
    if let Some(handler_type) = populate_interface_field(iface, &val) {
        device::hal_set_report_handler(iface_ptr, rid, handler_type);
    }
}

// ============================================================
// TinyUSB host callbacks — thin FFI wrappers delegating to service::usb
// ============================================================

/// HID device mounted — configure protocol and start receiving reports.
#[export_name = "rust_on_hid_mount"]
pub unsafe extern "C" fn rust_on_hid_mount(
    dev_addr: u8,
    instance: u8,
    desc_report: *const u8,
    desc_len: u16,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let iface = iface_from_ptr(iface_ptr);
    iface.protocol = device::hal_tuh_hid_get_protocol(dev_addr, instance);

    // Parse HID report descriptor
    rust_parse_report_descriptor(iface_ptr, desc_report, desc_len as i32);

    let mut state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();

    // Passthrough: capture descriptor if enabled
    let pt = super::tasks::get_pt_state();
    let desc_slice = core::slice::from_raw_parts(desc_report, desc_len as usize);
    crate::service::passthrough_service::on_device_mount(
        pt, &state, dev_addr, instance, itf_protocol, desc_slice, &hal,
    );

    let params = crate::service::usb::MountParams { dev_addr, instance, itf_protocol };

    if let Some(proto) = crate::service::usb::on_hid_mount(&mut state, &hal, iface, &params) {
        device::hal_tuh_hid_set_protocol(dev_addr, instance, proto);
    }

    device::hal_tuh_hid_receive_report(dev_addr, instance);
}

/// HID device unmounted — clear connection state and zero interface.
#[export_name = "rust_on_hid_umount"]
pub unsafe extern "C" fn rust_on_hid_umount(
    dev_addr: u8,
    instance: u8,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let state = structs::DeviceState::from_globals();

    crate::service::usb::on_hid_umount(state.cfg, itf_protocol);

    // Passthrough: clean up state for this device
    let pt = super::tasks::get_pt_state();
    crate::service::passthrough_service::on_device_unmount(pt, dev_addr);

    // Zero the interface structure
    let iface = iface_ptr as *mut HidInterface;
    core::ptr::write_bytes(iface, 0, 1);
}

/// HID report received — dispatch to appropriate handler.
/// Raw pointer dispatch stays in FFI layer; only device_idx calc is delegated.
#[export_name = "rust_on_hid_report_received"]
pub unsafe extern "C" fn rust_on_hid_report_received(
    dev_addr: u8,
    instance: u8,
    report: *const u8,
    len: u16,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let iface = iface_from_ptr(iface_ptr);
    let mut state = structs::DeviceState::from_globals();

    // Stamp HID activity (mouse/keyboard/HID++) for the board-LED activity
    // flicker. u32 write is atomic across cores; led_blink_tick (Core1) reads it.
    state.cfg.last_hid_activity_us = device::hal_time_us_32();

    // Passthrough: forward non-keyboard reports first
    if itf_protocol != crate::domain::constants::HID_ITF_PROTOCOL_KEYBOARD {
        let pt = super::tasks::get_pt_state();
        let hal = crate::hal::pico::PicoHal::new();
        let report_slice = core::slice::from_raw_parts(report, len as usize);
        if matches!(
            crate::service::passthrough_service::on_report_received(
                pt, &mut state, report_slice, dev_addr, instance, &hal,
            ),
            crate::service::passthrough_service::ReportAction::Handled
        ) {
            return;
        }
    }

    let device_idx = hid_routing::calculate_device_idx(
        itf_protocol, dev_addr, instance,
        state.hid.kbd_dev_addr, state.hid.kbd_instance,
    );

    if iface.uses_report_id || itf_protocol == crate::domain::constants::HID_ITF_PROTOCOL_NONE {
        let report_id = if iface.uses_report_id { *report } else { 0 };

        if (report_id as usize) < structs::MAX_REPORTS {
            if let Some(handler) = iface.report_handler[report_id as usize] {
                handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
            }
        }
    } else if itf_protocol == crate::domain::constants::HID_ITF_PROTOCOL_KEYBOARD {
        if let Some(handler) = get_process_keyboard_report() {
            handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
        }
    } else if itf_protocol == crate::domain::constants::HID_ITF_PROTOCOL_MOUSE {
        if let Some(handler) = get_process_mouse_report() {
            handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
        }
    }

    device::hal_tuh_hid_receive_report(dev_addr, instance);
}

// Direct references to the Rust-exported report processors for fallback dispatch.
extern "C" {
    #[link_name = "process_keyboard_report"]
    fn process_keyboard_report_c(report: *mut u8, len: i32, itf: u8, iface: *mut HidInterface);
    #[link_name = "process_mouse_report"]
    fn process_mouse_report_c(report: *mut u8, len: i32, itf: u8, iface: *mut HidInterface);
}

fn get_process_keyboard_report() -> Option<unsafe extern "C" fn(*mut u8, i32, u8, *mut HidInterface)> {
    Some(process_keyboard_report_c)
}

fn get_process_mouse_report() -> Option<unsafe extern "C" fn(*mut u8, i32, u8, *mut HidInterface)> {
    Some(process_mouse_report_c)
}

/// HID set_protocol completed — update interface protocol field.
#[export_name = "rust_on_hid_set_protocol_complete"]
pub unsafe extern "C" fn rust_on_hid_set_protocol_complete(
    iface_ptr: *mut c_void,
    protocol: u8,
) {
    let iface = iface_from_ptr(iface_ptr);
    iface.protocol = protocol;
}

/// TinyUSB device set_report callback — config packets and keyboard LED handling.
#[export_name = "rust_on_tud_set_report"]
pub unsafe extern "C" fn rust_on_tud_set_report(
    instance: u8,
    report_id: u8,
    report_type: u8,
    buffer: *const u8,
    bufsize: u16,
) {
    use crate::domain::constants::{RAW_PACKET_LENGTH, START_LENGTH};

    const ITF_NUM_HID_VENDOR: u8 = 2;
    const REPORT_ID_VENDOR: u8 = 6;
    const REPORT_ID_KEYBOARD: u8 = 1;
    const HID_REPORT_TYPE_OUTPUT: u8 = 2;

    if buffer.is_null() { return; }

    // Passthrough: forward output reports from host to receiver
    {
        let pt = super::tasks::get_pt_state();
        if pt.active && instance >= crate::domain::passthrough::ITF_NUM_PT_BASE {
            let buf = core::slice::from_raw_parts(buffer, bufsize as usize);
            crate::service::passthrough_service::on_set_report(
                pt, instance, report_id, report_type, buf,
            );
            return;
        }
    }

    // Config vendor report (pointer dispatch stays in FFI)
    if instance == ITF_NUM_HID_VENDOR && report_id == REPORT_ID_VENDOR {
        let state = structs::DeviceState::from_globals();
        if !state.cfg.config_mode_active { return; }
        if bufsize as usize != RAW_PACKET_LENGTH { return; }

        extern "C" {
            fn validate_packet(packet: *const u8) -> bool;
            fn process_packet(packet: *const u8);
        }
        let packet_ptr = buffer.add(START_LENGTH);
        if !validate_packet(packet_ptr) { return; }
        process_packet(packet_ptr);
    }

    // Keyboard LED state change — delegate to service
    if report_id != REPORT_ID_KEYBOARD || bufsize != 1 || report_type != HID_REPORT_TYPE_OUTPUT {
        return;
    }

    let mut state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();
    crate::service::usb::process_led_report(&mut state, &hal, *buffer);
}

// ============================================================
// Setup init — thin FFI wrapper delegating to domain::config
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_init_config(config_mode_active: bool, board_role: u8, timestamp: u64) {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    crate::domain::config::init_config(cfg, config_mode_active, board_role, timestamp);
}

// ============================================================
// Flash config — thin FFI wrappers delegating to domain::config
// ============================================================

extern "C" {
    #[link_name = "default_config"]
    static DEFAULT_CONFIG: structs::Config;
}

#[export_name = "load_config"]
pub unsafe extern "C" fn rust_load_config() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hal = crate::hal::pico::PicoHal::new();
    let size = core::mem::size_of::<structs::Config>();
    let config_bytes = core::slice::from_raw_parts_mut(
        &mut cfg.config as *mut structs::Config as *mut u8, size,
    );
    if let Some(default) = crate::domain::config::load_config_from_bytes(
        config_bytes, &cfg.config, &hal, &DEFAULT_CONFIG,
    ) {
        cfg.config = default;
    }
}

#[export_name = "save_config"]
pub unsafe extern "C" fn rust_save_config() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hal = crate::hal::pico::PicoHal::new();
    let size = core::mem::size_of::<structs::Config>();
    let config_bytes = core::slice::from_raw_parts_mut(
        &mut cfg.config as *mut structs::Config as *mut u8, size,
    );
    cfg.config.checksum = crate::domain::config::compute_config_checksum(config_bytes);
    // Re-read bytes after checksum update
    let config_bytes = core::slice::from_raw_parts(
        &cfg.config as *const structs::Config as *const u8, size,
    );
    let page = crate::domain::config::prepare_save_page(config_bytes);
    hal.flash_write_config(&page);
}

#[export_name = "reset_config_timer"]
pub unsafe extern "C" fn rust_reset_config_timer() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let now = device::hal_time_us_64();
    crate::domain::config::reset_config_timer(cfg, now);
}

// ============================================================
// Output switching — thin FFI wrapper delegating to service::output
// ============================================================

extern "C" {
    fn release_all_keys();
}

/// Send an upstream keyboard-LED report, but only from Core1 (the host-stack
/// core). Called on Core0, it defers by flagging leds_resync_pending; the Core1
/// led_blink_tick drains the flag and re-sends. This keeps tuh_hid_set_report
/// off Core0, which would otherwise race tuh_task and hang Core1.
unsafe fn send_kbd_leds_xcore(da: u8, inst: u8, leds: *const u8, len: u8) {
    if device::hal_is_core1() {
        device::hal_tuh_hid_set_report(da, inst, leds, len);
    } else {
        (*core::ptr::addr_of_mut!(structs::GLOBAL_CFG)).leds_resync_pending = true;
    }
}

#[export_name = "set_active_output"]
pub unsafe extern "C" fn rust_set_active_output(output: u8) {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hid = &*core::ptr::addr_of!(structs::GLOBAL_HID);
    crate::service::output::switch_output(
        cfg,
        hid,
        output,
        |on| device::hal_gpio_put_led(on),
        |da, inst, leds, len| send_kbd_leds_xcore(da, inst, leds, len),
        |val, ptype| device::send_value(val, ptype),
        || release_all_keys(),
    );
}

// ============================================================
// Debug state dump
// ============================================================

/// Latches once the heap-low warning has fired (arena is monotonic, so one shot).
static mut HEAP_WARNED: bool = false;

#[no_mangle]
pub unsafe extern "C" fn hal_debug_dump_state() {
    use crate::service::dlog;

    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    let inuse = device::hal_heap_inuse();
    let arena = device::hal_heap_arena();
    let limit = device::hal_heap_limit();
    dlog::i(b"hb")
        .s(b"tud=").u(cfg.tud_connected as u32)
        .s(b" kbd=").u(cfg.keyboard_connected as u32)
        .s(b" mse=").u(cfg.mouse_connected as u32)
        .s(b" role=").u(cfg.board_role as u32)
        .s(b" out=").u(cfg.active_output as u32)
        .s(b" c0=").u64(cfg.core0_last_loop_pass)
        .s(b" c1=").u64(cfg.core1_last_loop_pass)
        .s(b" heap=").u(inuse)
        .s(b"/").u(arena)
        .s(b"/").u(limit)
        .done();

    // Early-warn on heap exhaustion: the log ring caps malloc at `limit` bytes,
    // so a runaway arena would crash. Fire once when arena crosses 75% of limit.
    let warned = *core::ptr::addr_of!(HEAP_WARNED);
    if !warned && limit > 0 && arena.saturating_mul(100) >= limit.saturating_mul(75) {
        *core::ptr::addr_of_mut!(HEAP_WARNED) = true;
        dlog::w(b"heap")
            .s(b"low arena=").u(arena)
            .s(b" limit=").u(limit)
            .s(b" raise __LOG_HEAP_RESERVE")
            .done();
    }
}

// ============================================================
// LED blink + toggle
// ============================================================

#[export_name = "blink_led"]
pub unsafe extern "C" fn rust_blink_led() {
    let led = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_LED);
    led.blinks_left = 5;
    led.last_led_change = device::hal_time_us_32() as i32;
}

#[export_name = "toggle_led"]
pub unsafe extern "C" fn rust_toggle_led() -> u8 {
    let state = !device::hal_gpio_get_led();
    device::hal_gpio_put_led(state);
    state as u8
}

#[no_mangle]
pub unsafe extern "C" fn hal_toggle_led() -> u8 {
    rust_toggle_led()
}

// ============================================================
// LED control — thin FFI wrappers delegating to service::led
// ============================================================

#[export_name = "restore_leds"]
pub unsafe extern "C" fn rust_restore_leds() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hid = &*core::ptr::addr_of!(structs::GLOBAL_HID);
    crate::service::led::sync_indicators(
        cfg,
        hid,
        |on| device::hal_gpio_put_led(on),
        |da, inst, leds, len| send_kbd_leds_xcore(da, inst, leds, len),
    );
}

#[export_name = "set_keyboard_leds"]
pub unsafe extern "C" fn rust_set_keyboard_leds(leds: u8) {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    let hid = &*core::ptr::addr_of!(structs::GLOBAL_HID);
    crate::service::led::send_kbd_led_report(
        cfg,
        hid,
        leds,
        |da, inst, led_val, len| send_kbd_leds_xcore(da, inst, led_val, len),
    );
}

// ============================================================
// TinyUSB descriptor selection — C callbacks delegate here
// ============================================================

// C descriptor arrays (defined in usb_descriptors.c, static const)
extern "C" {
    static desc_device_config: u8;
    static desc_device: u8;
    static desc_hid_report: u8;
    static desc_hid_report_relmouse: u8;
    static desc_hid_report_vendor: u8;
    static desc_configuration_config: u8;
    static desc_configuration: u8;
}

const ITF_NUM_HID_VENDOR: u8 = 2;
const ITF_NUM_HID_C: u8 = 0;
const ITF_NUM_HID_REL_M: u8 = 1;

#[no_mangle]
pub unsafe extern "C" fn rust_get_device_descriptor() -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    if crate::service::usb::is_config_mode(cfg) {
        return core::ptr::addr_of!(desc_device_config);
    }

    // Passthrough: present upstream device identity
    let pt = super::tasks::get_pt_state();
    if pt.active && pt.upstream_vid != 0 {
        // Reuse desc_device as template, patch VID/PID into static buffer
        static mut PT_DEVICE_DESC: [u8; 18] = [0; 18];
        let buf = core::ptr::addr_of_mut!(PT_DEVICE_DESC).cast::<u8>();
        let src = core::ptr::addr_of!(desc_device).cast::<u8>();
        core::ptr::copy_nonoverlapping(src, buf, 18);
        *buf.add(8) = (pt.upstream_vid & 0xFF) as u8;
        *buf.add(9) = (pt.upstream_vid >> 8) as u8;
        *buf.add(10) = (pt.upstream_pid & 0xFF) as u8;
        *buf.add(11) = (pt.upstream_pid >> 8) as u8;
        return buf;
    }

    core::ptr::addr_of!(desc_device)
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_hid_report_descriptor(instance: u8) -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    if crate::service::usb::is_config_mode(cfg) && instance == ITF_NUM_HID_VENDOR {
        return core::ptr::addr_of!(desc_hid_report_vendor);
    }

    // Passthrough: return captured descriptor for passthrough instances
    let pt = super::tasks::get_pt_state();
    if pt.active && instance >= crate::domain::passthrough::ITF_NUM_PT_BASE {
        if let Some((desc, _len)) = crate::domain::passthrough::get_report_desc(pt, instance) {
            return desc.as_ptr();
        }
    }

    match instance {
        ITF_NUM_HID_C => core::ptr::addr_of!(desc_hid_report),
        ITF_NUM_HID_REL_M => core::ptr::addr_of!(desc_hid_report_relmouse),
        _ => core::ptr::addr_of!(desc_hid_report),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_configuration_descriptor() -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    if crate::service::usb::is_config_mode(cfg) {
        return core::ptr::addr_of!(desc_configuration_config);
    }

    // Passthrough: return dynamically built descriptor
    let pt = super::tasks::get_pt_state();
    if pt.active {
        let (ptr, len) = super::pt_config_desc_ptr();
        if len > 0 {
            return ptr;
        }
    }

    core::ptr::addr_of!(desc_configuration)
}

// ============================================================
// TinyUSB device mount/unmount — set tud_connected flag
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_on_tud_mount() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    cfg.tud_connected = true;
}

#[no_mangle]
pub unsafe extern "C" fn rust_on_tud_umount() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    cfg.tud_connected = false;
}

// ============================================================
// LED diagnostics — blocking pattern playback (boot/halt)
// ============================================================

unsafe fn busy_wait_ms(ms: u16) {
    let start = device::hal_time_us_32();
    let duration = (ms as u32) * 1000;
    while device::hal_time_us_32().wrapping_sub(start) < duration {
        core::hint::spin_loop();
    }
}

/// Play a diagnostic LED pattern (blocking).
/// Always-level patterns play in all builds; DebugOnly patterns require dh_debug.
#[no_mangle]
pub unsafe extern "C" fn diag_led(event: u8) {
    use crate::domain::led_pattern::{DiagEvent, DiagLevel};

    if let Some(ev) = DiagEvent::from_u8(event) {
        let pat = ev.pattern();

        // Skip DebugOnly patterns in release builds
        #[cfg(not(feature = "dh_debug"))]
        if matches!(pat.level, DiagLevel::DebugOnly) { return; }

        play_pattern(&pat);
    }
}

unsafe fn play_pattern(pat: &crate::domain::led_pattern::LedPattern) {
    loop {
        for _ in 0..pat.blinks {
            device::hal_gpio_put_led(true);
            busy_wait_ms(pat.on_ms);
            device::hal_gpio_put_led(false);
            busy_wait_ms(pat.off_ms);
        }
        busy_wait_ms(pat.pause_ms);
        if !pat.repeat { break; }
    }
}
