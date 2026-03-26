use core::ffi::c_void;
use crate::domain::constants::{MAX_SCREEN_COORD, MIN_SCREEN_COORD, ABSOLUTE, RELATIVE};
use crate::domain::mouse;
use crate::hal::traits::*;

const MACOS_SWITCH_MOVE_X: i16 = 10;
const MACOS_SWITCH_MOVE_COUNT: usize = 5;


unsafe fn hal_from(dev: *mut c_void) -> crate::hal::pico::PicoHal {
    crate::hal::pico::PicoHal::new(dev)
}

/// Helper to output a mouse report via the routing logic

unsafe fn output_report(hal: &(impl ReportQueue + PeerLink), state: &crate::domain::structs::Device, report: &[u8; 8]) {
    if state.is_active_output() {
        hal.push_mouse_report(report.as_ptr());
    } else {
        hal.send_packet(
            report.as_ptr(), crate::domain::constants::PacketType::MouseReport as u8, 8,
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
    let hal = hal_from(dev);
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
    let hal = hal_from(dev);
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
    use crate::domain::mouse_logic::*;

    let state = crate::domain::structs::device_from_ptr(dev);
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    let output = &state.config.output[output_idx];
    let dir = match direction {
        1 => SwitchDirection::Left,
        2 => SwitchDirection::Right,
        _ => return,
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
        ScreenSwitchAction::Nothing => {}
        ScreenSwitchAction::SwitchToOtherPc => {
            let output_number = output.number;
            rust_switch_to_another_pc(dev, output_number, (1 - state.active_output) as i32, direction);
        }
        ScreenSwitchAction::SwitchVirtualDesktop { new_index } => {
            let os = output.os;
            rust_switch_virtual_desktop(dev, os, new_index as i32, direction);
            state.config.output[output_idx].screen_index = new_index;
        }
    }
}
