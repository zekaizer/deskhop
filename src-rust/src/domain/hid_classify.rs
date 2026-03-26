// HID descriptor data extraction — determines which handler should process
// a parsed report value based on usage page / global usage / usage matching.

use crate::domain::hid_parser::*;
use crate::domain::structs::{HidInterface, MAX_KEYBOARDS, MAX_KEYS, MAX_CC_BUTTONS, MAX_REPORTS};

/// What kind of HID data was found during descriptor parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractedType {
    MouseButtons,
    MouseX,
    MouseY,
    MouseWheel,
    MousePan,
    Keyboard,
    ConsumerControl,
    SystemControl,
    Unknown,
}

/// Match a parsed ReportVal to a semantic type based on usage page and usage.
/// This replaces C's extract_data() usage_map matching logic.
pub fn classify_report_val(val: &ReportVal) -> ExtractedType {
    let up = val.usage_page;
    let gu = val.global_usage;
    let u = val.usage;

    // Mouse buttons: Button page + Mouse global usage
    if up == HID_USAGE_PAGE_BUTTON && gu == HID_USAGE_DESKTOP_MOUSE {
        return ExtractedType::MouseButtons;
    }

    // Mouse X/Y/Wheel: Desktop page + Mouse global usage
    if up == HID_USAGE_PAGE_DESKTOP && gu == HID_USAGE_DESKTOP_MOUSE {
        return match u {
            HID_USAGE_DESKTOP_X => ExtractedType::MouseX,
            HID_USAGE_DESKTOP_Y => ExtractedType::MouseY,
            HID_USAGE_DESKTOP_WHEEL => ExtractedType::MouseWheel,
            _ => ExtractedType::Unknown,
        };
    }

    // Mouse Pan: Consumer page + Mouse global usage
    if up == HID_USAGE_PAGE_CONSUMER && gu == HID_USAGE_DESKTOP_MOUSE {
        if u == HID_USAGE_CONSUMER_AC_PAN {
            return ExtractedType::MousePan;
        }
    }

    // Keyboard: Keyboard page + Keyboard global usage
    if up == HID_USAGE_PAGE_KEYBOARD && gu == HID_USAGE_DESKTOP_KEYBOARD {
        return ExtractedType::Keyboard;
    }

    // Consumer Control: Consumer page + Consumer Control global usage
    if up == HID_USAGE_PAGE_CONSUMER && gu == HID_USAGE_CONSUMER_CONTROL {
        return ExtractedType::ConsumerControl;
    }

    // System Control: Desktop page + System Control global usage
    if up == HID_USAGE_PAGE_DESKTOP && gu == HID_USAGE_DESKTOP_SYSTEM_CONTROL {
        return ExtractedType::SystemControl;
    }

    ExtractedType::Unknown
}

/// Determine if a value represents a constant (padding) — should not be stored.
pub fn is_padding(val: &ReportVal) -> bool {
    val.item_type == CONSTANT
}

/// Determine if this value uses NKRO format (size > 32 bits, variable type)
pub fn is_nkro(val: &ReportVal) -> bool {
    val.size > 32 && val.data_type == VARIABLE
}

/// Check if this is a modifier key (left control 0xE0 within usage range)
pub fn is_modifier_key(val: &ReportVal) -> bool {
    const LEFT_CTRL: i32 = 0xE0;
    const MODIFIER_BIT_LENGTH: u16 = 8;

    val.size <= MODIFIER_BIT_LENGTH
        && val.data_type == VARIABLE
        && LEFT_CTRL >= val.usage_min
        && LEFT_CTRL <= val.usage_max
}

