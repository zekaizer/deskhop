use crate::app::state::AppState;

#[no_mangle]
pub extern "C" fn rust_get_app_state() -> *mut AppState {
    crate::app::state::rust_get_app_state()
}

#[no_mangle]
pub static RUST_SIZEOF_APP_STATE: u32 = core::mem::size_of::<AppState>() as u32;

#[no_mangle]
pub unsafe extern "C" fn rust_send_consumer_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();
    if !state.is_active_output() {
        crate::hal::device::hal_queue_packet(
            raw_report, crate::app::constants::PacketType::ConsumerControl as u8, 4,
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_system_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();
    if !state.is_active_output() {
        crate::hal::device::hal_queue_packet(
            raw_report, crate::app::constants::PacketType::SystemControl as u8, 1,
        );
    }
}
