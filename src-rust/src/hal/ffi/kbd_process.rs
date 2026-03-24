use core::ffi::c_void;
use crate::hal::device;
use crate::app::structs::KBD_REPORT_LENGTH;

/// Full keyboard report processing pipeline — replaces C process_keyboard_report.
/// Called from TinyUSB callback context via C shim.
#[no_mangle]
pub unsafe extern "C" fn rust_process_keyboard_report(
    raw_report: *mut u8,
    length: i32,
    itf: u8,
    iface: *mut c_void,  // hid_interface_t*
    dev: *mut c_void,    // device_t*
) {
    if raw_report.is_null() || iface.is_null() || dev.is_null() {
        return;
    }

    let state = crate::app::structs::device_from_ptr(dev);

    if length < KBD_REPORT_LENGTH as i32 {
        return;
    }

    if state.reboot_requested {
        return;
    }

    // Extract keyboard data — call Rust extract directly (no C roundtrip)
    let mut new_report = [0u8; 8];
    super::kbd_extract::rust_extract_kbd_data(raw_report, length, itf, iface, new_report.as_mut_ptr());

    // Update keyboard state for this device
    let kbd = &*(new_report.as_ptr() as *const crate::app::structs::HidKeyboardReport);
    crate::app::kbd_state::update_kbd_state(state, kbd, itf);

    // Check hotkeys
    let mut pass_to_os: u8 = 0;
    let mut acknowledge: u8 = 0;
    let matched = device::hal_check_all_hotkeys(
        new_report.as_ptr(), &mut pass_to_os, &mut acknowledge,
    );

    if matched == 0 {
        // Hotkey was matched and handler executed
        if acknowledge != 0 {
            device::hal_blink_led(dev);
        }
        if pass_to_os == 0 {
            return; // Don't pass to OS
        }
    }

    // Send key via combined report
    crate::app::kbd_state::send_key(dev, state);
}

/// Rust implementation of process_consumer_report
#[no_mangle]
pub unsafe extern "C" fn rust_process_consumer_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    iface: *mut c_void,
    dev: *mut c_void,
) {
    if raw_report.is_null() || iface.is_null() || length < 2 { return; }
    let _state = crate::app::structs::device_from_ptr(dev);

    let mut new_report = [0u8; 4]; // CONSUMER_CONTROL_LENGTH

    if device::hal_get_consumer_is_variable(iface) {
        let report_id = *raw_report;
        let max_buttons = 16i32; // MAX_CC_BUTTONS
        let max_bits = 8 * (length - 1);
        let limit = if max_buttons < max_bits { max_buttons } else { max_bits };

        for i in 0..limit {
            let bit_idx = i % 8;
            let byte_idx = i >> 3;
            if (*raw_report.add((byte_idx + 1) as usize) >> bit_idx) & 1 != 0 {
                let cc_val = device::hal_get_cc_array_value(iface, report_id, i);
                new_report[0] = (cc_val & 0xFF) as u8;
                new_report[1] = ((cc_val >> 8) & 0xFF) as u8;
            }
        }
    } else {
        for i in 0..core::cmp::min((length - 1) as usize, 4) {
            new_report[i] = *raw_report.add(i + 1);
        }
    }

    // Route: local queue if active output, UART if not
    crate::hal::ffi::state::rust_send_consumer_control(dev, new_report.as_ptr());
}

/// Rust implementation of process_system_report
#[no_mangle]
pub unsafe extern "C" fn rust_process_system_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    _iface: *mut c_void,
    dev: *mut c_void,
) {
    if raw_report.is_null() || length < 2 { return; }

    let report = [*raw_report.add(1), 0];

    // Route: local queue if active output, UART if not
    crate::hal::ffi::state::rust_send_system_control(dev, report.as_ptr());
}
