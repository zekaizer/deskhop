use crate::app::handlers::{get_border_position, BorderUpdate};

// ---- Hotkey handlers (app logic + HAL dispatch) ----

use crate::app::constants::PacketType;

#[no_mangle]
pub unsafe extern "C" fn rust_output_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if crate::app::hotkey_handlers::output_toggle(state) {
        crate::hal::device::hal_set_active_output(dev, state.active_output);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_mouse_zoom_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let val = crate::app::hotkey_handlers::mouse_zoom_toggle(state);
    crate::hal::device::hal_send_value(val as u8, PacketType::MouseZoom as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_lock_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let val = crate::app::hotkey_handlers::switch_lock_toggle(state);
    crate::hal::device::hal_send_value(val as u8, PacketType::SwitchLock as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_gaming_mode_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let val = crate::app::hotkey_handlers::gaming_mode_toggle(state);
    crate::hal::device::hal_send_value(val as u8, PacketType::GamingMode as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_a() { crate::hal::device::hal_reset_usb_boot(); }

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_b() {
    crate::hal::device::hal_send_value(1, PacketType::FirmwareUpgrade as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_wipe_config_hotkey(dev: *mut core::ffi::c_void) {
    crate::hal::device::hal_wipe_config();
    crate::hal::device::hal_load_config(dev);
    crate::hal::device::hal_send_value(1, PacketType::WipeConfig as u8);
}

fn screensaver_dispatch(state: &mut crate::app::structs::Device, mode: u8) {
    use crate::app::hotkey_handlers::ScreensaverAction;
    match crate::app::hotkey_handlers::screensaver_set(state, mode) {
        ScreensaverAction::UpdatedLocally => {}
        ScreensaverAction::SendToRemote(m) => {
            unsafe { crate::hal::device::hal_send_value(m, PacketType::Screensaver as u8); }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_enable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if let Some(mode) = crate::app::hotkey_handlers::screensaver_pong_mode(state) {
        screensaver_dispatch(state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter_enable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if let Some(mode) = crate::app::hotkey_handlers::screensaver_jitter_mode(state) {
        screensaver_dispatch(state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_disable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    screensaver_dispatch(state, 0);
}

#[no_mangle]
pub unsafe extern "C" fn rust_config_enable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let need_scratch = crate::app::hotkey_handlers::config_enable(state);
    if need_scratch {
        crate::hal::device::hal_set_config_mode_scratch();
    }
    crate::hal::device::hal_release_all_keys(dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_screen_border_hotkey(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
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
    let state = crate::app::structs::device_from_ptr(dev);
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
pub unsafe extern "C" fn rust_handle_simple_msg(ptype: u8, data: *const u8, dev: *mut core::ffi::c_void) -> u8 {
    if data.is_null() { return 0; }
    let state = crate::app::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    let action = crate::app::msg_handlers::handle_simple_msg(ptype, &arr, state);
    if crate::app::msg_handlers::apply_action(&action, state) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_output_select(dev: *mut core::ffi::c_void, output: u8) {
    let state = crate::app::structs::device_from_ptr(dev);
    state.active_output = output;
    if state.tud_connected { crate::hal::device::hal_release_all_keys(dev); }
    crate::hal::device::hal_restore_leds(dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = crate::app::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::app::msg_handlers::handle_keyboard_uart(&arr, state);
    // Route combined keyboard report
    let combined = crate::app::kbd_state::combine_kbd_states(state);
    if state.is_active_output() {
        crate::hal::device::hal_queue_kbd_report(dev, &combined as *const _ as *const u8);
    } else {
        crate::hal::device::hal_queue_packet(
            &combined as *const _ as *const u8,
            crate::app::constants::PacketType::KeyboardReport as u8,
            crate::app::structs::KBD_REPORT_LENGTH as i32,
        );
    }
    let role = state.board_role as usize;
    if role < state.last_activity.len() {
        state.last_activity[role] = crate::hal::device::hal_time_us_64();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = crate::app::structs::device_from_ptr(dev);
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
    let state = crate::app::structs::device_from_ptr(dev);
    let other = 1 - state.board_role as usize;
    if other < state.keyboard_leds.len() { state.keyboard_leds[other] = led_value; }
    if state.keyboard_connected && !state.is_active_output() {
        crate::hal::device::hal_restore_leds(dev);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_sync_borders(dev: *mut core::ffi::c_void, data: *const u8) {
    if data.is_null() { return; }
    let state = crate::app::structs::device_from_ptr(dev);
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

/// Handle FW response byte — update checksum, page buffer, advance address.
#[no_mangle]
pub unsafe extern "C" fn rust_handle_response_byte(data: *const u8, dev: *mut core::ffi::c_void) {
    if data.is_null() { return; }
    let state = crate::app::structs::device_from_ptr(dev);

    // data is uart_packet_t.data (8 bytes): data32[0]=address, data[0]=offset, data32[1]=fw_data
    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);

    if address != state.fw.address {
        state.fw.upgrade_in_progress = false;
        state.fw.address = 0;
        return;
    }

    if (address & 0xfff) == 0x000 {
        crate::hal::device::hal_toggle_led();
    }

    // STAGING_IMAGE_SIZE = 1024 * 256 = 262144, FLASH_SECTOR_SIZE = 4096
    const STAGING_IMAGE_SIZE: u32 = 262144;
    const FLASH_SECTOR_SIZE: u32 = 4096;

    if address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE {
        for i in 0..4 {
            state.fw.checksum = crate::app::crc::crc32_iter(state.fw.checksum, *data.add(4 + i));
        }
    }

    let offset = *data as usize;
    if offset + 4 <= state.page_buffer.len() {
        let fw_data = core::slice::from_raw_parts(data.add(4), 4);
        state.page_buffer[offset..offset + 4].copy_from_slice(fw_data);
    }

    state.fw.address += 4;
    state.fw.byte_done = true;
}

/// Rust implementation of handle_request_byte_msg
#[no_mangle]
pub unsafe extern "C" fn rust_handle_request_byte(data: *mut u8) {
    if data.is_null() { return; }
    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
    const STAGING_IMAGE_SIZE: u32 = 262144;
    if address > STAGING_IMAGE_SIZE { return; }
    let fw_data = crate::hal::device::hal_read_fw_running_u32(address);
    let bytes = fw_data.to_le_bytes();
    *data.add(4) = bytes[0]; *data.add(5) = bytes[1];
    *data.add(6) = bytes[2]; *data.add(7) = bytes[3];
    crate::hal::device::hal_queue_packet(
        data, crate::app::constants::PacketType::ResponseByte as u8, 8,
    );
}

/// Rust implementation of handle_api_msgs
#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_msgs(ptype: u8, data: *const u8, dev: *mut core::ffi::c_void) {
    if data.is_null() { return; }
    let api_idx = *data;
    let mut offset: u32 = 0;
    let mut len: u32 = 0;
    let mut readonly = false;

    if crate::hal::device::hal_get_field_map(api_idx, &mut offset, &mut len, &mut readonly) != 0 {
        return;
    }

    const SET_VAL: u8 = 21; // PacketType::SetVal
    const GET_VAL: u8 = 20; // PacketType::GetVal

    if ptype == SET_VAL {
        if readonly { return; }
        crate::hal::device::hal_api_write_field(offset, len, data.add(1));
    } else if ptype == GET_VAL {
        let mut response = [0u8; 10]; // uart_packet_t
        response[0] = GET_VAL;
        response[1] = api_idx;
        crate::hal::device::hal_api_read_field(offset, len, response[2..].as_mut_ptr());
        // Queue config packet via HAL
        extern "C" { fn hal_queue_cfg_packet(dev: *mut core::ffi::c_void, packet: *const u8); }
        hal_queue_cfg_packet(dev, response.as_ptr());
    }

    // Reset config timer
    let state = crate::app::structs::device_from_ptr(dev);
    state.config_mode_timer = crate::hal::device::hal_time_us_64() + 300_000_000; // CONFIG_MODE_TIMEOUT
}

/// Rust implementation of handle_api_read_all_msg
#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_read_all_msgs(dev: *mut core::ffi::c_void) {
    let count = crate::hal::device::hal_get_field_map_length();
    for i in 0..count {
        let idx = crate::hal::device::hal_get_field_map_idx(i);
        let data = [idx, 0, 0, 0, 0, 0, 0, 0];
        rust_handle_api_msgs(20, data.as_ptr(), dev); // GET_VAL
    }
}

