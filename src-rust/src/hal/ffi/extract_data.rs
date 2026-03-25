use core::ffi::c_void;
use crate::app::hid_parser::*;
use crate::app::extract::{classify_report_val, is_padding, ExtractedType};
use crate::app::structs::{iface_from_ptr, MAX_REPORTS};
use crate::hal::device;

/// Rust implementation of extract_data — replaces C version.
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
            // Complex C logic — keep HAL wrapper
            device::hal_handle_keyboard_descriptor(iface_ptr, val_ptr);
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 1);
            }
        }
        ExtractedType::ConsumerControl => {
            // Complex C logic — keep HAL wrapper
            device::hal_handle_consumer_control_values(iface_ptr, val_ptr);
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
