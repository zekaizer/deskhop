use crate::domain::screensaver::{PongState, JitterState, MouseReport as SSMouseReport};

static mut PONG_STATE: PongState = PongState::new();
static mut JITTER_STATE: JitterState = JitterState::new();

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong(out: *mut u8) {
    let state = &mut *core::ptr::addr_of_mut!(PONG_STATE);
    write_report(out, &state.step());
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter(out: *mut u8) {
    let state = &mut *core::ptr::addr_of_mut!(JITTER_STATE);
    write_report(out, &state.step());
}

unsafe fn write_report(out: *mut u8, r: &SSMouseReport) {
    if out.is_null() { return; }
    *out = r.buttons;
    let xb = r.x.to_le_bytes(); *out.add(1) = xb[0]; *out.add(2) = xb[1];
    let yb = r.y.to_le_bytes(); *out.add(3) = yb[0]; *out.add(4) = yb[1];
    *out.add(5) = r.wheel as u8; *out.add(6) = r.pan as u8; *out.add(7) = r.mode;
}
