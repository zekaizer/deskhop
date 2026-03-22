// FFI exports — functions callable from C.
// All functions here are #[no_mangle] pub extern "C" and form the
// Rust→C API boundary. Internal Rust functions should NOT be here.

use crate::app::{constants, crc, mouse, packet};

// ---- Checksum / CRC ----

#[no_mangle]
pub unsafe extern "C" fn rust_calc_checksum(data: *const u8, length: i32) -> u8 {
    if data.is_null() || length <= 0 {
        return 0;
    }
    crc::calc_checksum(core::slice::from_raw_parts(data, length as usize))
}

#[no_mangle]
pub unsafe extern "C" fn rust_calc_crc32(data: *const u8, length: usize) -> u32 {
    if data.is_null() {
        return 0;
    }
    crc::calc_crc32(core::slice::from_raw_parts(data, length))
}

#[no_mangle]
pub extern "C" fn rust_crc32_iter(crc: u32, byte: u8) -> u32 {
    crc::crc32_iter(crc, byte)
}

/// Verify checksum of a uart_packet_t.
/// Layout: [type(1) + data(8) + checksum(1)] = 10 bytes
#[no_mangle]
pub unsafe extern "C" fn rust_verify_checksum(packet: *const u8) -> bool {
    if packet.is_null() {
        return false;
    }
    let data = core::slice::from_raw_parts(packet.add(1), 8);
    let checksum = *packet.add(9);
    crc::calc_checksum(data) == checksum
}

// ---- Packet validation ----

/// Validate packet type for config endpoint.
/// Layout: [type(1) + data(8) + checksum(1)]
#[no_mangle]
pub unsafe extern "C" fn rust_validate_packet(packet: *const u8) -> bool {
    if packet.is_null() {
        return false;
    }
    let packet_type = *packet;
    let proxy_inner = *packet.add(1);
    constants::validate_packet_type(packet_type, proxy_inner)
}

// ---- Mouse math ----

#[no_mangle]
pub extern "C" fn rust_move_and_keep_on_screen(position: i32, offset: i32) -> i32 {
    mouse::move_and_keep_on_screen(position, offset)
}

#[no_mangle]
pub extern "C" fn rust_is_screen_switch_needed(position: i32, offset: i32, threshold: u16) -> i32 {
    mouse::is_screen_switch_needed(position, offset, threshold)
}

#[no_mangle]
pub extern "C" fn rust_calculate_mouse_acceleration_factor(
    offset_x: i32,
    offset_y: i32,
    enabled: bool,
) -> f32 {
    mouse::calculate_mouse_acceleration_factor(offset_x, offset_y, enabled)
}

#[no_mangle]
pub extern "C" fn rust_scale_y_coordinate(
    pointer_y: i16,
    from_top: i32,
    from_bottom: i32,
    to_top: i32,
    to_bottom: i32,
) -> i16 {
    mouse::scale_y_coordinate(pointer_y, (from_top, from_bottom), (to_top, to_bottom))
}

// ---- HID parser ----

#[no_mangle]
pub unsafe extern "C" fn rust_get_descriptor_value(report: *const u8, size: i32) -> u32 {
    if report.is_null() {
        return 0;
    }
    let max_len = match size { 1 => 1, 2 => 2, 3 => 4, _ => 0 };
    let data = core::slice::from_raw_parts(report, max_len);
    crate::app::hid_parser::get_descriptor_value(data, size as u8)
}

// ---- HID report ----

/// C-callable: get_report_value(report, len, val) -> int32_t
/// val points to report_val_t — we need offset (u16 at +0) and size (u16 at +4)
#[no_mangle]
pub unsafe extern "C" fn rust_get_report_value(
    report: *const u8,
    len: i32,
    val: *const u8,
) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 {
        return 0;
    }
    let report_slice = core::slice::from_raw_parts(report, len as usize);
    // ReportVal layout: offset(u16) at +0, offset_idx(u16) at +2, size(u16) at +4
    let offset = u16::from_le_bytes([*val, *val.add(1)]);
    let size = u16::from_le_bytes([*val.add(4), *val.add(5)]);
    crate::app::hid_report::get_report_value(report_slice, offset, size)
}