/// Find keyboard index by report_id (returns index, not reference -- avoids borrow conflicts).
pub fn find_keyboard_idx(iface: &HidInterface, rid: u8) -> usize {
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

/// Populate keyboard descriptor fields from a parsed ReportVal.
pub fn handle_keyboard_descriptor(iface: &mut HidInterface, val: &ReportVal) {
    let item_type = val.item_type;
    let data_type = val.data_type;
    let size = val.size;
    let offset_idx = val.offset_idx;
    let usage_min = val.usage_min;
    let usage_max = val.usage_max;

    if item_type == CONSTANT || iface.num_keyboards >= MAX_KEYBOARDS as u8 {
        return;
    }

    let ki = find_keyboard_idx(iface, val.report_id);
    let kbd = &mut iface.keyboards[ki];

    const KBD_MODIFIER_BIT_LENGTH: u16 = 8;
    if size <= KBD_MODIFIER_BIT_LENGTH && data_type == VARIABLE
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

/// Populate consumer control descriptor fields from a parsed ReportVal.
pub fn handle_consumer_control(iface: &mut HidInterface, val: &ReportVal) {
    let offset = val.offset as usize;
    let data_type = val.data_type;
    let usage = val.usage;

    if offset > MAX_CC_BUTTONS { return; }

    let ki = find_keyboard_idx(iface, val.report_id);
    if data_type == VARIABLE {
        if offset < iface.keyboards[ki].cc_array.len() {
            iface.keyboards[ki].cc_array[offset] = usage;
        }
        iface.consumer.is_variable = true;
    }

    iface.consumer.is_array |= data_type == ARRAY;
}

/// Classify a ReportVal and populate the corresponding HidInterface field.
/// Returns `Some(handler_type)` if a report handler should be registered for this
/// report_id, or `None` if no handler registration is needed (e.g. Unknown type).
pub fn populate_interface_field(iface: &mut HidInterface, val: &ReportVal) -> Option<u8> {
    let rid = val.report_id;

    match classify_report_val(val) {
        ExtractedType::MouseButtons => {
            if is_padding(val) {
                let current = iface.mouse.buttons.size;
                iface.mouse.buttons.size = current + val.size;
            } else {
                iface.mouse.buttons = *val;
                iface.mouse.is_found = true;
            }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(0) } else { None }
        }
        ExtractedType::MouseX => {
            if !is_padding(val) { iface.mouse.move_x = *val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(0) } else { None }
        }
        ExtractedType::MouseY => {
            if !is_padding(val) { iface.mouse.move_y = *val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(0) } else { None }
        }
        ExtractedType::MouseWheel => {
            if !is_padding(val) { iface.mouse.wheel = *val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(0) } else { None }
        }
        ExtractedType::MousePan => {
            if !is_padding(val) { iface.mouse.pan = *val; }
            iface.mouse.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(0) } else { None }
        }
        ExtractedType::Keyboard => {
            handle_keyboard_descriptor(iface, val);
            if (rid as usize) < MAX_REPORTS { Some(1) } else { None }
        }
        ExtractedType::ConsumerControl => {
            handle_consumer_control(iface, val);
            iface.consumer.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(2) } else { None }
        }
        ExtractedType::SystemControl => {
            if !is_padding(val) { iface.system.val = *val; }
            iface.system.report_id = rid;
            if (rid as usize) < MAX_REPORTS { Some(3) } else { None }
        }
        ExtractedType::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_val(usage_page: u16, global_usage: u16, usage: u16) -> ReportVal {
        ReportVal {
            usage_page,
            global_usage,
            usage,
            ..ReportVal::default()
        }
    }

    #[test]
    fn test_classify_mouse_buttons() {
        let val = make_val(HID_USAGE_PAGE_BUTTON, HID_USAGE_DESKTOP_MOUSE, 0);
        assert_eq!(classify_report_val(&val), ExtractedType::MouseButtons);
    }

    #[test]
    fn test_classify_mouse_x() {
        let val = make_val(HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_X);
        assert_eq!(classify_report_val(&val), ExtractedType::MouseX);
    }

    #[test]
    fn test_classify_mouse_y() {
        let val = make_val(HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_Y);
        assert_eq!(classify_report_val(&val), ExtractedType::MouseY);
    }

    #[test]
    fn test_classify_mouse_wheel() {
        let val = make_val(HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_WHEEL);
        assert_eq!(classify_report_val(&val), ExtractedType::MouseWheel);
    }

    #[test]
    fn test_classify_mouse_pan() {
        let val = make_val(HID_USAGE_PAGE_CONSUMER, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_CONSUMER_AC_PAN);
        assert_eq!(classify_report_val(&val), ExtractedType::MousePan);
    }

    #[test]
    fn test_classify_keyboard() {
        let val = make_val(HID_USAGE_PAGE_KEYBOARD, HID_USAGE_DESKTOP_KEYBOARD, 0);
        assert_eq!(classify_report_val(&val), ExtractedType::Keyboard);
    }

    #[test]
    fn test_classify_consumer_control() {
        let val = make_val(HID_USAGE_PAGE_CONSUMER, HID_USAGE_CONSUMER_CONTROL, 0);
        assert_eq!(classify_report_val(&val), ExtractedType::ConsumerControl);
    }

    #[test]
    fn test_classify_system_control() {
        let val = make_val(HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_SYSTEM_CONTROL, 0);
        assert_eq!(classify_report_val(&val), ExtractedType::SystemControl);
    }

    #[test]
    fn test_classify_unknown() {
        let val = make_val(0xFF, 0xFF, 0xFF);
        assert_eq!(classify_report_val(&val), ExtractedType::Unknown);
    }

    #[test]
    fn test_is_padding() {
        let mut val = ReportVal::default();
        val.item_type = CONSTANT;
        assert!(is_padding(&val));

        val.item_type = DATA;
        assert!(!is_padding(&val));
    }

    #[test]
    fn test_is_nkro() {
        let mut val = ReportVal::default();
        val.size = 240;
        val.data_type = VARIABLE;
        assert!(is_nkro(&val));

        val.size = 8;
        assert!(!is_nkro(&val));
    }

    #[test]
    fn test_is_modifier_key() {
        let val = ReportVal {
            size: 8,
            data_type: VARIABLE,
            usage_min: 0xE0,
            usage_max: 0xE7,
            ..ReportVal::default()
        };
        assert!(is_modifier_key(&val));

        // Not modifier — wrong range
        let val2 = ReportVal {
            size: 8,
            data_type: VARIABLE,
            usage_min: 0x00,
            usage_max: 0x0F,
            ..ReportVal::default()
        };
        assert!(!is_modifier_key(&val2));

        // Not modifier — array type
        let val3 = ReportVal {
            size: 8,
            data_type: ARRAY,
            usage_min: 0xE0,
            usage_max: 0xE7,
            ..ReportVal::default()
        };
        assert!(!is_modifier_key(&val3));
    }
}
