use crate::hal::traits::*;

#[no_mangle]
pub unsafe extern "C" fn rust_send_consumer_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    if state.is_active_output() {
        hal.push_consumer_control(raw_report);
        let role = state.board_role as usize;
        if role < state.last_activity.len() {
            state.last_activity[role] = hal.now_us_64();
        }
    } else {
        hal.send_packet(
            raw_report, crate::app::constants::PacketType::ConsumerControl as u8, 4,
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_system_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    if state.is_active_output() {
        hal.push_system_control(raw_report);
        let role = state.board_role as usize;
        if role < state.last_activity.len() {
            state.last_activity[role] = hal.now_us_64();
        }
    } else {
        hal.send_packet(
            raw_report, crate::app::constants::PacketType::SystemControl as u8, 1,
        );
    }
}
