// Task FFI wrappers — scheduler entry points called from tasks.c.

use core::ffi::c_void;

// --- Static state ---
// These are only accessed from a single core's task scheduler, so
// raw static mut access via addr_of_mut! is safe in practice.

static mut DBG_COUNT: u32 = 0;
static mut LAST_POINTER_MOVE: u32 = 0;

// --- Watchdog / heartbeat ---

#[export_name = "kick_watchdog_task"]
pub unsafe extern "C" fn rust_kick_watchdog_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::check_system_health(state, &hal);

    // Debug: dump state every ~5s (30Hz x 150)
    use crate::hal::traits::Trace;
    let count = &mut *core::ptr::addr_of_mut!(DBG_COUNT);
    *count += 1;
    if *count >= 150 {
        *count = 0;
        hal.dump_state();
    }
}

#[export_name = "heartbeat_output_task"]
pub unsafe extern "C" fn rust_heartbeat_output_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::heartbeat_tick(state, &hal);
}

// --- Packet receiver task ---

#[export_name = "packet_receiver_task"]
pub unsafe extern "C" fn rust_packet_receiver_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::packet_receive_tick(state, &hal);
}

// --- HID output queue task ---

#[export_name = "process_hid_queue_task"]
pub unsafe extern "C" fn rust_process_hid_queue_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    crate::service::tasks::process_hid_queue(&hal);
}

// --- LED blinking task ---

#[export_name = "led_blinking_task"]
pub unsafe extern "C" fn rust_led_blinking_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::led_blink_tick(state, &hal);
}

// --- Screensaver task ---

#[export_name = "screensaver_task"]
pub unsafe extern "C" fn rust_screensaver_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;

    // Generate report from static state (pong/jitter)
    let mut report_bytes = [0u8; 8];
    let role = state.cfg.board_role as usize;
    if role < state.cfg.config.output.len() {
        match state.cfg.config.output[role].screensaver.mode {
            1 => super::util::rust_screensaver_pong(report_bytes.as_mut_ptr()),
            2 => super::util::rust_screensaver_jitter(report_bytes.as_mut_ptr()),
            _ => {}
        }
    }

    let last_move = &mut *core::ptr::addr_of_mut!(LAST_POINTER_MOVE);
    if let Some(t) = crate::service::tasks::screensaver_tick(state, &hal, *last_move, &report_bytes) {
        *last_move = t;
    }
}

// --- Keyboard queue tasks ---

#[export_name = "release_all_keys"]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::backend::host_link::release_all_keys(state, &hal);
}

#[export_name = "process_kbd_queue_task"]
pub unsafe extern "C" fn rust_process_kbd_queue_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::backend::host_link::send_pending_kbd(state, &hal);
}

#[export_name = "process_mouse_queue_task"]
pub unsafe extern "C" fn rust_process_mouse_queue_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::backend::host_link::send_pending_mouse(state, &hal);
}

// --- UART TX task ---

#[export_name = "process_uart_tx_task"]
pub unsafe extern "C" fn rust_process_uart_tx_task(dev: *mut c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    crate::service::tasks::flush_outbox(&hal);
}
