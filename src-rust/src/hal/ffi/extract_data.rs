use core::ffi::c_void;
use crate::app::hid_parser::*;
use crate::app::extract::{classify_report_val, is_padding, ExtractedType};
use crate::app::structs::MAX_REPORTS;
use crate::hal::device;

/// Serialize ReportVal to packed C layout (23 bytes)
fn serialize_report_val(val: &ReportVal, buf: &mut [u8; 24]) {
    buf[0..2].copy_from_slice(&val.offset.to_le_bytes());
    buf[2..4].copy_from_slice(&val.offset_idx.to_le_bytes());
    buf[4..6].copy_from_slice(&val.size.to_le_bytes());
    buf[6..10].copy_from_slice(&val.usage_min.to_le_bytes());
    buf[10..14].copy_from_slice(&val.usage_max.to_le_bytes());
    buf[14] = val.item_type;
    buf[15] = val.data_type;
    buf[16] = val.report_id;
    buf[17..19].copy_from_slice(&val.global_usage.to_le_bytes());
    buf[19..21].copy_from_slice(&val.usage_page.to_le_bytes());
    buf[21..23].copy_from_slice(&val.usage.to_le_bytes());
}

/// Rust implementation of extract_data — replaces C version.
/// Classifies the ReportVal and populates hid_interface_t fields via HAL setters.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_data(iface: *mut c_void, val_ptr: *const u8) {
    if iface.is_null() || val_ptr.is_null() { return; }

    // Deserialize ReportVal from packed bytes
    let val = ReportVal {
        offset: u16::from_le_bytes([*val_ptr, *val_ptr.add(1)]),
        offset_idx: u16::from_le_bytes([*val_ptr.add(2), *val_ptr.add(3)]),
        size: u16::from_le_bytes([*val_ptr.add(4), *val_ptr.add(5)]),
        usage_min: i32::from_le_bytes([*val_ptr.add(6), *val_ptr.add(7), *val_ptr.add(8), *val_ptr.add(9)]),
        usage_max: i32::from_le_bytes([*val_ptr.add(10), *val_ptr.add(11), *val_ptr.add(12), *val_ptr.add(13)]),
        item_type: *val_ptr.add(14),
        data_type: *val_ptr.add(15),
        report_id: *val_ptr.add(16),
        global_usage: u16::from_le_bytes([*val_ptr.add(17), *val_ptr.add(18)]),
        usage_page: u16::from_le_bytes([*val_ptr.add(19), *val_ptr.add(20)]),
        usage: u16::from_le_bytes([*val_ptr.add(21), *val_ptr.add(22)]),
    };

    let mut packed = [0u8; 24];
    serialize_report_val(&val, &mut packed);
    let rid = val.report_id;

    match classify_report_val(&val) {
        ExtractedType::MouseButtons => {
            if is_padding(&val) {
                device::hal_add_mouse_buttons_padding(iface, val.size);
            } else {
                device::hal_set_mouse_buttons(iface, packed.as_ptr());
            }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); } // mouse
        }
        ExtractedType::MouseX => {
            if !is_padding(&val) { device::hal_set_mouse_move_x(iface, packed.as_ptr()); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MouseY => {
            if !is_padding(&val) { device::hal_set_mouse_move_y(iface, packed.as_ptr()); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MouseWheel => {
            if !is_padding(&val) { device::hal_set_mouse_wheel(iface, packed.as_ptr()); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::MousePan => {
            if !is_padding(&val) { device::hal_set_mouse_pan(iface, packed.as_ptr()); }
            device::hal_set_mouse_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 0); }
        }
        ExtractedType::Keyboard => {
            device::hal_handle_keyboard_descriptor(iface, packed.as_ptr());
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 1); } // keyboard
        }
        ExtractedType::ConsumerControl => {
            device::hal_handle_consumer_control_values(iface, packed.as_ptr());
            device::hal_set_consumer_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 2); } // consumer
        }
        ExtractedType::SystemControl => {
            if !is_padding(&val) { device::hal_set_system_val(iface, packed.as_ptr()); }
            device::hal_set_system_report_id(iface, rid);
            if rid < MAX_REPORTS as u8 { device::hal_set_report_handler(iface, rid, 3); } // system
        }
        ExtractedType::Unknown => {}
    }
}
