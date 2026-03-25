use core::ffi::c_void;
use crate::app::structs::{iface_from_ptr, get_keyboard};

const KBD_REPORT_LENGTH: usize = 8;
const MAX_KEYS: usize = 32;
const KEYS_IN_USB_REPORT: usize = 6;
const MODIFIER_BIT_LENGTH: u16 = 8;
const HID_PROTOCOL_BOOT: u8 = 0;

/// Full extract_kbd_data — replaces C implementation.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_kbd_data(
    raw_report: *mut u8,
    _len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,
    out_report: *mut u8,
) -> i32 {
    if raw_report.is_null() || iface_ptr.is_null() || out_report.is_null() || _len < 8 {
        return 0;
    }

    core::ptr::write_bytes(out_report, 0, KBD_REPORT_LENGTH);

    let iface = iface_from_ptr(iface_ptr);
    let report_id = *raw_report;

    if iface.protocol == HID_PROTOCOL_BOOT {
        return extract_kbd_boot(raw_report, _len, out_report);
    }

    let kbd = get_keyboard(iface, report_id);
    if kbd.is_nkro {
        return extract_kbd_nkro(raw_report, _len as usize, iface, kbd, out_report);
    }

    if !iface.uses_report_id && (_len == KBD_REPORT_LENGTH as i32 || _len == KBD_REPORT_LENGTH as i32 + 1) {
        return extract_kbd_boot(raw_report, _len, out_report);
    }

    extract_kbd_other(raw_report, iface, kbd, out_report)
}

unsafe fn extract_kbd_boot(raw_report: *const u8, len: i32, out: *mut u8) -> i32 {
    let src = if len == KBD_REPORT_LENGTH as i32 + 1 {
        raw_report.add(1)
    } else {
        raw_report
    };
    core::ptr::copy_nonoverlapping(src, out, KBD_REPORT_LENGTH);
    KBD_REPORT_LENGTH as i32
}

unsafe fn extract_kbd_other(
    raw_report: *const u8,
    iface: &crate::app::structs::HidInterface,
    kbd: &crate::app::structs::KeyboardDescriptor,
    out: *mut u8,
) -> i32 {
    let mut src = raw_report;
    if iface.uses_report_id {
        src = src.add(1);
    }

    let mod_offset = { kbd.modifier.offset_idx } as usize;
    *out = *src.add(mod_offset);

    let mut j = 0usize;
    for i in 0..MAX_KEYS {
        if j >= KEYS_IN_USB_REPORT { break; }
        if kbd.key_array[i] {
            *out.add(2 + j) = *src.add(i);
            j += 1;
        }
    }

    KBD_REPORT_LENGTH as i32
}

unsafe fn extract_kbd_nkro(
    raw_report: *const u8, len: usize,
    iface: &crate::app::structs::HidInterface,
    kbd: &crate::app::structs::KeyboardDescriptor,
    out: *mut u8,
) -> i32 {
    let usage_min = { kbd.nkro.usage_min };
    let usage_max = { kbd.nkro.usage_max };
    let nkro_size = { kbd.nkro.size };

    if (usage_max - usage_min + 1) != nkro_size as i32 {
        return -1;
    }

    let mod_size = { kbd.modifier.size };
    if mod_size != MODIFIER_BIT_LENGTH {
        return -1;
    }

    let mut ptr = raw_report;
    if iface.uses_report_id {
        ptr = ptr.add(1);
    }

    let mod_offset = { kbd.modifier.offset_idx } as usize;
    *out = *ptr.add(mod_offset);

    let nkro_offset = { kbd.nkro.offset_idx } as usize;
    let nkro_ptr = ptr.add(nkro_offset);
    let nkro_report = core::slice::from_raw_parts(nkro_ptr, core::cmp::min(len, 32));
    let keycode = core::slice::from_raw_parts_mut(out.add(2), KEYS_IN_USB_REPORT);

    crate::app::hid_report::extract_bit_variable(
        nkro_report, usage_min, usage_max, 0, keycode,
    ) as i32
}
