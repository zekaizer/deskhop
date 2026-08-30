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
        nkro_report,
        usage_min,
        usage_max,
        kbd.nkro.offset,
        &mut out[2..2 + KEYS_IN_USB_REPORT],
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

    #[test]
    fn test_extract_boot_protocol_full_report() {
        // Standard 8-byte boot protocol report with all fields populated
        let iface = HidInterface { protocol: 0, ..zeroed_iface() };
        let kbd = KeyboardDescriptor::default();
        let report = [0xFF, 0x00, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, KBD_REPORT_LENGTH as i32);
        assert_eq!(out[0], 0xFF); // modifier
        assert_eq!(out[1], 0x00); // reserved
        assert_eq!(out[2], 0x04); // keycode[0] = A
        assert_eq!(out[3], 0x05); // keycode[1] = B
        assert_eq!(out[4], 0x06); // keycode[2] = C
        assert_eq!(out[5], 0x07); // keycode[3] = D
        assert_eq!(out[6], 0x08); // keycode[4] = E
        assert_eq!(out[7], 0x09); // keycode[5] = F
    }

    #[test]
    fn test_extract_boot_protocol_with_report_id() {
        // 9-byte report (report ID prefix) — boot protocol skips first byte
        let iface = HidInterface { protocol: 0, ..zeroed_iface() };
        let kbd = KeyboardDescriptor::default();
        let report = [0x01, 0x03, 0x00, 0x04, 0x05, 0x00, 0x00, 0x00, 0x00];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, KBD_REPORT_LENGTH as i32);
        assert_eq!(out[0], 0x03); // modifier (was at index 1)
        assert_eq!(out[2], 0x04); // first keycode
        assert_eq!(out[3], 0x05); // second keycode
    }

    #[test]
    fn test_extract_nkro_basic() {
        // Non-boot, NKRO keyboard with bitmap-based key extraction
        let mut iface = zeroed_iface();
        iface.protocol = 1; // not boot
        iface.uses_report_id = false;

        let mut kbd = KeyboardDescriptor::default();
        kbd.is_nkro = true;
        kbd.modifier = ReportVal {
            offset: 0,
            offset_idx: 0, // modifier at byte 0
            size: 8,       // MODIFIER_BIT_LENGTH
            usage_min: 0,
            usage_max: 0,
            ..ReportVal::default()
        };
        kbd.nkro = ReportVal {
            offset: 0,
            offset_idx: 1,    // NKRO bitmap starts at byte 1
            size: 8,          // 8 bits = 8 usage codes
            usage_min: 0,
            usage_max: 7,     // (7 - 0 + 1) == 8 == size
            ..ReportVal::default()
        };

        // Report: modifier=0x01, then NKRO bitmap byte
        // Bitmap 0b00010100 = bits 2 and 4 set → usage codes 2, 4
        let report = [0x01, 0b00010100, 0, 0, 0, 0, 0, 0];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert!(rc > 0);
        assert_eq!(out[0], 0x01); // modifier preserved
        // Keys extracted from bitmap
        assert_eq!(out[2], 2); // usage_min + bit 2
        assert_eq!(out[3], 4); // usage_min + bit 4
    }

    #[test]
    fn test_extract_nkro_midbyte_bit_offset() {
        // NKRO bitmap starting mid-byte (offset 12 = byte 1, bit 4): the bit
        // offset within the first bitmap byte must come from nkro.offset & 7
        // (C behaviour) — a hardcoded 0 shifts every extracted usage.
        let mut iface = zeroed_iface();
        iface.protocol = 1;
        iface.uses_report_id = false;

        let mut kbd = KeyboardDescriptor::default();
        kbd.is_nkro = true;
        kbd.modifier = ReportVal {
            offset: 0,
            offset_idx: 0,
            size: 8,
            ..ReportVal::default()
        };
        kbd.nkro = ReportVal {
            offset: 12,     // bit offset: byte 1, bit 4
            offset_idx: 1,  // bitmap bytes start at src[1]
            size: 4,
            usage_min: 4,
            usage_max: 7,   // span 4 == size
            ..ReportVal::default()
        };

        // src[1] bits 4 and 6 set → usages 4 and 6
        let report = [0x01, 0b0101_0000, 0, 0, 0, 0, 0, 0];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, 2);
        assert_eq!(out[0], 0x01);
        assert_eq!(out[2], 4);
        assert_eq!(out[3], 6);
    }

    #[test]
    fn test_extract_standard_keycode_mapping() {
        // Non-boot, non-NKRO, uses_report_id=true → extract_kbd_other path
        let mut iface = zeroed_iface();
        iface.protocol = 1;        // not boot
        iface.uses_report_id = true;

        let mut kbd = KeyboardDescriptor::default();
        kbd.is_nkro = false;
        kbd.modifier = ReportVal {
            offset: 0,
            offset_idx: 0, // modifier at byte 0 (after report ID skip)
            ..ReportVal::default()
        };
        // Mark key_array positions that hold keycodes
        kbd.key_array[2] = true; // byte 2 is a key
        kbd.key_array[3] = true; // byte 3 is a key

        // Report: [report_id, modifier, reserved, key1, key2, ...]
        let report = [0x01, 0x02, 0x00, 0x04, 0x05, 0x00, 0x00, 0x00, 0x00];
        let (out, rc) = extract_kbd_data(&report, &iface, &kbd);
        assert_eq!(rc, KBD_REPORT_LENGTH as i32);
        assert_eq!(out[0], 0x02); // modifier from byte 0 of payload
        assert_eq!(out[2], 0x04); // key from key_array[2]
        assert_eq!(out[3], 0x05); // key from key_array[3]
    }
}
