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

    let state = &mut *crate::app::state::rust_get_app_state();

    if length < KBD_REPORT_LENGTH as i32 {
        return;
    }

    if state.reboot_requested {
        return;
    }

    // Extract keyboard data from raw HID report → standard 8-byte report
    let mut new_report = [0u8; 8]; // hid_keyboard_report_t
    device::hal_extract_kbd_data(raw_report, length, itf, iface, new_report.as_mut_ptr());

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
