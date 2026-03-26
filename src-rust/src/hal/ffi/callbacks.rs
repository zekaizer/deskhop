// USB/UART callback FFI — all functions called from usb.c/uart.c callbacks.

use core::ffi::c_void;
use crate::domain::constants::{PacketType, MAX_SCREEN_COORD, MIN_SCREEN_COORD, ABSOLUTE, RELATIVE};
use crate::domain::actions::{get_border_position, BorderUpdate};
use crate::domain::keyboard::HotkeyAction;
use crate::domain::mouse;
use crate::domain::mouse_logic;
use crate::domain::hid_parser::{self, ReportVal, CONSTANT, VARIABLE, ARRAY};
use crate::domain::hid_classify::{classify_report_val, is_padding, ExtractedType};
use crate::domain::structs::{self, iface_from_ptr, get_keyboard, HidInterface,
                              KBD_REPORT_LENGTH, MAX_REPORTS, MAX_KEYBOARDS, MAX_KEYS,
                              MAX_CC_BUTTONS};
use crate::hal::device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

// ============================================================
// Shared helpers
// ============================================================

fn border_to_bytes(top: i32, bottom: i32) -> [u8; 8] {
    let t = top.to_le_bytes();
    let b = bottom.to_le_bytes();
    [t[0], t[1], t[2], t[3], b[0], b[1], b[2], b[3]]
}

unsafe fn hal_from(dev: *mut c_void) -> crate::hal::pico::PicoHal {
    crate::hal::pico::PicoHal::new(dev)
}

// ============================================================
// Keyboard report processing (from kbd_process.rs)
// ============================================================

/// Full keyboard report processing pipeline.
/// Called directly from TinyUSB callback (process_report_f signature).

#[export_name = "process_keyboard_report"]
pub unsafe extern "C" fn rust_process_keyboard_report(
    raw_report: *mut u8,
    length: i32,
    itf: u8,
    iface: *mut c_void,  // hid_interface_t*
) {
    if raw_report.is_null() || iface.is_null() { return; }

    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);

    if length < KBD_REPORT_LENGTH as i32 {
        return;
    }

    if state.reboot_requested {
        return;
    }

    // Extract keyboard data -- call Rust extract directly (no C roundtrip)
    let mut new_report = [0u8; 8];
    rust_extract_kbd_data(raw_report, length, itf, iface, new_report.as_mut_ptr());

    // Update keyboard state for this device
    let kbd = &*(new_report.as_ptr() as *const crate::domain::structs::HidKeyboardReport);
    crate::domain::kbd_state::update_kbd_state(state, kbd, itf);

    // Check hotkeys -- fully in Rust, no C roundtrip
    let report_for_hotkey = crate::domain::keyboard::KeyboardReport {
        modifier: new_report[0],
        reserved: new_report[1],
        keycode: [new_report[2], new_report[3], new_report[4],
                  new_report[5], new_report[6], new_report[7]],
    };
    if let Some(m) = crate::domain::keyboard::check_all_hotkeys(&report_for_hotkey) {
        // Execute the hotkey action
        execute_hotkey_action(dev, m.action);
        if m.acknowledge {
            hal.blink();
        }
        if !m.pass_to_os {
            return;
        }
    }

    // Send key via combined report -- route based on active output
    let combined = crate::domain::kbd_state::combine_kbd_states(state);
    hal.route_kbd(state, &combined as *const _ as *const u8);
}

