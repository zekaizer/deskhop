// Task scheduler entry points — called from Rust scheduler in lib.rs.
// C-called FFI exports (#[no_mangle]) are at the bottom of this file.

// --- Static state ---
// DBG_COUNT and LAST_POINTER_MOVE are touched only from Core0 scheduler tasks,
// so raw static mut access via addr_of_mut! is safe.

static mut DBG_COUNT: u32 = 0;
static mut LAST_POINTER_MOVE: u32 = 0;

// --- Passthrough state ---
//
// KNOWN RACE: PT_STATE is shared mutable state accessed from BOTH cores
// without atomic protection. Writers:
//   - Core0: passthrough_task (scheduler), rust_on_tud_set_report (TinyUSB
//     device cb)
//   - Core1: rust_on_hid_mount, rust_on_hid_umount, rust_on_hid_report_received
//     (TinyUSB host callbacks)
// Readers (Core0): rust_get_*_descriptor, pt_is_active, pt_iface_count,
// pt_upstream_vid/pid (called from TinyUSB device GET_DESCRIPTOR path).
//
// This violates ADR-0002's "no contention" invariant. Race windows are narrow
// in practice (USB enumeration is the only timing-sensitive path) and the
// failure modes (torn descriptor reads, use-after-compaction) are rare but
// real. Production fix is a Core1→Core0 mount/umount/report event queue so
// PT_STATE mutation is serialized to Core0; deferred. See CONTEXT.md
// "Flagged ambiguities".

use crate::domain::passthrough::PassthroughState;

static mut PT_STATE: PassthroughState = unsafe { core::mem::zeroed() };

/// Get mutable reference to passthrough state.
/// SAFETY: Caller must accept the cross-core race documented above.
pub(crate) unsafe fn get_pt_state() -> &'static mut PassthroughState {
    &mut *core::ptr::addr_of_mut!(PT_STATE)
}

// --- Remap engine state (Core0 only) ---

use crate::domain::key_remap::RemapEngine;

static mut REMAP_ENGINE: RemapEngine = unsafe { core::mem::zeroed() };

pub(crate) unsafe fn get_remap_engine() -> &'static mut RemapEngine {
    &mut *core::ptr::addr_of_mut!(REMAP_ENGINE)
}

// ============================================================
// Scheduler task functions (Rust ABI — called from lib.rs)
// ============================================================

pub(crate) unsafe fn kick_watchdog_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    crate::service::tasks::check_system_health(state, &hal);

    use crate::hal::traits::Trace;
    let count = &mut *core::ptr::addr_of_mut!(DBG_COUNT);
    *count += 1;
    if *count >= 150 {
        *count = 0;
        hal.dump_state();
    }
}

pub(crate) unsafe fn heartbeat_output_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let state = crate::domain::structs::DeviceState::from_globals();
    crate::service::tasks::heartbeat_tick(&state, &hal);
}

pub(crate) unsafe fn packet_receiver_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    crate::service::tasks::packet_receive_tick(&mut state, &hal);
}

pub(crate) unsafe fn process_hid_queue_task() {
    let hal = crate::hal::pico::PicoHal::new();
    crate::service::tasks::process_hid_queue(&hal);
}

pub(crate) unsafe fn led_blinking_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    crate::service::tasks::led_blink_tick(&mut state, &hal);
}

pub(crate) unsafe fn screensaver_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;

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

pub(crate) unsafe fn process_kbd_queue_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let state = crate::domain::structs::DeviceState::from_globals();
    crate::service::backend::host_link::send_pending_kbd(&state, &hal);
}

pub(crate) unsafe fn process_mouse_queue_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let state = crate::domain::structs::DeviceState::from_globals();
    crate::service::backend::host_link::send_pending_mouse(&state, &hal);
}

pub(crate) unsafe fn process_uart_tx_task() {
    let hal = crate::hal::pico::PicoHal::new();
    crate::service::tasks::flush_outbox(&hal);
}

pub(crate) unsafe fn debug_log_flush_task() {
    crate::service::peer_log::debug_log_flush_task();
}

pub(crate) unsafe fn passthrough_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let pt = get_pt_state();
    crate::service::passthrough_service::passthrough_task(pt, &mut state, &hal);
}

pub(crate) unsafe fn remap_engine_tick_task() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    use crate::hal::traits::Timer;
    let now = hal.now_us_64();
    let engine = get_remap_engine();
    use crate::domain::key_remap;
    let hold_entered = key_remap::remap_engine_tick(engine, now);

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

// ============================================================
// C-callable FFI exports (#[no_mangle] / #[export_name])
// ============================================================

/// Called from C initial_setup to init remap engine + apply gaming_mode_default.
#[no_mangle]
pub unsafe extern "C" fn rust_passthrough_init() {
    let state = crate::domain::structs::DeviceState::from_globals();
    let os_a = state.cfg.config.output[0].os;
    let os_b = state.cfg.config.output[1].os;
    let engine = get_remap_engine();
    crate::domain::key_remap::remap_engine_init(engine, os_a, os_b);
    let cfg = &mut *core::ptr::addr_of_mut!(crate::domain::structs::GLOBAL_CFG);
    cfg.gaming_mode = cfg.config.gaming_mode_default != 0;
}

/// Called from C set_active_output / Rust callbacks.
#[export_name = "release_all_keys"]
pub unsafe extern "C" fn rust_release_all_keys() {
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    crate::service::backend::host_link::release_all_keys(&mut state, &hal);
}

// --- Passthrough state accessors for C (usb_descriptors.c) ---

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
