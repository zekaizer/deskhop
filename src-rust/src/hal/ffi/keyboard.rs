// HID queue dispatch FFI wrappers — delegate to app::host_link.

#[export_name = "release_all_keys"]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::host_link::release_all_keys(state, &hal);
}

#[export_name = "process_kbd_queue_task"]
pub unsafe extern "C" fn rust_process_kbd_queue_task(dev: *mut core::ffi::c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::host_link::send_pending_kbd(state, &hal);
}

#[export_name = "process_mouse_queue_task"]
pub unsafe extern "C" fn rust_process_mouse_queue_task(dev: *mut core::ffi::c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::host_link::send_pending_mouse(state, &hal);
}
