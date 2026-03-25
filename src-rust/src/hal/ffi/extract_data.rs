use core::ffi::c_void;
use crate::app::hid_parser::*;
use crate::app::extract::{classify_report_val, is_padding, ExtractedType};
use crate::app::structs::{iface_from_ptr, HidInterface, MAX_REPORTS, MAX_KEYBOARDS, MAX_KEYS, MAX_CC_BUTTONS};
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
            handle_keyboard_descriptor(iface, &val);
            if (rid as usize) < MAX_REPORTS {
                device::hal_set_report_handler(iface_ptr, rid, 1);
            }
        }
        ExtractedType::ConsumerControl => {
            handle_consumer_control(iface, &val);
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

/// Find keyboard index by report_id (returns index, not reference — avoids borrow conflicts)
fn find_keyboard_idx(iface: &HidInterface, rid: u8) -> usize {
    if iface.num_keyboards == 1 || !iface.uses_report_id {
        return 0;
    }
    for n in 0..iface.num_keyboards as usize {
        if n < MAX_KEYBOARDS && iface.keyboards[n].report_id == rid {
            return n;
        }
    }
    0
}

/// Replaces C handle_keyboard_descriptor_values
fn handle_keyboard_descriptor(iface: &mut HidInterface, val: &ReportVal) {
    let item_type = { val.item_type };
    let data_type = { val.data_type };
    let size = { val.size };
    let offset_idx = { val.offset_idx };
    let usage_min = { val.usage_min };
    let usage_max = { val.usage_max };

    if item_type == CONSTANT || iface.num_keyboards >= MAX_KEYBOARDS as u8 {
        return;
    }

    let ki = find_keyboard_idx(iface, { val.report_id });
    let kbd = &mut iface.keyboards[ki];

    const MODIFIER_BIT_LENGTH: u16 = 8;
    if size <= MODIFIER_BIT_LENGTH && data_type == VARIABLE
        && usage_min <= 0xE0 && usage_max >= 0xE0
    {
        kbd.modifier = *val;
    }

    if (offset_idx as usize) < MAX_KEYS {
        kbd.key_array[offset_idx as usize] = data_type == ARRAY;
    }

    if size > 32 && data_type == VARIABLE {
        kbd.is_nkro = true;
        kbd.nkro = *val;
    }

    if !kbd.is_found {
        kbd.is_found = true;
        iface.num_keyboards += 1;
    }
}

/// Replaces C handle_consumer_control_values
fn handle_consumer_control(iface: &mut HidInterface, val: &ReportVal) {
    let offset = { val.offset } as usize;
    let data_type = { val.data_type };
    let usage = { val.usage };

    if offset > MAX_CC_BUTTONS { return; }

    let ki = find_keyboard_idx(iface, { val.report_id });
    if data_type == VARIABLE {
        if offset < iface.keyboards[ki].cc_array.len() {
            iface.keyboards[ki].cc_array[offset] = usage;
        }
        iface.consumer.is_variable = true;
    }

    iface.consumer.is_array |= data_type == ARRAY;
}
