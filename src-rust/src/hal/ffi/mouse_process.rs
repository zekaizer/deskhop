use core::ffi::c_void;
use crate::hal::device;
use crate::app::mouse_logic;

/// Full mouse report processing pipeline — replaces C process_mouse_report.
#[no_mangle]
pub unsafe extern "C" fn rust_process_mouse_report(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface: *mut c_void,  // hid_interface_t*
    dev: *mut c_void,    // device_t*
) {
    if raw_report.is_null() || iface.is_null() || dev.is_null() {
        return;
    }

    let state = &mut *crate::app::state::rust_get_app_state();

    // Extract mouse values from HID report (delegates to C for hid_interface_t access)
    let mut values = [0i32; 5]; // [move_x, move_y, wheel, pan, buttons]
    device::hal_extract_report_values(raw_report, len, dev, iface, values.as_mut_ptr());

    let mouse_vals = mouse_logic::MouseValues {
        move_x: values[0],
        move_y: values[1],
        wheel: values[2],
        pan: values[3],
        buttons: values[4],
    };

    // Update mouse position and detect screen switch
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

    // Create mouse report
    let report = mouse_logic::create_mouse_report(
        state.pointer_x, state.pointer_y, &mouse_vals,
        state.relative_mouse, state.gaming_mode,
    );

    // Output mouse report (local queue or UART)
    let report_bytes = [
        report.buttons,
        report.x.to_le_bytes()[0], report.x.to_le_bytes()[1],
        report.y.to_le_bytes()[0], report.y.to_le_bytes()[1],
        report.wheel as u8,
        report.pan as u8,
        report.mode,
    ];

    if state.is_active_output() {
        device::hal_queue_mouse_report(dev, report_bytes.as_ptr());
        let role = state.board_role as usize;
        if role < state.last_activity.len() {
            state.last_activity[role] = device::hal_time_us_64();
        }
    } else {
        device::hal_queue_packet(
            report_bytes.as_ptr(),
            crate::app::constants::PacketType::MouseReport as u8,
            8,
        );
    }

    // Screen switch handling
    let dir_code = match dir {
        mouse_logic::SwitchDirection::None => return,
        mouse_logic::SwitchDirection::Left => 1u8,
        mouse_logic::SwitchDirection::Right => 2u8,
    };

    // Delegate screen switch to C (needs set_active_output + virtual desktop logic)
    // For now, call the C do_screen_switch via HAL
    extern "C" {
        fn do_screen_switch(dev: *mut c_void, direction: i32);
    }

    let c_dir = match dir {
        mouse_logic::SwitchDirection::Left => 1i32,  // LEFT enum
        mouse_logic::SwitchDirection::Right => 2i32,  // RIGHT enum
        _ => return,
    };
    do_screen_switch(dev, c_dir);
}
