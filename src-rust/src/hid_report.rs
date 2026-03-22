/// Extract a value from a HID report given bit offset and bit size.
/// Handles cross-byte boundaries and sign extension.
pub fn get_report_value(report: &[u8], offset_bits: u16, size_bits: u16) -> i32 {
    if size_bits == 0 {
        return 0;
    }

    let byte_offset = (offset_bits >> 3) as usize;
    let offset_in_bits = offset_bits & 7;

    if byte_offset >= report.len() {
        return 0;
    }

    let mask: u32 = if size_bits >= 32 {
        0xFFFF_FFFF
    } else {
        (1u32 << size_bits) - 1
    };

    let mut result: i32 = (report[byte_offset] >> offset_in_bits) as i32;
    let mut remaining_bits: u16 = 8 - offset_in_bits;
    let mut idx = byte_offset;

    while size_bits > remaining_bits && idx < report.len() - 1 {
        idx += 1;
        result |= (report[idx] as i32) << remaining_bits;
        remaining_bits += 8;
    }

    result &= mask as i32;

    // WORKAROUND(c-compat): Sign extension matches C's get_report_value() exactly.
    // 1-bit value with bit set => -1 (not +1). This is correct per HID spec
    // for relative values but surprising for buttons. Revisit when decoupled from C.
    if size_bits < 32 {
        let sign_bit = 1u32 << (size_bits - 1);
        if (result as u32) & sign_bit != 0 {
            result |= (0xFFFF_FFFFu32 << size_bits) as i32;
        }
    }

    result
}

/// Extract key codes from a bit-variable NKRO report.
/// Returns the number of keys found.
pub fn extract_bit_variable(
    raw_report: &[u8],
    usage_min: i32,
    usage_max: i32,
    bit_offset: u16,
    dst: &mut [u8],
) -> usize {
    let mut key_count = 0;
    let start_bit = (bit_offset & 0b111) as usize;

    let mut j = start_bit;
    for i in usage_min..=usage_max {
        if key_count >= dst.len() {
            break;
        }

        let byte_index = j >> 3;
        let bit_index = j & 0b111;

        if byte_index < raw_report.len() && (raw_report[byte_index] & (1 << bit_index)) != 0 {
            dst[key_count] = i as u8;
            key_count += 1;
        }

        j += 1;
    }

    key_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_report_value_byte_aligned() {
        let report = [0x00, 0x42, 0xFF];
        // 8 bits at offset 8 (second byte)
        assert_eq!(get_report_value(&report, 8, 8), 0x42);
    }

    #[test]
    fn test_get_report_value_cross_byte() {
        // 0xAB = 10101011, 0xCD = 11001101
        let report = [0xAB, 0xCD];
        // 4 bits at offset 4: should get upper nibble of first + lower nibble of second
        // bits 4..7 of 0xAB = 1010, bits 0..3 of 0xCD = 1101 -> not what we want
        // Actually: offset=4, size=8 -> bits [4:11]
        // 0xAB >> 4 = 0x0A, 0xCD << 4 = 0xD0, result = 0xDA
        assert_eq!(get_report_value(&report, 4, 8) as u8, 0xDA);
    }

    #[test]
    fn test_get_report_value_single_bit() {
        let report = [0b00000100]; // bit 2 set
        // 1-bit value with MSB set => sign-extended to -1 (matches C behavior)
        assert_eq!(get_report_value(&report, 2, 1), -1);
        // bit 0 not set => 0
        assert_eq!(get_report_value(&report, 0, 1), 0);
    }

    #[test]
    fn test_get_report_value_signed() {
        // -1 in 8-bit = 0xFF
        let report = [0xFF];
        assert_eq!(get_report_value(&report, 0, 8), -1);
    }

    #[test]
    fn test_get_report_value_out_of_bounds() {
        let report = [0x42];
        assert_eq!(get_report_value(&report, 16, 8), 0);
    }

    #[test]
    fn test_extract_bit_variable_basic() {
        // Bits: 00000101 = keys at position 0 and 2 (usage_min + 0, usage_min + 2)
        let report = [0b00000101];
        let mut dst = [0u8; 6];
        let count = extract_bit_variable(&report, 0, 7, 0, &mut dst);
        assert_eq!(count, 2);
        assert_eq!(dst[0], 0); // usage_min + 0
        assert_eq!(dst[1], 2); // usage_min + 2
    }

    #[test]
    fn test_extract_bit_variable_with_offset() {
        // usage_min = 0xE0 (LEFT_CTRL), bits set at positions 0 and 1
        let report = [0b00000011];
        let mut dst = [0u8; 6];
        let count = extract_bit_variable(&report, 0xE0, 0xE7, 0, &mut dst);
        assert_eq!(count, 2);
        assert_eq!(dst[0], 0xE0);
        assert_eq!(dst[1], 0xE1);
    }

    #[test]
    fn test_extract_bit_variable_dst_limit() {
        // All bits set but dst can only hold 2
        let report = [0xFF];
        let mut dst = [0u8; 2];
        let count = extract_bit_variable(&report, 0, 7, 0, &mut dst);
        assert_eq!(count, 2);
    }
}
