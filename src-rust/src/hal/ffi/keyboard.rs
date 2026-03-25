// HID input FFI — direct exports replacing C wrappers in hid_input.c.

/// Called from C: set_active_output → release_all_keys
#[export_name = "release_all_keys"]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::kbd_state::release_all_keys(state);
    let empty = crate::app::structs::HidKeyboardReport::default();
    crate::hal::device::hal_queue_kbd_report(dev, &empty as *const _ as *const u8);
}

/// Called from Rust scheduler (process_kbd_queue_task)
#[export_name = "process_kbd_queue_task"]
pub unsafe extern "C" fn rust_process_kbd_queue_task(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if !state.tud_connected { return; }
    let mut report = [0u8; 8];
    if !crate::hal::device::hal_kbd_queue_peek(dev, report.as_mut_ptr()) { return; }
    if crate::hal::device::hal_tud_suspended() { crate::hal::device::hal_tud_remote_wakeup(); }
    if !crate::hal::device::hal_tud_hid_n_ready(crate::app::constants::ITF_NUM_HID) { return; }
    if crate::hal::device::hal_tud_hid_keyboard_report(1, report[0], report[2..].as_ptr()) {
        crate::hal::device::hal_kbd_queue_remove(dev, report.as_mut_ptr());
    }
}

/// Called from Rust scheduler (process_mouse_queue_task)
#[export_name = "process_mouse_queue_task"]
pub unsafe extern "C" fn rust_process_mouse_queue_task(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if !state.tud_connected { return; }
    let mut r = [0u8; 8];
    if !crate::hal::device::hal_mouse_queue_peek(dev, r.as_mut_ptr()) { return; }
    if crate::hal::device::hal_tud_suspended() { crate::hal::device::hal_tud_remote_wakeup(); }
    if !crate::hal::device::hal_tud_hid_n_ready(crate::app::constants::ITF_NUM_HID) { return; }
    let mode = r[7];
    let buttons = r[0];
    if crate::hal::device::hal_tud_mouse_report(
        mode, buttons,
        i16::from_le_bytes([r[1], r[2]]), i16::from_le_bytes([r[3], r[4]]),
        r[5] as i8, r[6] as i8,
    ) {
        crate::hal::device::hal_mouse_queue_remove(dev, r.as_mut_ptr());
    }
}
