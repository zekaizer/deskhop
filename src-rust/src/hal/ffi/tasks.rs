// Task FFI wrappers — scheduler entry points called from tasks.c.

// --- Static state ---
// These are only accessed from a single core's task scheduler, so
// raw static mut access via addr_of_mut! is safe in practice.

static mut DBG_COUNT: u32 = 0;
static mut LAST_POINTER_MOVE: u32 = 0;

// --- Passthrough state (Core0 only) ---

use crate::domain::passthrough::PassthroughState;

static mut PT_STATE: PassthroughState = unsafe { core::mem::zeroed() };

/// Get mutable reference to passthrough state (Core0 only).
pub(crate) unsafe fn get_pt_state() -> &'static mut PassthroughState {
    &mut *core::ptr::addr_of_mut!(PT_STATE)
}

// --- Watchdog / heartbeat ---

#[export_name = "kick_watchdog_task"]
pub unsafe extern "C" fn rust_kick_watchdog_task() {
    let hal = crate::hal::pico::PicoHal::new();
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
pub unsafe extern "C" fn rust_heartbeat_output_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::heartbeat_tick(state, &hal);
}

// --- Packet receiver task ---

#[export_name = "packet_receiver_task"]
pub unsafe extern "C" fn rust_packet_receiver_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::packet_receive_tick(state, &hal);
}

// --- HID output queue task ---

#[export_name = "process_hid_queue_task"]
pub unsafe extern "C" fn rust_process_hid_queue_task() {
    let hal = crate::hal::pico::PicoHal::new();
    crate::service::tasks::process_hid_queue(&hal);
}

// --- LED blinking task ---

#[export_name = "led_blinking_task"]
pub unsafe extern "C" fn rust_led_blinking_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::led_blink_tick(state, &hal);
}

// --- Screensaver task ---

#[export_name = "screensaver_task"]
pub unsafe extern "C" fn rust_screensaver_task() {
    let hal = crate::hal::pico::PicoHal::new();
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
pub unsafe extern "C" fn rust_release_all_keys_state() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::backend::host_link::release_all_keys(state, &hal);
}

#[export_name = "process_kbd_queue_task"]
pub unsafe extern "C" fn rust_process_kbd_queue_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::backend::host_link::send_pending_kbd(state, &hal);
}

#[export_name = "process_mouse_queue_task"]
pub unsafe extern "C" fn rust_process_mouse_queue_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::backend::host_link::send_pending_mouse(state, &hal);
}

// --- Passthrough task ---

#[export_name = "passthrough_task"]
pub unsafe extern "C" fn rust_passthrough_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let pt = get_pt_state();
    crate::service::passthrough_service::passthrough_task(pt, state, &hal);
}

// --- Remap engine tick task ---

use crate::domain::key_remap::RemapEngine;

static mut REMAP_ENGINE: RemapEngine = unsafe { core::mem::zeroed() };

/// Get mutable reference to remap engine (Core0 only).
pub(crate) unsafe fn get_remap_engine() -> &'static mut RemapEngine {
    &mut *core::ptr::addr_of_mut!(REMAP_ENGINE)
}

#[export_name = "remap_engine_tick_task"]
pub unsafe extern "C" fn rust_remap_engine_tick_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    use crate::hal::traits::Timer;
    let now = hal.now_us_64();
    let engine = get_remap_engine();
    use crate::domain::key_remap;
    let hold_entered = key_remap::remap_engine_tick(engine, now);

    // Safety net: emit pending tap from previous process_keyboard_report
    let mut pending = crate::domain::structs::HidKeyboardReport::default();
    if key_remap::remap_engine_get_pending(engine, &mut pending) {
        let mut tap_press = crate::domain::kbd_state::combine_kbd_states(state);
        for k in tap_press.keycode.iter_mut() {
            if *k == 0 {
                *k = pending.keycode[0];
                break;
            }
        }
        tap_press.modifier |= pending.modifier;
        use crate::service::router::ReportRouter;
        let report_bytes: [u8; 8] = core::mem::transmute(tap_press);
        hal.route_kbd(state, &report_bytes);
        let release = crate::domain::kbd_state::combine_kbd_states(state);
        let release_bytes: [u8; 8] = core::mem::transmute(release);
        hal.route_kbd(state, &release_bytes);
    }

    if hold_entered {
        let combined = crate::domain::kbd_state::combine_kbd_states(state);
        let bytes: [u8; 8] = core::mem::transmute(combined);
        use crate::service::router::ReportRouter;
        hal.route_kbd(state, &bytes);
    }
}

/// Initialize remap engine and apply gaming_mode_default. Called once from initial_setup.
#[no_mangle]
pub unsafe extern "C" fn rust_passthrough_init() {
    let state = crate::domain::structs::DeviceState::from_globals();
    let os_a = state.cfg.config.output[0].os;
    let os_b = state.cfg.config.output[1].os;
    let engine = get_remap_engine();
    crate::domain::key_remap::remap_engine_init(engine, os_a, os_b);
    // Apply gaming_mode_default from config
    let cfg = &mut *core::ptr::addr_of_mut!(crate::domain::structs::GLOBAL_CFG);
    cfg.gaming_mode = cfg.config.gaming_mode_default != 0;
}

// --- Passthrough FFI accessors for C (usb_descriptors.c) ---

#[no_mangle]
pub unsafe extern "C" fn pt_is_active() -> bool {
    (*core::ptr::addr_of!(PT_STATE)).active
}

#[no_mangle]
pub unsafe extern "C" fn pt_iface_count() -> u8 {
    (*core::ptr::addr_of!(PT_STATE)).iface_count
}

#[no_mangle]
pub unsafe extern "C" fn pt_upstream_vid() -> u16 {
    (*core::ptr::addr_of!(PT_STATE)).upstream_vid
}

#[no_mangle]
pub unsafe extern "C" fn pt_upstream_pid() -> u16 {
    (*core::ptr::addr_of!(PT_STATE)).upstream_pid
}

#[no_mangle]
pub unsafe extern "C" fn pt_config_desc(out_len: *mut u16) -> *const u8 {
    let (ptr, len) = super::pt_config_desc_ptr();
    if !out_len.is_null() { *out_len = len; }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn pt_get_report_desc(device_instance: u8, out_len: *mut u16) -> *const u8 {
    let pt = &*core::ptr::addr_of!(PT_STATE);
    match crate::domain::passthrough::get_report_desc(pt, device_instance) {
        Some((desc, len)) => {
            if !out_len.is_null() { *out_len = len; }
            desc.as_ptr()
        }
        None => core::ptr::null(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn pt_iface_protocol(idx: u8) -> u8 {
    let pt = &*core::ptr::addr_of!(PT_STATE);
    if (idx as usize) < pt.ifaces.len() { pt.ifaces[idx as usize].itf_protocol } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn pt_iface_desc_len(idx: u8) -> u16 {
    let pt = &*core::ptr::addr_of!(PT_STATE);
    if (idx as usize) < pt.ifaces.len() { pt.ifaces[idx as usize].desc_len } else { 0 }
}

// --- UART TX task ---

#[export_name = "process_uart_tx_task"]
pub unsafe extern "C" fn rust_process_uart_tx_task() {
    let hal = crate::hal::pico::PicoHal::new();
    crate::service::tasks::flush_outbox(&hal);
}