/// Rust implementation of process_consumer_report
#[export_name = "process_consumer_report"]
pub unsafe extern "C" fn rust_process_consumer_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    iface: *mut c_void,
) {
    if raw_report.is_null() || iface.is_null() || length < 2 { return; }
    let ifc = iface_from_ptr(iface);

    let mut new_report = [0u8; 4]; // CONSUMER_CONTROL_LENGTH

    if ifc.consumer.is_variable {
        let report_id = *raw_report;
        let kbd = get_keyboard(ifc, report_id);
        let max_buttons = 16i32; // MAX_CC_BUTTONS
        let max_bits = 8 * (length - 1);
        let limit = if max_buttons < max_bits { max_buttons } else { max_bits };

        for i in 0..limit {
            let bit_idx = i % 8;
            let byte_idx = i >> 3;
            if (*raw_report.add((byte_idx + 1) as usize) >> bit_idx) & 1 != 0 {
                let idx = i as usize;
                if idx < kbd.cc_array.len() {
                    let cc_val = kbd.cc_array[idx];
                    new_report[0] = (cc_val & 0xFF) as u8;
                    new_report[1] = ((cc_val >> 8) & 0xFF) as u8;
                }
            }
        }
    } else {
        for i in 0..core::cmp::min((length - 1) as usize, 4) {
            new_report[i] = *raw_report.add(i + 1);
        }
    }

    // Route: local queue if active output, UART if not
    let dev = crate::domain::structs::get_global_device() as *mut _ as *mut c_void;
    rust_send_consumer_control(dev, new_report.as_ptr());
}

#[export_name = "process_system_report"]
pub unsafe extern "C" fn rust_process_system_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    _iface: *mut c_void,
) {
    if raw_report.is_null() || length < 2 { return; }

    let report = [*raw_report.add(1), 0];

    let dev = crate::domain::structs::get_global_device() as *mut _ as *mut c_void;
    rust_send_system_control(dev, report.as_ptr());
}

// ============================================================
// Mouse report processing (from mouse_process.rs)
// ============================================================

/// Full mouse report processing pipeline.
/// Called directly from TinyUSB callback (process_report_f signature).

#[export_name = "process_mouse_report"]
pub unsafe extern "C" fn rust_process_mouse_report(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,  // hid_interface_t*
) {
    if raw_report.is_null() || iface_ptr.is_null() { return; }

    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);
    let iface = iface_from_ptr(iface_ptr);

    let mut values = [0i32; 5];
    const HID_PROTOCOL_BOOT: u8 = 0;

    if iface.protocol == HID_PROTOCOL_BOOT {
        values[0] = *raw_report.add(1) as i8 as i32; // x
        values[1] = *raw_report.add(2) as i8 as i32; // y
        values[2] = *raw_report.add(3) as i8 as i32; // wheel
        values[3] = 0; // pan (not in boot)
        values[4] = *raw_report as i32; // buttons
    } else {
        let uses_id = iface.uses_report_id;
        let report_slice = core::slice::from_raw_parts(raw_report, len as usize);

        fn extract_val(report: &[u8], uses_id: bool, rv: &ReportVal) -> Option<i32> {
            let rid = { rv.report_id };
            let src = if uses_id {
                if report[0] != rid { return None; }
                &report[1..]
            } else { report };
            let offset = { rv.offset };
            let size = { rv.size };
            Some(crate::domain::hid_report::get_report_value(src, offset, size))
        }

        if let Some(v) = extract_val(report_slice, uses_id, &iface.mouse.move_x) { values[0] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, &iface.mouse.move_y) { values[1] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, &iface.mouse.wheel) { values[2] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, &iface.mouse.pan) { values[3] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, &iface.mouse.buttons) {
            values[4] = v;
        } else {
            values[4] = state.mouse_buttons as i32;
        }
    }

    let mouse_vals = mouse_logic::MouseValues {
        move_x: values[0],
        move_y: values[1],
        wheel: values[2],
        pan: values[3],
        buttons: values[4],
    };

    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }
    let output = &state.config.output[output_idx];

    let (new_x, new_y, dir) = mouse_logic::update_mouse_position(
        state.pointer_x, state.pointer_y, &mouse_vals,
        output.speed_x, output.speed_y,
        state.mouse_zoom, state.config.enable_acceleration != 0,
        state.config.jump_threshold,
    );
    state.pointer_x = new_x;
    state.pointer_y = new_y;
    state.mouse_buttons = mouse_vals.buttons as i16;

    let report = mouse_logic::create_mouse_report(
        state.pointer_x, state.pointer_y, &mouse_vals,
        state.relative_mouse, state.gaming_mode,
    );

    let report_bytes = [
        report.buttons,
        report.x.to_le_bytes()[0], report.x.to_le_bytes()[1],
        report.y.to_le_bytes()[0], report.y.to_le_bytes()[1],
        report.wheel as u8,
        report.pan as u8,
        report.mode,
    ];

    hal.route_mouse(state, report_bytes.as_ptr());

    // Screen switch handling
    let c_dir = match dir {
        mouse_logic::SwitchDirection::None => return,
        mouse_logic::SwitchDirection::Left => 1i32,
        mouse_logic::SwitchDirection::Right => 2i32,
    };
    rust_do_screen_switch(dev, c_dir);
}

