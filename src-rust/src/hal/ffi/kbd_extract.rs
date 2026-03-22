use core::ffi::c_void;
use crate::hal::device;

const KBD_REPORT_LENGTH: usize = 8;
const MAX_KEYS: usize = 32;
const KEYS_IN_USB_REPORT: usize = 6;
const MODIFIER_BIT_LENGTH: u16 = 8;
const HID_PROTOCOL_BOOT: u8 = 0;

/// Full extract_kbd_data — replaces C implementation.
/// Dispatches to boot/nkro/other extraction based on protocol and descriptor.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_kbd_data(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface: *mut c_void,
    out_report: *mut u8,
) -> i32 {
    if raw_report.is_null() || iface.is_null() || out_report.is_null() || len < 8 {
        return 0;
    }

    // Clear output
    core::ptr::write_bytes(out_report, 0, KBD_REPORT_LENGTH);

    let protocol = device::hal_get_iface_protocol(iface);
    let report_id = *raw_report;

    // Boot protocol — simple memcpy
    if protocol == HID_PROTOCOL_BOOT {
        return extract_kbd_boot(raw_report, len, out_report);
    }

    // NKRO
    if device::hal_get_kbd_is_nkro(iface, report_id) {
        return extract_kbd_nkro(raw_report, len as usize, iface, report_id, out_report);
    }

    // Standard 8-byte report without report ID
    let uses_report_id = device::hal_get_iface_uses_report_id(iface);
    if !uses_report_id && (len == KBD_REPORT_LENGTH as i32 || len == KBD_REPORT_LENGTH as i32 + 1) {
        return extract_kbd_boot(raw_report, len, out_report);
    }

    // Other — parsed descriptor
    extract_kbd_other(raw_report, len as usize, iface, report_id, out_report)
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
    raw_report: *const u8, len: usize, iface: *mut c_void,
    report_id: u8, out: *mut u8,
) -> i32 {
    let mut src = raw_report;
    if device::hal_get_iface_uses_report_id(iface) {
        src = src.add(1);
    }

    let mod_offset = device::hal_get_kbd_modifier_offset_idx(iface, report_id) as usize;
    *out = *src.add(mod_offset); // modifier

    let mut j = 0usize;
    for i in 0..MAX_KEYS {
        if j >= KEYS_IN_USB_REPORT { break; }
        if device::hal_get_kbd_key_array(iface, report_id, i as i32) {
            *out.add(2 + j) = *src.add(i); // keycode[j]
            j += 1;
        }
    }

    KBD_REPORT_LENGTH as i32
}

unsafe fn extract_kbd_nkro(
    raw_report: *const u8, len: usize, iface: *mut c_void,
    report_id: u8, out: *mut u8,
) -> i32 {
    let usage_min = device::hal_get_kbd_nkro_usage_min(iface, report_id);
    let usage_max = device::hal_get_kbd_nkro_usage_max(iface, report_id);
    let nkro_size = device::hal_get_kbd_nkro_size(iface, report_id);

    if (usage_max - usage_min + 1) != nkro_size as i32 {
        return -1;
    }

    let mod_size = device::hal_get_kbd_modifier_size(iface, report_id);
    if mod_size != MODIFIER_BIT_LENGTH {
        return -1;
    }

    let mut ptr = raw_report;
    if device::hal_get_iface_uses_report_id(iface) {
        ptr = ptr.add(1);
    }

    let mod_offset = device::hal_get_kbd_modifier_offset_idx(iface, report_id) as usize;
    *out = *ptr.add(mod_offset); // modifier

    // Move to nkro offset
    let nkro_offset = device::hal_get_kbd_nkro_offset_idx(iface, report_id) as usize;
    let nkro_ptr = ptr.add(nkro_offset);
    let nkro_report = core::slice::from_raw_parts(nkro_ptr, core::cmp::min(len, 32));
    let keycode = core::slice::from_raw_parts_mut(out.add(2), KEYS_IN_USB_REPORT);

    crate::app::hid_report::extract_bit_variable(
        nkro_report, usage_min, usage_max, 0, keycode,
    ) as i32
}
