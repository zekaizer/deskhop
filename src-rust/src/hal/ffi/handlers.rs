use crate::app::handlers::{get_border_position, BorderUpdate};

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
pub unsafe extern "C" fn rust_fw_upgrade_a() { crate::app::hotkey_handlers::fw_upgrade_a(); }

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_b() { crate::app::hotkey_handlers::fw_upgrade_b(); }

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

#[no_mangle]
pub unsafe extern "C" fn rust_screen_border_hotkey(dev: *mut core::ffi::c_void) {
    let state = &mut *crate::app::state::rust_get_app_state();
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        crate::hal::device::hal_save_config(dev);
    }
    let b = &state.config.output[idx].border;
    let bytes = [
        (b.top & 0xFF) as u8, ((b.top >> 8) & 0xFF) as u8,
        ((b.top >> 16) & 0xFF) as u8, ((b.top >> 24) & 0xFF) as u8,
        (b.bottom & 0xFF) as u8, ((b.bottom >> 8) & 0xFF) as u8,
        ((b.bottom >> 16) & 0xFF) as u8, ((b.bottom >> 24) & 0xFF) as u8,
    ];
    crate::hal::device::hal_queue_packet(
        bytes.as_ptr(), crate::app::constants::PacketType::SyncBorders as u8, 8,
    );
}

#[no_mangle]
pub unsafe extern "C" fn rust_screenlock_handler(dev: *mut core::ffi::c_void) {
    let state = &*crate::app::state::rust_get_app_state();
    for out in 0..2u8 {
        if let Some((modifier, key)) = crate::app::handlers::screenlock_keys(state.config.output[out as usize].os) {
            let mut report = [0u8; 8];
            report[0] = modifier; report[2] = key;
            if state.board_role == out {
                crate::hal::device::hal_queue_kbd_report(dev, report.as_ptr());
                crate::hal::device::hal_release_all_keys(dev);
            } else {
                crate::hal::device::hal_queue_packet(report.as_ptr(), crate::app::constants::PacketType::KeyboardReport as u8, 8);
                crate::hal::device::hal_queue_packet([0u8; 8].as_ptr(), crate::app::constants::PacketType::KeyboardReport as u8, 8);
            }
        }
    }
}

// ---- UART message handlers ----

#[no_mangle]
pub unsafe extern "C" fn rust_handle_simple_msg(ptype: u8, data: *const u8) -> u8 {
    if data.is_null() { return 0; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    let action = crate::app::msg_handlers::handle_simple_msg(ptype, &arr, state);
    if crate::app::msg_handlers::apply_action(&action, state) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart(data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_mouse_uart(&arr, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart(data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_keyboard_uart(&arr, state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_output_select(dev: *mut core::ffi::c_void, output: u8) {
    let state = &mut *crate::app::state::rust_get_app_state();
    state.active_output = output;
    if state.tud_connected { crate::hal::device::hal_release_all_keys(dev); }
    crate::hal::device::hal_restore_leds(dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_keyboard_uart(&arr, state);
    crate::app::kbd_state::send_key(dev, state);
    let role = state.board_role as usize;
    if role < state.last_activity.len() {
        state.last_activity[role] = crate::hal::device::hal_time_us_64();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    crate::hal::device::hal_queue_mouse_report(dev, data);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_mouse_uart(&arr, state);
    let role = state.board_role as usize;
    if role < state.last_activity.len() {
        state.last_activity[role] = crate::hal::device::hal_time_us_64();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_set_report(dev: *mut core::ffi::c_void, led_value: u8) {
    let state = &mut *crate::app::state::rust_get_app_state();
    let other = 1 - state.board_role as usize;
    if other < state.keyboard_leds.len() { state.keyboard_leds[other] = led_value; }
    if state.keyboard_connected && !state.is_active_output() {
        crate::hal::device::hal_restore_leds(dev);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_sync_borders(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = &mut *crate::app::state::rust_get_app_state();
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        let b = &state.config.output[idx].border;
        let bytes = [
            (b.top & 0xFF) as u8, ((b.top >> 8) & 0xFF) as u8,
            ((b.top >> 16) & 0xFF) as u8, ((b.top >> 24) & 0xFF) as u8,
            (b.bottom & 0xFF) as u8, ((b.bottom >> 8) & 0xFF) as u8,
            ((b.bottom >> 16) & 0xFF) as u8, ((b.bottom >> 24) & 0xFF) as u8,
        ];
        crate::hal::device::hal_queue_packet(
            bytes.as_ptr(), crate::app::constants::PacketType::SyncBorders as u8, 8,
        );
    } else {
        let border = &mut state.config.output[idx].border;
        border.top = i32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
        border.bottom = i32::from_le_bytes([*data.add(4), *data.add(5), *data.add(6), *data.add(7)]);
    }
    crate::hal::device::hal_save_config(dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_border_position(pointer_y: i16, top: *mut i32, bottom: *mut i32) {
    match get_border_position(pointer_y) {
        BorderUpdate::Top(v) => { if !top.is_null() { *top = v; } }
        BorderUpdate::Bottom(v) => { if !bottom.is_null() { *bottom = v; } }
    }
}