// ============================================================
// UART message dispatch (from msg_dispatch.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_handle_simple_msg(ptype: u8, data: *const u8, dev: *mut c_void) -> u8 {
    if data.is_null() { return 0; }
    let state = crate::domain::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    let action = crate::domain::msg_handlers::handle_simple_msg(ptype, &arr, state);
    if crate::domain::msg_handlers::apply_action(&action, state) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_output_select(dev: *mut c_void, output: u8) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    state.active_output = output;
    if state.tud_connected { crate::service::backend::host_link::release_all_keys(state, &hal); }
    hal.sync_leds();
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart_full(dev: *mut c_void, data: *const u8) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::domain::msg_handlers::handle_keyboard_uart(&arr, state);
    let combined = crate::domain::kbd_state::combine_kbd_states(state);
    hal.route_kbd(state, &combined as *const _ as *const u8);
    // UART keyboard data: always update activity (even when routed to peer)
    if !state.is_active_output() {
        hal.touch_activity(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_mouse_uart_full(dev: *mut c_void, data: *const u8) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    hal.push_mouse_report(data);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    crate::domain::msg_handlers::handle_mouse_uart(&arr, state);
    hal.touch_activity(state);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_set_report(dev: *mut c_void, led_value: u8) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let other = 1usize.wrapping_sub(state.board_role as usize);
    if other < state.keyboard_leds.len() { state.keyboard_leds[other] = led_value; }
    if state.keyboard_connected && !state.is_active_output() {
        hal.sync_leds();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_sync_borders(dev: *mut c_void, data: *const u8) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        let b = &state.config.output[idx].border;
        let bytes = border_to_bytes(b.top, b.bottom);
        hal.send_packet(bytes.as_ptr(), PacketType::SyncBorders as u8, 8);
    } else {
        let border = &mut state.config.output[idx].border;
        border.top = i32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
        border.bottom = i32::from_le_bytes([*data.add(4), *data.add(5), *data.add(6), *data.add(7)]);
    }
    hal.save();
}

// ============================================================
// Hotkey dispatch (from hotkey_dispatch.rs)
// ============================================================

/// Execute a hotkey action by its enum variant. Called from kbd_process.

pub unsafe fn execute_hotkey_action(dev: *mut c_void, action: HotkeyAction) {
    match action {
        HotkeyAction::OutputToggle => rust_output_toggle(dev),
        HotkeyAction::MouseZoomToggle => rust_mouse_zoom_toggle(dev),
        HotkeyAction::SwitchLockToggle => rust_switch_lock_toggle(dev),
        HotkeyAction::ScreenLock => rust_screenlock_handler(dev),
        HotkeyAction::GamingModeToggle => rust_gaming_mode_toggle(dev),
        HotkeyAction::ScreensaverPong => rust_screensaver_pong_enable(dev),
        HotkeyAction::ScreensaverJitter => rust_screensaver_jitter_enable(dev),
        HotkeyAction::ScreensaverDisable => rust_screensaver_disable(dev),
        HotkeyAction::WipeConfig => rust_wipe_config_hotkey(dev),
        HotkeyAction::ScreenBorder => rust_screen_border_hotkey(dev),
        HotkeyAction::ConfigEnable => rust_config_enable(dev),
        HotkeyAction::FwUpgradeA => rust_fw_upgrade_a(dev),
        HotkeyAction::FwUpgradeB => rust_fw_upgrade_b(dev),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_output_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    if crate::domain::hotkey_handlers::output_toggle(state) {
        hal.switch_output(state.active_output);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_mouse_zoom_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let val = crate::domain::hotkey_handlers::mouse_zoom_toggle(state);
    hal.send_value(val as u8, PacketType::MouseZoom as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_lock_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let val = crate::domain::hotkey_handlers::switch_lock_toggle(state);
    hal.send_value(val as u8, PacketType::SwitchLock as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_gaming_mode_toggle(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let val = crate::domain::hotkey_handlers::gaming_mode_toggle(state);
    hal.send_value(val as u8, PacketType::GamingMode as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_a(dev: *mut c_void) {
    let hal = hal_from(dev);
    hal.reboot_to_bootloader();
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_b(dev: *mut c_void) {
    let hal = hal_from(dev);
    hal.send_value(1, PacketType::FirmwareUpgrade as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_wipe_config_hotkey(dev: *mut c_void) {
    let hal = hal_from(dev);
    hal.wipe();
    hal.load();
    hal.send_value(1, PacketType::WipeConfig as u8);
}

fn screensaver_dispatch(hal: &impl PeerLink, state: &mut crate::domain::structs::Device, mode: u8) {
    use crate::domain::hotkey_handlers::ScreensaverAction;
    match crate::domain::hotkey_handlers::screensaver_set(state, mode) {
        ScreensaverAction::UpdatedLocally => {}
        ScreensaverAction::SendToRemote(m) => {
            hal.send_value(m, PacketType::Screensaver as u8);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_enable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    if let Some(mode) = crate::domain::hotkey_handlers::screensaver_pong_mode(state) {
        screensaver_dispatch(&hal, state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter_enable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    if let Some(mode) = crate::domain::hotkey_handlers::screensaver_jitter_mode(state) {
        screensaver_dispatch(&hal, state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_disable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    screensaver_dispatch(&hal, state, 0);
}

#[no_mangle]
pub unsafe extern "C" fn rust_config_enable(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    // Order matters: set scratch FIRST, release keys, THEN request reboot.
    if !state.config_mode_active {
        hal.set_boot_flag();
    }
    crate::service::backend::host_link::release_all_keys(state, &hal);
    state.reboot_requested = true;
}

#[no_mangle]
pub unsafe extern "C" fn rust_screen_border_hotkey(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        hal.save();
    }
    let b = &state.config.output[idx].border;
    let bytes = border_to_bytes(b.top, b.bottom);
    hal.send_packet(bytes.as_ptr(), PacketType::SyncBorders as u8, 8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_screenlock_handler(dev: *mut c_void) {
    let hal = hal_from(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    for out in 0..2u8 {
        if let Some((modifier, key)) = crate::domain::actions::screenlock_keys(state.config.output[out as usize].os) {
            let mut report = [0u8; 8];
            report[0] = modifier; report[2] = key;
            if state.board_role == out {
                hal.push_kbd_report(report.as_ptr());
                crate::service::backend::host_link::release_all_keys(state, &hal);
            } else {
                hal.send_packet(report.as_ptr(), PacketType::KeyboardReport as u8, 8);
                hal.send_packet([0u8; 8].as_ptr(), PacketType::KeyboardReport as u8, 8);
            }
        }
    }
}

// ============================================================
// Screen switch (from screen_switch.rs)
// ============================================================

const MACOS_SWITCH_MOVE_X: i16 = 10;
const MACOS_SWITCH_MOVE_COUNT: usize = 5;

/// Helper to output a mouse report via the routing logic

unsafe fn output_report(hal: &(impl ReportQueue + PeerLink), state: &crate::domain::structs::Device, report: &[u8; 8]) {
    if state.is_active_output() {
        hal.push_mouse_report(report.as_ptr());
    } else {
        hal.send_packet(
            report.as_ptr(), PacketType::MouseReport as u8, 8,
        );
    }
}

/// Replace C's switch_to_another_pc

#[no_mangle]
pub unsafe extern "C" fn rust_switch_to_another_pc(
    dev: *mut c_void,
    output_number: u32,
    output_to: i32,
    direction: i32,  // LEFT=1, RIGHT=2
) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    let mouse_park_pos = state.config.output[output_idx].mouse_park_pos;
    let mouse_y = match mouse_park_pos {
        0 => MIN_SCREEN_COORD,
        1 => MAX_SCREEN_COORD,
        _ => state.pointer_y,
    };

    let hidden = [
        0u8,
        MAX_SCREEN_COORD.to_le_bytes()[0], MAX_SCREEN_COORD.to_le_bytes()[1],
        mouse_y.to_le_bytes()[0], mouse_y.to_le_bytes()[1],
        0, 0, 0,
    ];

    output_report(&hal, state, &hidden);
    hal.switch_output(output_to as u8);

    state.pointer_x = if direction == 1 { MAX_SCREEN_COORD } else { MIN_SCREEN_COORD };

    let other = 1 - output_number;
    if (output_number as usize) < state.config.output.len()
        && (other as usize) < state.config.output.len()
    {
        let from = &state.config.output[output_number as usize];
        let to = &state.config.output[other as usize];
        state.pointer_y = mouse::scale_y_coordinate(
            state.pointer_y,
            (from.border.top, from.border.bottom),
            (to.border.top, to.border.bottom),
        );
    }
}

/// Replace C's switch_virtual_desktop_macos

#[no_mangle]
pub unsafe extern "C" fn rust_switch_virtual_desktop_macos(dev: *mut c_void, direction: i32) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let left = direction == 1;

    let edge_x = if left { MIN_SCREEN_COORD } else { MAX_SCREEN_COORD };
    let edge = [
        state.mouse_buttons as u8,
        edge_x.to_le_bytes()[0], edge_x.to_le_bytes()[1],
        (MAX_SCREEN_COORD / 2).to_le_bytes()[0], (MAX_SCREEN_COORD / 2).to_le_bytes()[1],
        0, 0, ABSOLUTE,
    ];
    output_report(&hal, state, &edge);

    let move_x: i16 = if left { -MACOS_SWITCH_MOVE_X } else { MACOS_SWITCH_MOVE_X };
    let rel = [
        state.mouse_buttons as u8,
        move_x.to_le_bytes()[0], move_x.to_le_bytes()[1],
        0, 0, 0, 0, RELATIVE,
    ];
    for _ in 0..MACOS_SWITCH_MOVE_COUNT {
        output_report(&hal, state, &rel);
    }
}

/// Replace C's switch_virtual_desktop

#[no_mangle]
pub unsafe extern "C" fn rust_switch_virtual_desktop(
    dev: *mut c_void, os: u8, new_index: i32, direction: i32,
) {
    let state = crate::domain::structs::device_from_ptr(dev);
    use crate::domain::constants::{OS_MACOS, OS_WINDOWS};

    match os {
        OS_MACOS => rust_switch_virtual_desktop_macos(dev, direction),
        OS_WINDOWS => { state.relative_mouse = new_index > 1; }
        _ => {}
    }

    state.pointer_x = if direction == 2 { MIN_SCREEN_COORD } else { MAX_SCREEN_COORD };
}

/// Replace C's do_screen_switch

#[no_mangle]
pub unsafe extern "C" fn rust_do_screen_switch(dev: *mut c_void, direction: i32) {
    let state = crate::domain::structs::device_from_ptr(dev);
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    let output = &state.config.output[output_idx];
    let dir = match direction {
        1 => mouse_logic::SwitchDirection::Left,
        2 => mouse_logic::SwitchDirection::Right,
        _ => return,
    };

    let ctx = mouse_logic::SwitchContext {
        switch_lock: state.switch_lock,
        gaming_mode: state.gaming_mode,
        mouse_buttons: state.mouse_buttons,
        screen_pos: output.pos,
        screen_index: output.screen_index,
        screen_count: output.screen_count,
    };

    match mouse_logic::decide_screen_switch(dir, &ctx) {
        mouse_logic::ScreenSwitchAction::Nothing => {}
        mouse_logic::ScreenSwitchAction::SwitchToOtherPc => {
            let output_number = output.number;
            rust_switch_to_another_pc(dev, output_number, (1 - state.active_output) as i32, direction);
        }
        mouse_logic::ScreenSwitchAction::SwitchVirtualDesktop { new_index } => {
            let os = output.os;
            rust_switch_virtual_desktop(dev, os, new_index as i32, direction);
            state.config.output[output_idx].screen_index = new_index;
        }
    }
}

// ============================================================
// Consumer/system control routing (from state.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_send_consumer_control(dev: *mut c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    hal.route_consumer(state, raw_report);
}

#[no_mangle]
pub unsafe extern "C" fn rust_send_system_control(dev: *mut c_void, raw_report: *const u8) {
    if raw_report.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    hal.route_system(state, raw_report);
}

// ============================================================
// Firmware upgrade + API message handlers (from fw_handlers.rs)
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_handle_response_byte(data: *const u8, dev: *mut c_void) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);

    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);

    if address != state.fw.address {
        state.fw.upgrade_in_progress = false;
        state.fw.address = 0;
        return;
    }

    if (address & 0xfff) == 0x000 {
        hal.toggle();
    }

    const STAGING_IMAGE_SIZE: u32 = 262144;
    const FLASH_SECTOR_SIZE: u32 = 4096;

    if address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE {
        for i in 0..4 {
            state.fw.checksum = crate::domain::crc::crc32_iter(state.fw.checksum, *data.add(4 + i));
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

#[no_mangle]
pub unsafe extern "C" fn rust_handle_request_byte(data: *mut u8) {
    if data.is_null() { return; }
    // No dev pointer -- use global device for PicoHal
    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);

    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
    const STAGING_IMAGE_SIZE: u32 = 262144;
    if address >= STAGING_IMAGE_SIZE { return; }
    let fw_data = hal.read_running_fw(address);
    let bytes = fw_data.to_le_bytes();
    *data.add(4) = bytes[0]; *data.add(5) = bytes[1];
    *data.add(6) = bytes[2]; *data.add(7) = bytes[3];
    hal.send_packet(data, PacketType::ResponseByte as u8, 8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_msgs(ptype: u8, data: *const u8, dev: *mut c_void) {
    if data.is_null() { return; }
    super::config::handle_api_msg(ptype, *data, data.add(1), dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_read_all_msgs(dev: *mut c_void) {
    super::config::handle_api_read_all(dev);
}

// ============================================================
// HID report + parser FFI (from hid.rs)
// ============================================================

#[export_name = "get_report_value"]
pub unsafe extern "C" fn rust_get_report_value(report: *const u8, len: i32, val: *const u8) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 { return 0; }
    let slice = core::slice::from_raw_parts(report, len as usize);
    let rv = core::ptr::read_unaligned(val as *const ReportVal);
    crate::domain::hid_report::get_report_value(slice, rv.offset, rv.size)
}

#[export_name = "extract_kbd_data"]
pub unsafe extern "C" fn rust_extract_kbd_data_export(
    raw_report: *mut u8, len: i32, itf: u8, iface: *mut c_void, out: *mut u8,
) -> i32 {
    rust_extract_kbd_data(raw_report, len, itf, iface, out)
}

#[export_name = "extract_data"]
pub unsafe extern "C" fn rust_extract_data_export(
    iface: *mut c_void, val: *const u8,
) {
    rust_extract_data(iface, val);
}

#[export_name = "parse_report_descriptor"]
pub unsafe extern "C" fn rust_parse_report_descriptor_export(
    iface: *mut c_void, report: *const u8, desc_len: i32,
) {
    rust_parse_report_descriptor(iface, report, desc_len);
}

// ============================================================
// HID parser (from hid_parser_ffi.rs)
// ============================================================

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls extract_data for each
/// parsed INPUT item to populate hid_interface_t.
#[no_mangle]
pub unsafe extern "C" fn rust_parse_report_descriptor(
    iface_ptr: *mut c_void,  // hid_interface_t*
    report: *const u8,
    desc_len: i32,
) {
    if iface_ptr.is_null() || report.is_null() || desc_len <= 0 {
        return;
    }

    let desc = core::slice::from_raw_parts(report, desc_len as usize);
    let (_parser, results) = hid_parser::parse_descriptor(desc);

    let iface = iface_from_ptr(iface_ptr);

    for input in results.iter() {
        if input.uses_report_id {
            iface.uses_report_id = true;
        }

        for i in 0..input.count {
            let val = &input.vals[i];
            rust_extract_data(iface_ptr, val as *const _ as *const u8);
        }
    }
}

// ============================================================
// Keyboard data extraction (from kbd_extract.rs)
// ============================================================

const KBD_EXTRACT_REPORT_LENGTH: usize = 8;
const KBD_EXTRACT_MAX_KEYS: usize = 32;
const KEYS_IN_USB_REPORT: usize = 6;
const MODIFIER_BIT_LENGTH: u16 = 8;
const HID_PROTOCOL_BOOT: u8 = 0;

/// Full extract_kbd_data -- replaces C implementation.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_kbd_data(
    raw_report: *mut u8,
    _len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,
    out_report: *mut u8,
) -> i32 {
    if raw_report.is_null() || iface_ptr.is_null() || out_report.is_null() || _len < 8 {
        return 0;
    }

    core::ptr::write_bytes(out_report, 0, KBD_EXTRACT_REPORT_LENGTH);

    let iface = iface_from_ptr(iface_ptr);
    let report_id = *raw_report;

    if iface.protocol == HID_PROTOCOL_BOOT {
        return extract_kbd_boot(raw_report, _len, out_report);
    }

    let kbd = get_keyboard(iface, report_id);
    if kbd.is_nkro {
        return extract_kbd_nkro(raw_report, _len as usize, iface, kbd, out_report);
    }

    if !iface.uses_report_id && (_len == KBD_EXTRACT_REPORT_LENGTH as i32 || _len == KBD_EXTRACT_REPORT_LENGTH as i32 + 1) {
        return extract_kbd_boot(raw_report, _len, out_report);
    }

    extract_kbd_other(raw_report, iface, kbd, out_report)
}

unsafe fn extract_kbd_boot(raw_report: *const u8, len: i32, out: *mut u8) -> i32 {
    let src = if len == KBD_EXTRACT_REPORT_LENGTH as i32 + 1 {
        raw_report.add(1)
    } else {
        raw_report
    };
    core::ptr::copy_nonoverlapping(src, out, KBD_EXTRACT_REPORT_LENGTH);
    KBD_EXTRACT_REPORT_LENGTH as i32
}

unsafe fn extract_kbd_other(
    raw_report: *const u8,
    iface: &HidInterface,
    kbd: &structs::KeyboardDescriptor,
    out: *mut u8,
) -> i32 {
    let mut src = raw_report;
    if iface.uses_report_id {
        src = src.add(1);
    }

    let mod_offset = { kbd.modifier.offset_idx } as usize;
    *out = *src.add(mod_offset);

    let mut j = 0usize;
    for i in 0..KBD_EXTRACT_MAX_KEYS {
        if j >= KEYS_IN_USB_REPORT { break; }
        if kbd.key_array[i] {
            *out.add(2 + j) = *src.add(i);
            j += 1;
        }
    }

    KBD_EXTRACT_REPORT_LENGTH as i32
}

unsafe fn extract_kbd_nkro(
    raw_report: *const u8, len: usize,
    iface: &HidInterface,
    kbd: &structs::KeyboardDescriptor,
    out: *mut u8,
) -> i32 {
    let usage_min = { kbd.nkro.usage_min };
    let usage_max = { kbd.nkro.usage_max };
    let nkro_size = { kbd.nkro.size };

    if (usage_max - usage_min + 1) != nkro_size as i32 {
        return -1;
    }

    let mod_size = { kbd.modifier.size };
    if mod_size != MODIFIER_BIT_LENGTH {
        return -1;
    }

    let mut ptr = raw_report;
    if iface.uses_report_id {
        ptr = ptr.add(1);
    }

    let mod_offset = { kbd.modifier.offset_idx } as usize;
    *out = *ptr.add(mod_offset);

    let nkro_offset = { kbd.nkro.offset_idx } as usize;
    let nkro_ptr = ptr.add(nkro_offset);
    let nkro_report = core::slice::from_raw_parts(nkro_ptr, core::cmp::min(len, 32));
    let keycode = core::slice::from_raw_parts_mut(out.add(2), KEYS_IN_USB_REPORT);

    crate::domain::hid_report::extract_bit_variable(
        nkro_report, usage_min, usage_max, 0, keycode,
    ) as i32
}

// ============================================================
// extract_data (from extract_data.rs)
// ============================================================

/// Rust implementation of extract_data -- replaces C version.
/// Classifies the ReportVal and populates hid_interface_t fields directly.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_data(iface_ptr: *mut c_void, val_ptr: *const u8) {
    if iface_ptr.is_null() || val_ptr.is_null() { return; }

    let val = core::ptr::read_unaligned(val_ptr as *const ReportVal);
    let rid = { val.report_id };
    let iface = iface_from_ptr(iface_ptr);

    match classify_report_val(&val) {
        ExtractedType::MouseButtons => {
            if is_padding(&val) {
                // Add padding to existing buttons size
                let current = { iface.mouse.buttons.size };
                iface.mouse.buttons.size = current + { val.size };
            } else {
                iface.mouse.buttons = val;
                iface.mouse.is_found = true;
            }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 0);
            }
        }
        ExtractedType::MouseX => {
            if !is_padding(&val) { iface.mouse.move_x = val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 0);
            }
        }
        ExtractedType::MouseY => {
            if !is_padding(&val) { iface.mouse.move_y = val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 0);
            }
        }
        ExtractedType::MouseWheel => {
            if !is_padding(&val) { iface.mouse.wheel = val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 0);
            }
        }
        ExtractedType::MousePan => {
            if !is_padding(&val) { iface.mouse.pan = val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 0);
            }
        }
        ExtractedType::Keyboard => {
            handle_keyboard_descriptor(iface, &val);
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 1);
            }
        }
        ExtractedType::ConsumerControl => {
            handle_consumer_control(iface, &val);
            iface.consumer.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 2);
            }
        }
        ExtractedType::SystemControl => {
            if !is_padding(&val) { iface.system.val = val; }
            iface.system.report_id = rid;
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 3);
            }
        }
        ExtractedType::Unknown => {}
    }
}

/// Find keyboard index by report_id (returns index, not reference -- avoids borrow conflicts)
fn find_keyboard_idx(iface: &HidInterface, rid: u8) -> usize {
    if iface.num_keyboards == 1 || !iface.uses_report_id {
        return 0;
    }
    for n in 0..iface.num_keyboards as usize {
        if n < MAX_KEYBOARDS && iface.keyboards[n].report_id == rid {
            return n;
        }
    }
    0
}

/// Replaces C handle_keyboard_descriptor_values
fn handle_keyboard_descriptor(iface: &mut HidInterface, val: &ReportVal) {
    let item_type = { val.item_type };
    let data_type = { val.data_type };
    let size = { val.size };
    let offset_idx = { val.offset_idx };
    let usage_min = { val.usage_min };
    let usage_max = { val.usage_max };

    if item_type == CONSTANT || iface.num_keyboards >= MAX_KEYBOARDS as u8 {
        return;
    }

    let ki = find_keyboard_idx(iface, val.report_id );
    let kbd = &mut iface.keyboards[ki];

    const KBD_MODIFIER_BIT_LENGTH: u16 = 8;
    if size <= KBD_MODIFIER_BIT_LENGTH && data_type == VARIABLE
        && usage_min <= 0xE0 && usage_max >= 0xE0
    {
        kbd.modifier = *val;
    }

    if (offset_idx as usize) < MAX_KEYS {
        kbd.key_array[offset_idx as usize] = data_type == ARRAY;
    }

    if size > 32 && data_type == VARIABLE {
        kbd.is_nkro = true;
        kbd.nkro = *val;
    }

    if !kbd.is_found {
        kbd.is_found = true;
        iface.num_keyboards += 1;
    }
}

/// Replaces C handle_consumer_control_values
fn handle_consumer_control(iface: &mut HidInterface, val: &ReportVal) {
    let offset = { val.offset } as usize;
    let data_type = { val.data_type };
    let usage = { val.usage };

    if offset > MAX_CC_BUTTONS { return; }

    let ki = find_keyboard_idx(iface, val.report_id );
    if data_type == VARIABLE {
        if offset < iface.keyboards[ki].cc_array.len() {
            iface.keyboards[ki].cc_array[offset] = usage;
        }
        iface.consumer.is_variable = true;
    }

    iface.consumer.is_array |= data_type == ARRAY;
}
