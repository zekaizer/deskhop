use crate::app::router::ReportRouter;

#[no_mangle]
pub unsafe extern "C" fn rust_send_consumer_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    hal.route_consumer(state, raw_report);
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_system_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    hal.route_system(state, raw_report);
}
