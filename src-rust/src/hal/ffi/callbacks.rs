// USB/UART callback FFI — all functions called from usb.c/uart.c callbacks.

use core::ffi::c_void;
use crate::domain::hid_routing;
use crate::domain::keyboard::HotkeyAction;
use crate::domain::mouse_logic;
use crate::domain::hid_parser::{self, ReportVal};
use crate::domain::structs::{iface_from_ptr, get_keyboard, KBD_REPORT_LENGTH};
use crate::hal::device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

// ============================================================
// Shared helpers
// ============================================================

unsafe fn hal_from(dev: *mut c_void) -> crate::hal::pico::PicoHal {
    crate::hal::pico::PicoHal::new(dev)
}

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
    if raw_report.is_null() || iface.is_null() { return; }
    if length < KBD_REPORT_LENGTH as i32 { return; }

    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);

    // Extract keyboard data (unsafe pointer work stays in ffi)
    let mut new_report = [0u8; 8];
    rust_extract_kbd_data(raw_report, length, itf, iface, new_report.as_mut_ptr());

    // Delegate to service
    use crate::service::frontend::kbd_pipeline::{self, KbdAction};
    match kbd_pipeline::process_report(state, &new_report, itf) {
        KbdAction::HotkeyConsumed { action, acknowledge } => {
            execute_hotkey_action(dev, action);
            if acknowledge { hal.blink(); }
            return;
        }
        KbdAction::HotkeyPassthrough { action, acknowledge } => {
            execute_hotkey_action(dev, action);
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
    if raw_report.is_null() || iface.is_null() || length < 2 { return; }
    let ifc = iface_from_ptr(iface);

    let raw = core::slice::from_raw_parts(raw_report, length as usize);
    let report_id = *raw_report;
    let kbd = get_keyboard(ifc, report_id);
    let new_report = hid_routing::parse_consumer_report(
        raw, ifc.consumer.is_variable, &kbd.cc_array,
    );

    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);
    hal.route_consumer(state, &new_report);
}

#[export_name = "process_system_report"]
pub unsafe extern "C" fn rust_process_system_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    _iface: *mut c_void,
) {
    if raw_report.is_null() || length < 2 { return; }

    let report = [*raw_report.add(1), 0];

    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);
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
    if raw_report.is_null() || iface_ptr.is_null() { return; }

    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);
    let iface = iface_from_ptr(iface_ptr);

    // Extract raw HID values (unsafe pointer work stays in ffi)
    let values = extract_mouse_values(raw_report, len, iface, state.mouse_buttons);

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
pub unsafe extern "C" fn rust_process_uart_packet(packet_ptr: *const u8, dev: *mut c_void) {
    if packet_ptr.is_null() { return; }
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);

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
pub unsafe fn execute_hotkey_action(dev: *mut c_void, action: HotkeyAction) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::hotkey_dispatch::execute_action(state, &hal, action);
}

// ============================================================
// HID report + parser FFI (from hid.rs)
// ============================================================

#[export_name = "get_report_value"]
pub unsafe extern "C" fn rust_get_report_value(report: *const u8, len: i32, val: *const u8) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 { return 0; }
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
    if iface_ptr.is_null() || val_ptr.is_null() { return; }

    let val = core::ptr::read_unaligned(val_ptr as *const ReportVal);
    let rid = val.report_id;
    let iface = iface_from_ptr(iface_ptr);

    use crate::domain::hid_classify::populate_interface_field;
    if let Some(handler_type) = populate_interface_field(iface, &val) {
        device::hal_set_report_handler(iface_ptr, rid, handler_type);
    }
}
