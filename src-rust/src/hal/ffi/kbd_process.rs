use core::ffi::c_void;
use crate::hal::traits::*;
use crate::app::structs::KBD_REPORT_LENGTH;

/// Full keyboard report processing pipeline.
/// Called directly from TinyUSB callback (process_report_f signature).

#[export_name = "process_keyboard_report"]
pub unsafe extern "C" fn rust_process_keyboard_report(
    raw_report: *mut u8,
    length: i32,
    itf: u8,
    iface: *mut c_void,  // hid_interface_t*
) {
    if raw_report.is_null() || iface.is_null() { return; }

    let state = crate::app::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);

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

    // Check hotkeys — fully in Rust, no C roundtrip
    let report_for_hotkey = crate::app::keyboard::KeyboardReport {
        modifier: new_report[0],
        reserved: new_report[1],
        keycode: [new_report[2], new_report[3], new_report[4],
                  new_report[5], new_report[6], new_report[7]],
    };
    if let Some(m) = crate::app::keyboard::check_all_hotkeys(&report_for_hotkey) {
        // Execute the hotkey action
        super::hotkey_dispatch::execute_hotkey_action(dev, m.action);
        if m.acknowledge {
            hal.blink();
        }
        if !m.pass_to_os {
            return;
        }
    }

    // Send key via combined report — route based on active output
    use crate::app::router::ReportRouter;
    let combined = crate::app::kbd_state::combine_kbd_states(state);
    hal.route_kbd(state, &combined as *const _ as *const u8);
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
    let ifc = crate::app::structs::iface_from_ptr(iface);

    let mut new_report = [0u8; 4]; // CONSUMER_CONTROL_LENGTH

    if ifc.consumer.is_variable {
        let report_id = *raw_report;
        let kbd = crate::app::structs::get_keyboard(ifc, report_id);
        let max_buttons = 16i32; // MAX_CC_BUTTONS
        let max_bits = 8 * (length - 1);
        let limit = if max_buttons < max_bits { max_buttons } else { max_bits };

        for i in 0..limit {
            let bit_idx = i % 8;
            let byte_idx = i >> 3;
            if (*raw_report.add((byte_idx + 1) as usize) >> bit_idx) & 1 != 0 {
                let idx = i as usize;
                if idx < kbd.cc_array.len() {
                    let cc_val = kbd.cc_array[idx];
                    new_report[0] = (cc_val & 0xFF) as u8;
                    new_report[1] = ((cc_val >> 8) & 0xFF) as u8;
                }
            }
        }
    } else {
        for i in 0..core::cmp::min((length - 1) as usize, 4) {
            new_report[i] = *raw_report.add(i + 1);
        }
    }

    // Route: local queue if active output, UART if not
    let dev = crate::app::structs::get_global_device() as *mut _ as *mut c_void;
    crate::hal::ffi::state::rust_send_consumer_control(dev, new_report.as_ptr());
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

    let dev = crate::app::structs::get_global_device() as *mut _ as *mut c_void;
    crate::hal::ffi::state::rust_send_system_control(dev, report.as_ptr());
}
