// Task FFI wrappers — scheduler entry points called from tasks.c.

use core::ffi::c_void;

// --- Static state ---

static mut DBG_COUNT: u32 = 0;
static mut LAST_POINTER_MOVE: u32 = 0;

// --- Watchdog / heartbeat ---

#[export_name = "kick_watchdog_task"]
pub unsafe extern "C" fn rust_kick_watchdog_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::tasks::check_system_health(state, &hal);

    // Debug: dump state every ~5s (30Hz x 150)
    use crate::hal::traits::Trace;
    DBG_COUNT += 1;
    if DBG_COUNT >= 150 {
        DBG_COUNT = 0;
        hal.dump_state();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_heartbeat_output_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::tasks::heartbeat_tick(state, &hal);
}

// --- Screensaver task ---

#[export_name = "screensaver_task"]
pub unsafe extern "C" fn rust_screensaver_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);

    // Generate report from static state (pong/jitter)
    let mut report_bytes = [0u8; 8];
    let role = state.board_role as usize;
    if role < state.config.output.len() {
        match state.config.output[role].screensaver.mode {
            1 => super::util::rust_screensaver_pong(report_bytes.as_mut_ptr()),
            2 => super::util::rust_screensaver_jitter(report_bytes.as_mut_ptr()),
            _ => {}
        }
    }

    if let Some(t) = crate::service::tasks::screensaver_tick(state, &hal, LAST_POINTER_MOVE, &report_bytes) {
        LAST_POINTER_MOVE = t;
    }
}

// --- Keyboard queue tasks ---

#[export_name = "release_all_keys"]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::backend::host_link::release_all_keys(state, &hal);
}

#[export_name = "process_kbd_queue_task"]
pub unsafe extern "C" fn rust_process_kbd_queue_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::backend::host_link::send_pending_kbd(state, &hal);
}

#[export_name = "process_mouse_queue_task"]
pub unsafe extern "C" fn rust_process_mouse_queue_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    crate::service::backend::host_link::send_pending_mouse(state, &hal);
}

// --- UART TX task ---

#[export_name = "process_uart_tx_task"]
pub unsafe extern "C" fn rust_process_uart_tx_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    crate::service::tasks::flush_outbox(&hal);
}
