// Task FFI wrappers — thin entry points that delegate to app::tasks.

use core::ffi::c_void;

static mut DBG_COUNT: u32 = 0;

#[export_name = "kick_watchdog_task"]
pub unsafe extern "C" fn rust_kick_watchdog_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::tasks::check_system_health(state, &hal);

    // Debug: dump state every ~5s (30Hz × 150)
    use crate::hal::traits::Trace;
    DBG_COUNT += 1;
    if DBG_COUNT >= 150 {
        DBG_COUNT = 0;
        hal.dump_state();
    }
}

static mut LAST_POINTER_MOVE: u32 = 0;

#[export_name = "screensaver_task"]
pub unsafe extern "C" fn rust_screensaver_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);

    // Generate report from static state (pong/jitter)
    let mut report_bytes = [0u8; 8];
    let role = state.board_role as usize;
    if role < state.config.output.len() {
        match state.config.output[role].screensaver.mode {
            1 => super::screensaver::rust_screensaver_pong(report_bytes.as_mut_ptr()),
            2 => super::screensaver::rust_screensaver_jitter(report_bytes.as_mut_ptr()),
            _ => {}
        }
    }

    if let Some(t) = crate::app::tasks::screensaver_tick(state, &hal, LAST_POINTER_MOVE, &report_bytes) {
        LAST_POINTER_MOVE = t;
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_heartbeat_output_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::app::structs::device_from_ptr(dev);
    crate::app::tasks::heartbeat_tick(state, &hal);
}