/// C-callable: extract_bit_variable(kbd, raw_report, len, dst) -> int32_t
/// kbd is report_val_t* — we need offset(u16 at +0) and usage_min/max(i32 at +6/+10)
#[no_mangle]
pub unsafe extern "C" fn rust_extract_bit_variable(
    kbd: *const u8,
    raw_report: *const u8,
    len: i32,
    dst: *mut u8,
) -> i32 {
    if kbd.is_null() || raw_report.is_null() || dst.is_null() || len <= 0 {
        return 0;
    }
    let report = core::slice::from_raw_parts(raw_report, len as usize);
    let dst_slice = core::slice::from_raw_parts_mut(dst, len as usize);

    let offset = u16::from_le_bytes([*kbd, *kbd.add(1)]);
    let usage_min = i32::from_le_bytes([*kbd.add(6), *kbd.add(7), *kbd.add(8), *kbd.add(9)]);
    let usage_max = i32::from_le_bytes([*kbd.add(10), *kbd.add(11), *kbd.add(12), *kbd.add(13)]);

    crate::app::hid_report::extract_bit_variable(report, usage_min, usage_max, offset, dst_slice) as i32
}

// ---- Keyboard ----

/// C-callable: key_in_report(key, report) -> bool
/// report points to hid_keyboard_report_t [modifier(1) + reserved(1) + keycode(6)]
#[no_mangle]
pub unsafe extern "C" fn rust_key_in_report(key: u8, report: *const u8) -> bool {
    if report.is_null() {
        return false;
    }
    // keycode starts at offset 2, length 6
    let keycode = core::slice::from_raw_parts(report.add(2), 6);
    keycode.iter().any(|&k| k == key)
}

/// C-callable: check_specific_hotkey(hotkey, report) -> bool
/// hotkey: [modifier(1) + keys(6) + key_count(1) + ...]
/// report: hid_keyboard_report_t [modifier(1) + reserved(1) + keycode(6)]
#[no_mangle]
pub unsafe extern "C" fn rust_check_specific_hotkey(
    hotkey_modifier: u8,
    hotkey_keys: *const u8,
    hotkey_key_count: u8,
    report: *const u8,
) -> bool {
    if report.is_null() {
        return false;
    }
    let report_modifier = *report;

    // All specified modifiers must be present
    if hotkey_modifier != (report_modifier & hotkey_modifier) {
        return false;
    }

    // All specified keys must be in report
    let keycode = core::slice::from_raw_parts(report.add(2), 6);
    if hotkey_keys.is_null() {
        return true; // No keys required, modifier-only hotkey
    }
    for i in 0..hotkey_key_count as usize {
        let key = *hotkey_keys.add(i);
        if !keycode.iter().any(|&k| k == key) {
            return false;
        }
    }
    true
}

// ---- Mouse logic ----

/// C-callable update_mouse_position. Returns switch direction: 0=none, 1=left, 2=right.
/// Updates pointer_x, pointer_y, mouse_buttons in AppState.
#[no_mangle]
pub unsafe extern "C" fn rust_update_mouse_position(
    move_x: i32, move_y: i32, wheel: i32, pan: i32, buttons: i32,
) -> u8 {
    let state = &mut *crate::app::state::rust_get_app_state();
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return 0; }

    let output = &state.config.output[output_idx];
    let values = crate::app::mouse_logic::MouseValues {
        move_x, move_y, wheel, pan, buttons,
    };

    let (new_x, new_y, dir) = crate::app::mouse_logic::update_mouse_position(
        state.pointer_x, state.pointer_y, &values,
        output.speed_x, output.speed_y,
        state.mouse_zoom, state.config.enable_acceleration != 0,
        state.config.jump_threshold,
    );

    state.pointer_x = new_x;
    state.pointer_y = new_y;
    state.mouse_buttons = buttons as i16;

    match dir {
        crate::app::mouse_logic::SwitchDirection::None => 0,
        crate::app::mouse_logic::SwitchDirection::Left => 1,
        crate::app::mouse_logic::SwitchDirection::Right => 2,
    }
}

