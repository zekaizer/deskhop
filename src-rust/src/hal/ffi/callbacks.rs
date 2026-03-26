// USB/UART callback FFI — all functions called from usb.c/uart.c callbacks.

use core::ffi::c_void;
use crate::domain::constants::PacketType;
use crate::domain::actions::{get_border_position, border_to_bytes, BorderUpdate};
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

    let dev = crate::domain::structs::get_global_device() as *mut _ as *mut c_void;
    rust_send_consumer_control(dev, new_report.as_ptr());
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

    let dev = crate::domain::structs::get_global_device() as *mut _ as *mut c_void;
    rust_send_system_control(dev, report.as_ptr());
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
// UART message dispatch (from msg_dispatch.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_handle_simple_msg(ptype: u8, data: *const u8, dev: *mut c_void) -> u8 {
    if data.is_null() { return 0; }
    let state = crate::domain::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    let action = crate::domain::msg_handlers::handle_simple_msg(ptype, &arr, state);
    if crate::domain::msg_handlers::apply_action(&action, state) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_output_select(dev: *mut c_void, output: u8) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::msg_bridge::handle_output_select(state, &hal, output);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart_full(dev: *mut c_void, data: *const u8) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::service::msg_bridge::handle_kbd_from_peer(state, &hal, &arr);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart_full(dev: *mut c_void, data: *const u8) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::service::msg_bridge::handle_mouse_from_peer(state, &hal, &arr);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_set_report(dev: *mut c_void, led_value: u8) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::msg_bridge::handle_set_report(state, &hal, led_value);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_sync_borders(dev: *mut c_void, data: *const u8) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let remote = {
        let mut arr = [0u8; 8];
        core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
        arr
    };
    crate::service::msg_bridge::handle_sync_borders(state, &hal, Some(&remote));
}

// ============================================================
// Hotkey dispatch (from hotkey_dispatch.rs)
// ============================================================

/// Execute a hotkey action by its enum variant. Called from kbd_process.

