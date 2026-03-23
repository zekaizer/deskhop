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

    // Extract mouse values — use HAL getters for hid_interface_t mouse fields
    let mut values = [0i32; 5];
    let protocol = device::hal_get_iface_protocol(iface);
    const HID_PROTOCOL_BOOT: u8 = 0;

    if protocol == HID_PROTOCOL_BOOT {
        // Boot protocol: fixed layout [buttons, x, y, wheel, pan]
        values[0] = *raw_report.add(1) as i8 as i32; // x
        values[1] = *raw_report.add(2) as i8 as i32; // y
        values[2] = *raw_report.add(3) as i8 as i32; // wheel
        values[3] = 0; // pan (not in boot)
        values[4] = *raw_report as i32; // buttons
    } else {
        // Report protocol: use descriptor-parsed field locations
        let uses_id = device::hal_get_iface_uses_report_id(iface);
        let report_slice = core::slice::from_raw_parts(raw_report, len as usize);

        fn extract_val(report: &[u8], uses_id: bool, val_ptr: *const u8) -> Option<i32> {
            if val_ptr.is_null() { return None; }
            unsafe {
                let report_id = *val_ptr.add(16); // report_val_t.report_id at offset 16
                let src = if uses_id {
                    if report[0] != report_id { return None; }
                    &report[1..]
                } else { report };
                let offset = u16::from_le_bytes([*val_ptr, *val_ptr.add(1)]);
                let size = u16::from_le_bytes([*val_ptr.add(4), *val_ptr.add(5)]);
                Some(crate::app::hid_report::get_report_value(src, offset, size))
            }
        }

        if let Some(v) = extract_val(report_slice, uses_id, device::hal_get_mouse_move_x_val(iface)) { values[0] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, device::hal_get_mouse_move_y_val(iface)) { values[1] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, device::hal_get_mouse_wheel_val(iface)) { values[2] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, device::hal_get_mouse_pan_val(iface)) { values[3] = v; }
        if let Some(v) = extract_val(report_slice, uses_id, device::hal_get_mouse_buttons_val(iface)) {
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

    // Call Rust's do_screen_switch directly (no C roundtrip)
    let c_dir = match dir {
        mouse_logic::SwitchDirection::Left => 1i32,
        mouse_logic::SwitchDirection::Right => 2i32,
        _ => return,
    };
    super::screen_switch::rust_do_screen_switch(dev, c_dir);
}