/// C-callable create_mouse_report — writes 8 bytes to out.
#[no_mangle]
pub unsafe extern "C" fn rust_create_mouse_report(
    wheel: i32, pan: i32, buttons: i32, move_x: i32, move_y: i32, out: *mut u8,
) {
    if out.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();

    let values = crate::app::mouse_logic::MouseValues {
        move_x, move_y, wheel, pan, buttons,
    };

    let report = crate::app::mouse_logic::create_mouse_report(
        state.pointer_x, state.pointer_y, &values,
        state.relative_mouse, state.gaming_mode,
    );

    *out = report.buttons;
    let xb = report.x.to_le_bytes();
    *out.add(1) = xb[0]; *out.add(2) = xb[1];
    let yb = report.y.to_le_bytes();
    *out.add(3) = yb[0]; *out.add(4) = yb[1];
    *out.add(5) = report.wheel as u8;
    *out.add(6) = report.pan as u8;
    *out.add(7) = report.mode;
}

/// C-callable decide_screen_switch — returns ScreenSwitchAction encoded
#[no_mangle]
pub unsafe extern "C" fn rust_decide_screen_switch(direction: u8) -> u8 {
    use crate::app::mouse_logic::*;
    let state = &*crate::app::state::rust_get_app_state();
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return 0; }

    let output = &state.config.output[output_idx];
    let dir = match direction {
        1 => SwitchDirection::Left,
        2 => SwitchDirection::Right,
        _ => return 0,
    };

    let ctx = SwitchContext {
        switch_lock: state.switch_lock,
        gaming_mode: state.gaming_mode,
        mouse_buttons: state.mouse_buttons,
        screen_pos: output.pos,
        screen_index: output.screen_index,
        screen_count: output.screen_count,
    };

    match decide_screen_switch(dir, &ctx) {
        ScreenSwitchAction::Nothing => 0,
        ScreenSwitchAction::SwitchToOtherPc => 1,
        ScreenSwitchAction::SwitchVirtualDesktop { .. } => 2,
    }
}

// ---- Hotkey handlers ----

