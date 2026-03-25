// Keyboard FFI — only functions still called from C remain.

#[no_mangle]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::kbd_state::release_all_keys(state);
    let empty = crate::app::structs::HidKeyboardReport::default();
    crate::hal::device::hal_queue_kbd_report(dev, &empty as *const _ as *const u8);
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
