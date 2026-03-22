use core::ffi::c_void;
use crate::app::constants::{MAX_SCREEN_COORD, MIN_SCREEN_COORD};
use crate::app::mouse;
use crate::hal::device;

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
