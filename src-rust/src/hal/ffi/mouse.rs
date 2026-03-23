use crate::app::mouse;

#[no_mangle]
pub extern "C" fn rust_move_and_keep_on_screen(position: i32, offset: i32) -> i32 {
    mouse::move_and_keep_on_screen(position, offset)
}

#[no_mangle]
pub extern "C" fn rust_is_screen_switch_needed(position: i32, offset: i32, threshold: u16) -> i32 {
    mouse::is_screen_switch_needed(position, offset, threshold)
}

#[no_mangle]
pub extern "C" fn rust_calculate_mouse_acceleration_factor(x: i32, y: i32, enabled: bool) -> f32 {
    mouse::calculate_mouse_acceleration_factor(x, y, enabled)
}

#[no_mangle]
pub extern "C" fn rust_scale_y_coordinate(y: i16, ft: i32, fb: i32, tt: i32, tb: i32) -> i16 {
    mouse::scale_y_coordinate(y, (ft, fb), (tt, tb))
}

#[no_mangle]
pub unsafe extern "C" fn rust_update_mouse_position(
    move_x: i32, move_y: i32, wheel: i32, pan: i32, buttons: i32,
) -> u8 {
    let state = crate::app::structs::get_global_device();
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return 0; }
    let output = &state.config.output[idx];
    let values = crate::app::mouse_logic::MouseValues { move_x, move_y, wheel, pan, buttons };
    let (nx, ny, dir) = crate::app::mouse_logic::update_mouse_position(
        state.pointer_x, state.pointer_y, &values,
        output.speed_x, output.speed_y,
        state.mouse_zoom, state.config.enable_acceleration != 0, state.config.jump_threshold,
    );
    state.pointer_x = nx;
    state.pointer_y = ny;
    state.mouse_buttons = buttons as i16;
    match dir {
        crate::app::mouse_logic::SwitchDirection::None => 0,
        crate::app::mouse_logic::SwitchDirection::Left => 1,
        crate::app::mouse_logic::SwitchDirection::Right => 2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_create_mouse_report(
    wheel: i32, pan: i32, buttons: i32, move_x: i32, move_y: i32, out: *mut u8,
) {
    if out.is_null() { return; }
    let state = crate::app::structs::get_global_device();
    let values = crate::app::mouse_logic::MouseValues { move_x, move_y, wheel, pan, buttons };
    let r = crate::app::mouse_logic::create_mouse_report(
        state.pointer_x, state.pointer_y, &values, state.relative_mouse, state.gaming_mode,
    );
    *out = r.buttons;
    let xb = r.x.to_le_bytes(); *out.add(1) = xb[0]; *out.add(2) = xb[1];
    let yb = r.y.to_le_bytes(); *out.add(3) = yb[0]; *out.add(4) = yb[1];
    *out.add(5) = r.wheel as u8; *out.add(6) = r.pan as u8; *out.add(7) = r.mode;
}

#[no_mangle]
pub unsafe extern "C" fn rust_decide_screen_switch(direction: u8) -> u8 {
    use crate::app::mouse_logic::*;
    let state = crate::app::structs::get_global_device();
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return 0; }
    let output = &state.config.output[idx];
    let dir = match direction { 1 => SwitchDirection::Left, 2 => SwitchDirection::Right, _ => return 0 };
    let ctx = SwitchContext {
        switch_lock: state.switch_lock, gaming_mode: state.gaming_mode,
        mouse_buttons: state.mouse_buttons, screen_pos: output.pos,
        screen_index: output.screen_index, screen_count: output.screen_count,
    };
    match decide_screen_switch(dir, &ctx) {
        ScreenSwitchAction::Nothing => 0,
        ScreenSwitchAction::SwitchToOtherPc => 1,
        ScreenSwitchAction::SwitchVirtualDesktop { .. } => 2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_output_mouse_report(dev: *mut core::ffi::c_void, report: *const u8) {
    if report.is_null() { return; }
    let state = crate::app::structs::device_from_ptr(dev);
    if state.is_active_output() {
        crate::hal::device::hal_queue_mouse_report(dev, report);
        let role = state.board_role as usize;
        if role < state.last_activity.len() {
            state.last_activity[role] = crate::hal::device::hal_time_us_64();
        }
    } else {
        crate::hal::device::hal_queue_packet(
            report, crate::app::constants::PacketType::MouseReport as u8, 8,
        );
    }
}
