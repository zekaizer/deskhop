// Keyboard FFI — only functions still called from C remain.

#[no_mangle]
pub unsafe extern "C" fn rust_check_specific_hotkey(
    modifier: u8, keys: *const u8, key_count: u8, report: *const u8,
) -> bool {
    if report.is_null() { return false; }
    let report_modifier = *report;
    if modifier != (report_modifier & modifier) { return false; }
    let keycode = core::slice::from_raw_parts(report.add(2), 6);
    if keys.is_null() { return true; }
    for i in 0..key_count as usize {
        if !keycode.iter().any(|&k| k == *keys.add(i)) { return false; }
    }
    true
}

#[no_mangle]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::kbd_state::release_all_keys(dev, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_queue_kbd_report(dev: *mut core::ffi::c_void, report: *const u8) {
    let state = crate::app::structs::device_from_ptr(dev);
    if state.tud_connected && !report.is_null() {
        crate::hal::device::hal_queue_kbd_report(dev, report);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_queue_mouse_report(dev: *mut core::ffi::c_void, report: *const u8) {
    let state = crate::app::structs::device_from_ptr(dev);
    if state.tud_connected && !report.is_null() {
        crate::hal::device::hal_queue_mouse_report(dev, report);
    }
}
