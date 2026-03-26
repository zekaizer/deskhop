// Keyboard data extraction — pure data transforms on HID reports.
// Converts raw keyboard reports into a normalized 8-byte format.

use crate::domain::structs::{HidInterface, KeyboardDescriptor, KBD_REPORT_LENGTH};
use crate::domain::hid_report;
const MAX_KEYS: usize = 32;
const KEYS_IN_USB_REPORT: usize = 6;
const MODIFIER_BIT_LENGTH: u16 = 8;
const HID_PROTOCOL_BOOT: u8 = 0;

/// Determine extraction strategy and dispatch accordingly.
/// `report` is the raw HID report (including report ID if present).
/// `out` receives the normalized 8-byte keyboard report.
/// Returns the number of bytes written, or 0/-1 on error.
pub fn extract_kbd_data(
    report: &[u8],
    iface: &HidInterface,
    kbd: &KeyboardDescriptor,
) -> ([u8; KBD_REPORT_LENGTH], i32) {
    let mut out = [0u8; KBD_REPORT_LENGTH];

    if report.len() < KBD_REPORT_LENGTH {
        return (out, 0);
    }

    if iface.protocol == HID_PROTOCOL_BOOT {
        let rc = extract_kbd_boot(report, &mut out);
        return (out, rc);
    }

    if kbd.is_nkro {
        let rc = extract_kbd_nkro(report, iface, kbd, &mut out);
        return (out, rc);
    }

    if !iface.uses_report_id
        && (report.len() == KBD_REPORT_LENGTH || report.len() == KBD_REPORT_LENGTH + 1)
    {
        let rc = extract_kbd_boot(report, &mut out);
        return (out, rc);
    }

    let rc = extract_kbd_other(report, iface, kbd, &mut out);
    (out, rc)
}

/// Boot protocol extraction — copy 8 bytes (skip report ID byte if len == 9).
fn extract_kbd_boot(report: &[u8], out: &mut [u8; KBD_REPORT_LENGTH]) -> i32 {
    let offset = if report.len() == KBD_REPORT_LENGTH + 1 { 1 } else { 0 };
    let src = &report[offset..];
    let copy_len = core::cmp::min(src.len(), KBD_REPORT_LENGTH);
    out[..copy_len].copy_from_slice(&src[..copy_len]);
    KBD_REPORT_LENGTH as i32
}

/// Standard keyboard extraction — map modifier + key_array positions.
fn extract_kbd_other(
    report: &[u8],
    iface: &HidInterface,
    kbd: &KeyboardDescriptor,
    out: &mut [u8; KBD_REPORT_LENGTH],
) -> i32 {
    let src = if iface.uses_report_id { &report[1..] } else { report };

    let mod_offset = kbd.modifier.offset_idx as usize;
    if mod_offset < src.len() {
        out[0] = src[mod_offset];
    }

    let mut j = 0usize;
    for i in 0..MAX_KEYS {
        if j >= KEYS_IN_USB_REPORT { break; }
        if kbd.key_array[i] && i < src.len() {
            out[2 + j] = src[i];
            j += 1;
        }
    }

    KBD_REPORT_LENGTH as i32
}

/// NKRO keyboard extraction — extract modifier + bitmap keys.
fn extract_kbd_nkro(
    report: &[u8],
    iface: &HidInterface,
    kbd: &KeyboardDescriptor,
    out: &mut [u8; KBD_REPORT_LENGTH],
) -> i32 {
    let usage_min = kbd.nkro.usage_min;
    let usage_max = kbd.nkro.usage_max;
    let nkro_size = kbd.nkro.size;

    if (usage_max - usage_min + 1) != nkro_size as i32 {
        return -1;
    }

    let mod_size = kbd.modifier.size;
    if mod_size != MODIFIER_BIT_LENGTH {
        return -1;
    }

    let src = if iface.uses_report_id { &report[1..] } else { report };

    let mod_offset = kbd.modifier.offset_idx as usize;
    if mod_offset < src.len() {
        out[0] = src[mod_offset];
    }

    let nkro_offset = kbd.nkro.offset_idx as usize;
    if nkro_offset >= src.len() {
        return -1;
    }
    let nkro_end = core::cmp::min(nkro_offset + 32, src.len());
    let nkro_report = &src[nkro_offset..nkro_end];

    hid_report::extract_bit_variable(
        nkro_report, usage_min, usage_max, 0, &mut out[2..2 + KEYS_IN_USB_REPORT],
    ) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::structs::*;
    use crate::domain::hid_parser::ReportVal;

    fn zeroed_iface() -> HidInterface {
        unsafe { core::mem::zeroed() }
    }

    #[test]
    fn test_boot_protocol_8bytes() {
        let iface = HidInterface { protocol: 0, ..zeroed_iface() };
        let kbd = KeyboardDescriptor::default();
        let report = [0x01, 0x00, 0x04, 0x05, 0x00, 0x00, 0x00, 0x00];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, KBD_REPORT_LENGTH as i32);
        assert_eq!(out, report);
    }

    #[test]
    fn test_boot_protocol_9bytes_skips_report_id() {
        let iface = HidInterface { protocol: 0, ..zeroed_iface() };
        let kbd = KeyboardDescriptor::default();
        let report = [0x01, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, KBD_REPORT_LENGTH as i32);
        // Should skip first byte (report ID)
        assert_eq!(out[0], 0x02);
    }

    #[test]
    fn test_short_report_returns_zero() {
        let iface = zeroed_iface();
        let kbd = KeyboardDescriptor::default();
        let report = [0u8; 4];
        let (_, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, 0);
    }
}
