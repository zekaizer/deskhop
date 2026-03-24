use core::ffi::c_void;
use crate::app::hid_parser::*;
use crate::app::extract::{classify_report_val, is_padding, ExtractedType};
use crate::app::structs::MAX_REPORTS;
use crate::hal::device;

/// Rust implementation of extract_data — replaces C version.
/// Classifies the ReportVal and populates hid_interface_t fields via HAL setters.
/// Since ReportVal is #[repr(C, packed)] matching C's report_val_t, we can pass
/// it directly without serialization.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_data(iface: *mut c_void, val_ptr: *const u8) {
    if iface.is_null() || val_ptr.is_null() { return; }

    // ReportVal is packed = same layout as C report_val_t, read directly
    let val = core::ptr::read_unaligned(val_ptr as *const ReportVal);
    let rid = val.report_id;

    match classify_report_val(&val) {
        ExtractedType::MouseButtons => {
            if is_padding(&val) {
                device::hal_add_mouse_buttons_padding(iface, val.size);
            } else {
                device::hal_set_mouse_buttons(iface, val_ptr);
            }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MouseX => {
            if !is_padding(&val) { device::hal_set_mouse_move_x(iface, val_ptr); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MouseY => {
            if !is_padding(&val) { device::hal_set_mouse_move_y(iface, val_ptr); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MouseWheel => {
            if !is_padding(&val) { device::hal_set_mouse_wheel(iface, val_ptr); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MousePan => {
            if !is_padding(&val) { device::hal_set_mouse_pan(iface, val_ptr); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::Keyboard => {
            device::hal_handle_keyboard_descriptor(iface, val_ptr);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 1); }
        }
        ExtractedType::ConsumerControl => {
            device::hal_handle_consumer_control_values(iface, val_ptr);
            device::hal_set_consumer_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 2); }
        }
        ExtractedType::SystemControl => {
            if !is_padding(&val) { device::hal_set_system_val(iface, val_ptr); }
            device::hal_set_system_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 3); }
        }
        ExtractedType::Unknown => {}
    }
}