#[no_mangle]
pub unsafe extern "C" fn rust_output_toggle(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::output_toggle(dev, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_mouse_zoom_toggle() {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::mouse_zoom_toggle(state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_lock_toggle() {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::switch_lock_toggle(state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_gaming_mode_toggle() {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::gaming_mode_toggle(state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_a() {
    crate::app::hotkey_handlers::fw_upgrade_a();
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_b() {
    crate::app::hotkey_handlers::fw_upgrade_b();
}

#[no_mangle]
pub unsafe extern "C" fn rust_wipe_config_hotkey(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::wipe_config(dev, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_enable() {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::screensaver_pong_enable(state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter_enable() {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::screensaver_jitter_enable(state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_disable() {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::hotkey_handlers::screensaver_disable(state);
}

// ---- Screensaver set ----

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_set(value: u8) {
    let state = &mut *crate::app::state::rust_get_app_state();
    if state.is_active_output() {
        let role = state.board_role as usize;
        if role < state.config.output.len() {
            state.config.output[role].screensaver.mode = value;
        }
    } else {
        crate::hal::device::hal_send_value(value, crate::app::constants::PacketType::Screensaver as u8);
    }
}

/// Screen border hotkey — record Y position and sync
#[no_mangle]
pub unsafe extern "C" fn rust_screen_border_hotkey(dev: *mut core::ffi::c_void) {
    use crate::app::handlers::{get_border_position, BorderUpdate};
    let state = &mut *crate::app::state::rust_get_app_state();
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[output_idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[output_idx].border.bottom = v,
        }
        crate::hal::device::hal_save_config(dev);
    }

    // Send border to other board
    let border = &state.config.output[output_idx].border;
    let border_bytes = [
        (border.top & 0xFF) as u8,
        ((border.top >> 8) & 0xFF) as u8,
        ((border.top >> 16) & 0xFF) as u8,
        ((border.top >> 24) & 0xFF) as u8,
        (border.bottom & 0xFF) as u8,
        ((border.bottom >> 8) & 0xFF) as u8,
        ((border.bottom >> 16) & 0xFF) as u8,
        ((border.bottom >> 24) & 0xFF) as u8,
    ];
    crate::hal::device::hal_queue_packet(
        border_bytes.as_ptr(),
        crate::app::constants::PacketType::SyncBorders as u8,
        8, // sizeof(border_size_t)
    );
}

// ---- Screen lock ----

#[no_mangle]
pub unsafe extern "C" fn rust_screenlock_handler(dev: *mut core::ffi::c_void) {
    use crate::app::handlers::screenlock_keys;
    let state = &*crate::app::state::rust_get_app_state();

    for out in 0..2u8 {
        let os = state.config.output[out as usize].os;
        if let Some((modifier, key)) = screenlock_keys(os) {
            let mut report = [0u8; 8]; // hid_keyboard_report_t
            report[0] = modifier;
            report[2] = key;

            if state.board_role == out {
                crate::hal::device::hal_queue_kbd_report(dev, report.as_ptr());
                crate::hal::device::hal_release_all_keys(dev);
            } else {
                crate::hal::device::hal_queue_packet(
                    report.as_ptr(), crate::app::constants::PacketType::KeyboardReport as u8, 8,
                );
                let empty = [0u8; 8];
                crate::hal::device::hal_queue_packet(
                    empty.as_ptr(), crate::app::constants::PacketType::KeyboardReport as u8, 8,
                );
            }
        }
    }
}

/// Handle KBD set report — update LED state, restore if needed
#[no_mangle]
pub unsafe extern "C" fn rust_handle_set_report(dev: *mut core::ffi::c_void, led_value: u8) {
    let state = &mut *crate::app::state::rust_get_app_state();
    let other_role = 1 - state.board_role as usize;
    if other_role < state.keyboard_leds.len() {
        state.keyboard_leds[other_role] = led_value;
    }
    if state.keyboard_connected && !state.is_active_output() {
        crate::hal::device::hal_restore_leds(dev);
    }
}

/// Handle sync borders — update border config and save
#[no_mangle]
pub unsafe extern "C" fn rust_handle_sync_borders(dev: *mut core::ffi::c_void, data: *const u8) {
    use crate::app::handlers::{get_border_position, BorderUpdate};
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[output_idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[output_idx].border.bottom = v,
        }
        // Send to other board
        let border = &state.config.output[output_idx].border;
        let bytes = [
            (border.top & 0xFF) as u8, ((border.top >> 8) & 0xFF) as u8,
            ((border.top >> 16) & 0xFF) as u8, ((border.top >> 24) & 0xFF) as u8,
            (border.bottom & 0xFF) as u8, ((border.bottom >> 8) & 0xFF) as u8,
            ((border.bottom >> 16) & 0xFF) as u8, ((border.bottom >> 24) & 0xFF) as u8,
        ];
        crate::hal::device::hal_queue_packet(
            bytes.as_ptr(),
            crate::app::constants::PacketType::SyncBorders as u8,
            8,
        );
    } else {
        // Copy border data from packet
        let border = &mut state.config.output[output_idx].border;
        border.top = i32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
        border.bottom = i32::from_le_bytes([*data.add(4), *data.add(5), *data.add(6), *data.add(7)]);
    }
    crate::hal::device::hal_save_config(dev);
}

// ---- UART handler helpers ----

/// Handle output select — set output, release keys, restore LEDs
#[no_mangle]
pub unsafe extern "C" fn rust_handle_output_select(dev: *mut core::ffi::c_void, output: u8) {
    let state = &mut *crate::app::state::rust_get_app_state();
    state.active_output = output;
    if state.tud_connected {
        crate::hal::device::hal_release_all_keys(dev);
    }
    crate::hal::device::hal_restore_leds(dev);
}

/// Handle keyboard UART msg — update remote state, combine, queue
#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();

    // Update remote state
    let mut data_arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, data_arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_keyboard_uart(&data_arr, state);

    // Combine and queue
    crate::app::kbd_state::send_key(dev, state);
    let role = state.board_role as usize;
    if role < state.last_activity.len() {
        state.last_activity[role] = crate::hal::device::hal_time_us_64();
    }
}

/// Handle mouse abs UART msg — queue report, update state
#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();

    // Queue the mouse report
    crate::hal::device::hal_queue_mouse_report(dev, data);

    // Update state
    let mut data_arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, data_arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_mouse_uart(&data_arr, state);

    let role = state.board_role as usize;
    if role < state.last_activity.len() {
        state.last_activity[role] = crate::hal::device::hal_time_us_64();
    }
}

// ---- Handlers ----

/// C-callable: _get_border_position(pointer_y, border_top_ptr, border_bottom_ptr)
/// Sets either top or bottom based on pointer_y vs midpoint
#[no_mangle]
pub unsafe extern "C" fn rust_get_border_position(
    pointer_y: i16,
    border_top: *mut i32,
    border_bottom: *mut i32,
) {
    use crate::app::handlers::{get_border_position, BorderUpdate};
    match get_border_position(pointer_y) {
        BorderUpdate::Top(val) => {
            if !border_top.is_null() {
                *border_top = val;
            }
        }
        BorderUpdate::Bottom(val) => {
            if !border_bottom.is_null() {
                *border_bottom = val;
            }
        }
    }
}

// ---- Message handlers ----

/// Process a UART message and apply state changes to AppState.
/// Returns: 0=no HAL action needed, 1=HAL action needed (check action type),
/// specific codes for specific actions.
#[no_mangle]
pub unsafe extern "C" fn rust_handle_simple_msg(
    ptype: u8,
    data: *const u8,
) -> u8 {
    if data.is_null() {
        return 0;
    }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut data_arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, data_arr.as_mut_ptr(), 8);

    let action = crate::app::msg_handlers::handle_simple_msg(ptype, &data_arr, state);
    let needs_hal = crate::app::msg_handlers::apply_action(&action, state);

    if needs_hal { 1 } else { 0 }
}

/// Handle mouse report from UART — update pointer state in AppState
#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart(data: *const u8) {
    if data.is_null() {
        return;
    }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut data_arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, data_arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_mouse_uart(&data_arr, state);
}

/// Handle keyboard report from UART — update remote kbd state in AppState
#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart(data: *const u8) {
    if data.is_null() {
        return;
    }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut data_arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, data_arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_keyboard_uart(&data_arr, state);
}

// ---- Screensaver ----

use crate::app::screensaver::{PongState, JitterState, MouseReport as SSMouseReport};

static mut PONG_STATE: PongState = PongState::new();
static mut JITTER_STATE: JitterState = JitterState::new();

/// Pong screensaver step — returns mouse report via out pointer.
/// out must point to 8 bytes [buttons(1)+x(i16)+y(i16)+wheel(i8)+pan(i8)+mode(1)]
#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong(out: *mut u8) {
    let report = PONG_STATE.step();
    write_mouse_report(out, &report);
}

/// Jitter screensaver step
#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter(out: *mut u8) {
    let report = JITTER_STATE.step();
    write_mouse_report(out, &report);
}

/// Reset pong state (called when screensaver restarts)
#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_reset() {
    PONG_STATE = PongState::new();
}

unsafe fn write_mouse_report(out: *mut u8, report: &SSMouseReport) {
    if out.is_null() { return; }
    *out = report.buttons;
    let x_bytes = report.x.to_le_bytes();
    *out.add(1) = x_bytes[0];
    *out.add(2) = x_bytes[1];
    let y_bytes = report.y.to_le_bytes();
    *out.add(3) = y_bytes[0];
    *out.add(4) = y_bytes[1];
    *out.add(5) = report.wheel as u8;
    *out.add(6) = report.pan as u8;
    *out.add(7) = report.mode;
}

/// Check if screensaver should activate
#[no_mangle]
pub extern "C" fn rust_screensaver_should_activate(
    mode: u8,
    only_if_inactive: u8,
    idle_time_us: u64,
    max_time_us: u64,
    inactivity_us: u64,
    is_active_output: bool,
    tud_ready: bool,
    last_move_us: u32,
    current_time_us: u32,
) -> bool {
    let config = crate::app::screensaver::ScreensaverConfig {
        mode,
        only_if_inactive: only_if_inactive != 0,
        idle_time_us,
        max_time_us,
    };
    crate::app::screensaver::should_activate(
        &config, inactivity_us, is_active_output, tud_ready,
        last_move_us, current_time_us,
    )
}

// ---- Keyboard state ----

#[no_mangle]
pub unsafe extern "C" fn rust_update_kbd_state(report: *const u8, device_idx: u8) {
    if report.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let kbd = &*(report as *const crate::app::structs::HidKeyboardReport);
    crate::app::kbd_state::update_kbd_state(state, kbd, device_idx);
}

#[no_mangle]
pub unsafe extern "C" fn rust_update_remote_kbd_state(report: *const u8) {
    if report.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let kbd = &*(report as *const crate::app::structs::HidKeyboardReport);
    crate::app::kbd_state::update_remote_kbd_state(state, kbd);
}

#[no_mangle]
pub unsafe extern "C" fn rust_combine_kbd_states(out: *mut u8) {
    if out.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();
    let combined = crate::app::kbd_state::combine_kbd_states(state);
    core::ptr::copy_nonoverlapping(&combined as *const _ as *const u8, out, 8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_key(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::kbd_state::send_key(dev, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_release_all_keys_state(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::app::kbd_state::release_all_keys(dev, state);
}

// ---- Mouse output routing ----

/// Route mouse report to local queue or UART based on active output
#[no_mangle]
pub unsafe extern "C" fn rust_output_mouse_report(dev: *mut core::ffi::c_void, report: *const u8) {
    if report.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();

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

// ---- Consumer/System control routing ----

#[no_mangle]
pub unsafe extern "C" fn rust_send_consumer_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();
    if state.is_active_output() {
        // Queue locally — need protocol.c queue_cc_packet which is C-only (queue_t)
        // For now, delegate back to C
    } else {
        crate::hal::device::hal_queue_packet(
            raw_report, crate::app::constants::PacketType::ConsumerControl as u8, 4,
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_system_control(dev: *mut core::ffi::c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let state = &*crate::app::state::rust_get_app_state();
    if state.is_active_output() {
        // Queue locally — delegate to C
    } else {
        crate::hal::device::hal_queue_packet(
            raw_report, crate::app::constants::PacketType::SystemControl as u8, 1,
        );
    }
}

// ---- Packet utilities ----

#[no_mangle]
pub unsafe extern "C" fn rust_write_raw_packet(dst: *mut u8, packet_ptr: *const u8) {
    if dst.is_null() || packet_ptr.is_null() {
        return;
    }
    let pkt = packet::UartPacket {
        ptype: *packet_ptr,
        data: {
            let mut d = [0u8; constants::PACKET_DATA_LENGTH];
            core::ptr::copy_nonoverlapping(packet_ptr.add(1), d.as_mut_ptr(), constants::PACKET_DATA_LENGTH);
            d
        },
        checksum: *packet_ptr.add(9),
    };
    let raw = packet::write_raw_packet(&pkt);
    core::ptr::copy_nonoverlapping(raw.as_ptr(), dst, constants::RAW_PACKET_LENGTH);
}

#[no_mangle]
pub extern "C" fn rust_get_ptr_delta(current: u32, saved: u32, buffer_size: u32) -> u32 {
    packet::get_ptr_delta(current, saved, buffer_size)
}
