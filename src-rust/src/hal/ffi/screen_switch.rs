use core::ffi::c_void;
use crate::app::constants::{MAX_SCREEN_COORD, MIN_SCREEN_COORD, ABSOLUTE, RELATIVE};
use crate::app::mouse;
use crate::hal::device;

const MACOS_SWITCH_MOVE_X: i16 = 10;
const MACOS_SWITCH_MOVE_COUNT: usize = 5;

/// Replace C's switch_to_another_pc
#[no_mangle]
pub unsafe extern "C" fn rust_switch_to_another_pc(
    dev: *mut c_void,
    output_number: u32,
    output_to: i32,
    direction: i32,  // LEFT=1, RIGHT=2
) {
    let state = &mut *crate::app::state::rust_get_app_state();
    let output_idx = state.active_output as usize;
    if output_idx >= state.config.output.len() { return; }

    let mouse_park_pos = state.config.output[output_idx].mouse_park_pos;
    let mouse_y = match mouse_park_pos {
        0 => MIN_SCREEN_COORD,       // Top
        1 => MAX_SCREEN_COORD,       // Bottom
        _ => state.pointer_y,        // Previous
    };

    // Send hidden pointer to edge
    let hidden = [
        0u8,                                          // buttons
        MAX_SCREEN_COORD.to_le_bytes()[0],           // x low
        MAX_SCREEN_COORD.to_le_bytes()[1],           // x high
        mouse_y.to_le_bytes()[0],                    // y low
        mouse_y.to_le_bytes()[1],                    // y high
        0, 0, 0,                                     // wheel, pan, mode
    ];

    // Route via output_mouse_report logic
    if state.is_active_output() {
        device::hal_queue_mouse_report(dev, hidden.as_ptr());
    } else {
        device::hal_queue_packet(
            hidden.as_ptr(), crate::app::constants::PacketType::MouseReport as u8, 8,
        );
    }

    // Switch output
    device::hal_set_active_output(dev, output_to as u8);

    // Update pointer position
    state.pointer_x = if direction == 1 { MAX_SCREEN_COORD } else { MIN_SCREEN_COORD };

    // Scale Y coordinate for the target screen
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

/// Helper to output a mouse report via the routing logic
unsafe fn output_report(dev: *mut c_void, report: &[u8; 8]) {
    let state = &mut *crate::app::state::rust_get_app_state();
    if state.is_active_output() {
        device::hal_queue_mouse_report(dev, report.as_ptr());
    } else {
        device::hal_queue_packet(
            report.as_ptr(), crate::app::constants::PacketType::MouseReport as u8, 8,
        );
    }
}

/// Replace C's switch_virtual_desktop_macos
#[no_mangle]
pub unsafe extern "C" fn rust_switch_virtual_desktop_macos(dev: *mut c_void, direction: i32) {
    let state = &*crate::app::state::rust_get_app_state();
    let left = direction == 1;

    let edge_x = if left { MIN_SCREEN_COORD } else { MAX_SCREEN_COORD };
    let edge = [
        state.mouse_buttons as u8,
        edge_x.to_le_bytes()[0], edge_x.to_le_bytes()[1],
        (MAX_SCREEN_COORD / 2).to_le_bytes()[0], (MAX_SCREEN_COORD / 2).to_le_bytes()[1],
        0, 0, ABSOLUTE,
    ];
    output_report(dev, &edge);

    let move_x: i16 = if left { -MACOS_SWITCH_MOVE_X } else { MACOS_SWITCH_MOVE_X };
    let rel = [
        state.mouse_buttons as u8,
        move_x.to_le_bytes()[0], move_x.to_le_bytes()[1],
        0, 0, 0, 0, RELATIVE,
    ];
    for _ in 0..MACOS_SWITCH_MOVE_COUNT {
        output_report(dev, &rel);
    }
}

/// Replace C's switch_virtual_desktop
#[no_mangle]
pub unsafe extern "C" fn rust_switch_virtual_desktop(
    dev: *mut c_void, os: u8, new_index: i32, direction: i32,
) {
    let state = &mut *crate::app::state::rust_get_app_state();
    const MACOS: u8 = 2;
    const WINDOWS: u8 = 3;

    match os {
        MACOS => rust_switch_virtual_desktop_macos(dev, direction),
        WINDOWS => { state.relative_mouse = new_index > 1; }
        _ => {} // Linux/Android/Other — no special handling
    }

    state.pointer_x = if direction == 2 { MIN_SCREEN_COORD } else { MAX_SCREEN_COORD }; // RIGHT=2
}
