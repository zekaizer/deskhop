// UART message handler FFI — process_packet dispatcher callbacks.

use crate::domain::constants::PacketType;
use crate::domain::actions::{get_border_position, BorderUpdate};
use crate::app::router::ReportRouter;
use crate::hal::traits::*;

fn border_to_bytes(top: i32, bottom: i32) -> [u8; 8] {
    let t = top.to_le_bytes();
    let b = bottom.to_le_bytes();
    [t[0], t[1], t[2], t[3], b[0], b[1], b[2], b[3]]
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_simple_msg(ptype: u8, data: *const u8, dev: *mut core::ffi::c_void) -> u8 {
    if data.is_null() { return 0; }
    let state = crate::domain::structs::device_from_ptr(dev);
    let mut arr = [0u8; 8];
    core::ptr::copy_nonoverlapping(data, arr.as_mut_ptr(), 8);
    let action = crate::domain::msg_handlers::handle_simple_msg(ptype, &arr, state);
    if crate::domain::msg_handlers::apply_action(&action, state) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_output_select(dev: *mut core::ffi::c_void, output: u8) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    state.active_output = output;
    if state.tud_connected { crate::app::host_link::release_all_keys(state, &hal); }
    hal.sync_leds();
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_keyboard_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
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
pub unsafe extern "C" fn rust_handle_mouse_uart_full(dev: *mut core::ffi::c_void, data: *const u8) {
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
pub unsafe extern "C" fn rust_handle_set_report(dev: *mut core::ffi::c_void, led_value: u8) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);
    let other = 1usize.wrapping_sub(state.board_role as usize);
    if other < state.keyboard_leds.len() { state.keyboard_leds[other] = led_value; }
    if state.keyboard_connected && !state.is_active_output() {
        hal.sync_leds();
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_sync_borders(dev: *mut core::ffi::c_void, data: *const u8) {
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
