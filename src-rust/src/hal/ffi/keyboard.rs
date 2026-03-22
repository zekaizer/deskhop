#[no_mangle]
pub unsafe extern "C" fn rust_key_in_report(key: u8, report: *const u8) -> bool {
    if report.is_null() { return false; }
    let keycode = core::slice::from_raw_parts(report.add(2), 6);
    keycode.iter().any(|&k| k == key)
}

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
pub unsafe extern "C" fn rust_update_kbd_state(report: *const u8, device_idx: u8) {
    if report.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let kbd = &*(report as *const crate::app::structs::HidKeyboardReport);
    crate::app::kbd_state::update_kbd_state(state, kbd, device_idx);
}

#[no_mangle]
pub unsafe extern "C" fn rust_update_remote_kbd_state(report: *const u8) {
    if report.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let kbd = &*(report as *const crate::app::structs::HidKeyboardReport);
    crate::app::kbd_state::update_remote_kbd_state(state, kbd);
}

#[no_mangle]
pub unsafe extern "C" fn rust_combine_kbd_states(out: *mut u8) {
    if out.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();
    let combined = crate::app::kbd_state::combine_kbd_states(state);
    core::ptr::copy_nonoverlapping(&combined as *const _ as *const u8, out, 8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_key(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::kbd_state::send_key(dev, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::kbd_state::release_all_keys(dev, state);
}
