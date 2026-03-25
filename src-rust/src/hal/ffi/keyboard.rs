// HID input FFI — direct exports replacing C wrappers in hid_input.c.

use crate::hal::traits::*;

/// Called from C: set_active_output → release_all_keys
/// Also called from other ffi modules — cannot be cfg(not(test)).
#[export_name = "release_all_keys"]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::kbd_state::release_all_keys(state);
    let empty = crate::app::structs::HidKeyboardReport::default();
    // Keep device:: call — this function is used across ffi modules including test builds
    crate::hal::device::hal_queue_kbd_report(dev, &empty as *const _ as *const u8);
}

/// Called from Rust scheduler (process_kbd_queue_task)
#[cfg(not(test))]
#[export_name = "process_kbd_queue_task"]
pub unsafe extern "C" fn rust_process_kbd_queue_task(dev: *mut core::ffi::c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    if !state.tud_connected { return; }
    let mut report = [0u8; 8];
    if !hal.peek_kbd_report(report.as_mut_ptr()) { return; }
    if hal.is_suspended() { hal.remote_wakeup(); }
    if !hal.hid_ready(crate::app::constants::ITF_NUM_HID) { return; }
    if hal.send_keyboard_report(1, report[0], report[2..].as_ptr()) {
        hal.pop_kbd_report(report.as_mut_ptr());
    }
}

/// Called from Rust scheduler (process_mouse_queue_task)
#[cfg(not(test))]
#[export_name = "process_mouse_queue_task"]
pub unsafe extern "C" fn rust_process_mouse_queue_task(dev: *mut core::ffi::c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    if !state.tud_connected { return; }
    let mut r = [0u8; 8];
    if !hal.peek_mouse_report(r.as_mut_ptr()) { return; }
    if hal.is_suspended() { hal.remote_wakeup(); }
    if !hal.hid_ready(crate::app::constants::ITF_NUM_HID) { return; }
    let mode = r[7];
    let buttons = r[0];
    if hal.send_mouse_report(
        mode, buttons,
        i16::from_le_bytes([r[1], r[2]]), i16::from_le_bytes([r[3], r[4]]),
        r[5] as i8, r[6] as i8,
    ) {
        hal.pop_mouse_report(r.as_mut_ptr());
    }
}
