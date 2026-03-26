use core::ffi::c_void;
use crate::domain::mouse_logic;
use crate::domain::hid_parser::ReportVal;

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
    let iface = crate::domain::structs::iface_from_ptr(iface_ptr);

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

    use crate::app::router::ReportRouter;
    hal.route_mouse(state, report_bytes.as_ptr());

    // Screen switch handling
    let c_dir = match dir {
        mouse_logic::SwitchDirection::None => return,
        mouse_logic::SwitchDirection::Left => 1i32,
        mouse_logic::SwitchDirection::Right => 2i32,
    };
    super::screen_switch::rust_do_screen_switch(dev, c_dir);
}
