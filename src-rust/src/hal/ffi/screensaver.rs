use crate::app::screensaver::{PongState, JitterState, MouseReport as SSMouseReport};

static mut PONG_STATE: PongState = PongState::new();
static mut JITTER_STATE: JitterState = JitterState::new();

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong(out: *mut u8) {
    write_report(out, &PONG_STATE.step());
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter(out: *mut u8) {
    write_report(out, &JITTER_STATE.step());
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_reset() {
    PONG_STATE = PongState::new();
}

#[no_mangle]
pub extern "C" fn rust_screensaver_should_activate(
    mode: u8, only_if_inactive: u8, idle_time_us: u64, max_time_us: u64,
    inactivity_us: u64, is_active_output: bool, tud_ready: bool,
    last_move_us: u32, current_time_us: u32,
) -> bool {
    let config = crate::app::screensaver::ScreensaverConfig {
        mode, only_if_inactive: only_if_inactive != 0, idle_time_us, max_time_us,
    };
    crate::app::screensaver::should_activate(
        &config, inactivity_us, is_active_output, tud_ready, last_move_us, current_time_us,
    )
}

unsafe fn write_report(out: *mut u8, r: &SSMouseReport) {
    if out.is_null() { return; }
    *out = r.buttons;
    let xb = r.x.to_le_bytes(); *out.add(1) = xb[0]; *out.add(2) = xb[1];
    let yb = r.y.to_le_bytes(); *out.add(3) = yb[0]; *out.add(4) = yb[1];
    *out.add(5) = r.wheel as u8; *out.add(6) = r.pan as u8; *out.add(7) = r.mode;
}