pub unsafe fn execute_hotkey_action(dev: *mut c_void, action: HotkeyAction) {
    match action {
        HotkeyAction::OutputToggle => rust_output_toggle(dev),
        HotkeyAction::MouseZoomToggle => rust_mouse_zoom_toggle(dev),
        HotkeyAction::SwitchLockToggle => rust_switch_lock_toggle(dev),
        HotkeyAction::ScreenLock => rust_screenlock_handler(dev),
        HotkeyAction::GamingModeToggle => rust_gaming_mode_toggle(dev),
        HotkeyAction::ScreensaverPong => rust_screensaver_pong_enable(dev),
        HotkeyAction::ScreensaverJitter => rust_screensaver_jitter_enable(dev),
        HotkeyAction::ScreensaverDisable => rust_screensaver_disable(dev),
        HotkeyAction::WipeConfig => rust_wipe_config_hotkey(dev),
        HotkeyAction::ScreenBorder => rust_screen_border_hotkey(dev),
        HotkeyAction::ConfigEnable => rust_config_enable(dev),
        HotkeyAction::FwUpgradeA => rust_fw_upgrade_a(dev),
        HotkeyAction::FwUpgradeB => rust_fw_upgrade_b(dev),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_output_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    if crate::domain::hotkey_handlers::output_toggle(state) {
        hal.switch_output(state.active_output);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_mouse_zoom_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let val = crate::domain::hotkey_handlers::mouse_zoom_toggle(state);
    hal.send_value(val as u8, PacketType::MouseZoom as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_lock_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let val = crate::domain::hotkey_handlers::switch_lock_toggle(state);
    hal.send_value(val as u8, PacketType::SwitchLock as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_gaming_mode_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let val = crate::domain::hotkey_handlers::gaming_mode_toggle(state);
    hal.send_value(val as u8, PacketType::GamingMode as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_a(dev: *mut c_void) {
    let hal = hal_from(dev);
    hal.reboot_to_bootloader();
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_b(dev: *mut c_void) {
    let hal = hal_from(dev);
    hal.send_value(1, PacketType::FirmwareUpgrade as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_wipe_config_hotkey(dev: *mut c_void) {
    let hal = hal_from(dev);
    hal.wipe();
    hal.load();
    hal.send_value(1, PacketType::WipeConfig as u8);
}

fn screensaver_dispatch(hal: &impl PeerLink, state: &mut crate::domain::structs::Device, mode: u8) {
    use crate::domain::hotkey_handlers::ScreensaverAction;
    match crate::domain::hotkey_handlers::screensaver_set(state, mode) {
        ScreensaverAction::UpdatedLocally => {}
        ScreensaverAction::SendToRemote(m) => {
            hal.send_value(m, PacketType::Screensaver as u8);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_enable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    if let Some(mode) = crate::domain::hotkey_handlers::screensaver_pong_mode(state) {
        screensaver_dispatch(&hal, state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter_enable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    if let Some(mode) = crate::domain::hotkey_handlers::screensaver_jitter_mode(state) {
        screensaver_dispatch(&hal, state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_disable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    screensaver_dispatch(&hal, state, 0);
}

#[no_mangle]
pub unsafe extern "C" fn rust_config_enable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    // Order matters: set scratch FIRST, release keys, THEN request reboot.
    if !state.config_mode_active {
        hal.set_boot_flag();
    }
    crate::service::backend::host_link::release_all_keys(state, &hal);
    state.reboot_requested = true;
}

#[no_mangle]
pub unsafe extern "C" fn rust_screen_border_hotkey(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        hal.save();
    }
    let b = &state.config.output[idx].border;
    let bytes = border_to_bytes(b.top, b.bottom);
    hal.send_packet(&bytes, PacketType::SyncBorders as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_screenlock_handler(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    for out in 0..2u8 {
        if let Some((modifier, key)) = crate::domain::actions::screenlock_keys(state.config.output[out as usize].os) {
            let mut report = [0u8; 8];
            report[0] = modifier; report[2] = key;
            if state.board_role == out {
                hal.push_kbd_report(&report);
                crate::service::backend::host_link::release_all_keys(state, &hal);
            } else {
                hal.send_packet(&report, PacketType::KeyboardReport as u8);
                hal.send_packet(&[0u8; 8], PacketType::KeyboardReport as u8);
            }
        }
    }
}

// ============================================================
// Screen switch (from screen_switch.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_switch_to_another_pc(
    dev: *mut c_void, output_number: u32, output_to: i32, direction: i32,
) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::frontend::mouse_pipeline::switch_to_peer(
        state, &hal, output_number, output_to, direction,
    );
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_virtual_desktop_macos(dev: *mut c_void, direction: i32) {
    // Kept for C export compatibility — delegates to do_screen_switch path
    rust_do_screen_switch(dev, direction);
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_virtual_desktop(
    dev: *mut c_void, os: u8, new_index: i32, direction: i32,
) {
    // Kept for C export compatibility
    let _ = (os, new_index); // handled inside do_screen_switch path
    rust_do_screen_switch(dev, direction);
}

#[no_mangle]
pub unsafe extern "C" fn rust_do_screen_switch(dev: *mut c_void, direction: i32) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let dir = match direction {
        1 => mouse_logic::SwitchDirection::Left,
        2 => mouse_logic::SwitchDirection::Right,
        _ => return,
    };
    crate::service::frontend::mouse_pipeline::do_screen_switch(state, &hal, dir);
}

// ============================================================
// Consumer/system control routing (from state.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_send_consumer_control(dev: *mut c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let slice = core::slice::from_raw_parts(raw_report, 4);
    hal.route_consumer(state, slice);
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_system_control(dev: *mut c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let slice = core::slice::from_raw_parts(raw_report, 2);
    hal.route_system(state, slice);
}

// ============================================================
// Firmware upgrade + API message handlers (from fw_handlers.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_handle_response_byte(data: *const u8, dev: *mut c_void) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
    let fw_data = [*data.add(4), *data.add(5), *data.add(6), *data.add(7)];
    crate::service::fw_upgrade::receive_fw_byte(state, &hal, address, &fw_data);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_request_byte(data: *mut u8) {
    if data.is_null() { return; }
    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);
    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
    if let Some(response) = crate::service::fw_upgrade::send_fw_byte(state, &hal, address) {
        core::ptr::copy_nonoverlapping(response.as_ptr(), data, 8);
        hal.send_packet(&response, PacketType::ResponseByte as u8);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_msgs(ptype: u8, data: *const u8, dev: *mut c_void) {
    if data.is_null() { return; }
    super::config::handle_api_msg(ptype, *data, data.add(1), dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_read_all_msgs(dev: *mut c_void) {
    super::config::handle_api_read_all(dev);
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

#[export_name = "extract_kbd_data"]
pub unsafe extern "C" fn rust_extract_kbd_data_export(
    raw_report: *mut u8, len: i32, itf: u8, iface: *mut c_void, out: *mut u8,
) -> i32 {
    rust_extract_kbd_data(raw_report, len, itf, iface, out)
}

#[export_name = "extract_data"]
pub unsafe extern "C" fn rust_extract_data_export(
    iface: *mut c_void, val: *const u8,
) {
    rust_extract_data(iface, val);
}

#[export_name = "parse_report_descriptor"]
pub unsafe extern "C" fn rust_parse_report_descriptor_export(
    iface: *mut c_void, report: *const u8, desc_len: i32,
) {
    rust_parse_report_descriptor(iface, report, desc_len);
}

// ============================================================
// HID parser (from hid_parser_ffi.rs)
// ============================================================

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls extract_data for each
/// parsed INPUT item to populate hid_interface_t.
#[no_mangle]
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
#[no_mangle]
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
#[no_mangle]
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
