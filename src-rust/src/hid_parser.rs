/// HID descriptor item header (1 byte, unpacked into size/type/tag)
#[derive(Debug, Clone, Copy, Default)]
pub struct Header {
    pub size: u8, // 2 bits
    pub htype: u8, // 2 bits (type is reserved keyword)
    pub tag: u8,  // 4 bits
}

impl Header {
    /// Parse a header byte into its components
    pub fn from_byte(byte: u8) -> Self {
        Self {
            size: byte & 0x03,
            htype: (byte >> 2) & 0x03,
            tag: (byte >> 4) & 0x0F,
        }
    }
}

/// Size lookup: descriptor size field (0-3) to actual byte count
const SIZE_LOOKUP: [u8; 4] = [0, 1, 2, 4];

pub fn size_bytes(size_field: u8) -> u8 {
    SIZE_LOOKUP[(size_field & 0x03) as usize]
}

/// Extract a value from descriptor data based on size field
pub fn get_descriptor_value(data: &[u8], size: u8) -> u32 {
    match size {
        1 => {
            if data.is_empty() { 0 } else { data[0] as u32 }
        }
        2 => {
            if data.len() < 2 { 0 } else { u16::from_le_bytes([data[0], data[1]]) as u32 }
        }
        3 => {
            if data.len() < 4 { 0 } else {
                u32::from_le_bytes([data[0], data[1], data[2], data[3]])
            }
        }
        _ => 0,
    }
}

/// HID Report Item types
pub const RI_TYPE_MAIN: u8 = 0;
pub const RI_TYPE_GLOBAL: u8 = 1;
pub const RI_TYPE_LOCAL: u8 = 2;

/// Main item tags
pub const RI_MAIN_INPUT: u8 = 8;
pub const RI_MAIN_OUTPUT: u8 = 9;
pub const RI_MAIN_COLLECTION: u8 = 10;
pub const RI_MAIN_COLLECTION_END: u8 = 12;

/// Global item tags
pub const RI_GLOBAL_USAGE_PAGE: u8 = 0;
pub const RI_GLOBAL_LOGICAL_MIN: u8 = 1;
pub const RI_GLOBAL_LOGICAL_MAX: u8 = 2;
pub const RI_GLOBAL_REPORT_SIZE: u8 = 7;
pub const RI_GLOBAL_REPORT_ID: u8 = 8;
pub const RI_GLOBAL_REPORT_COUNT: u8 = 9;

/// Local item tags
pub const RI_LOCAL_USAGE: u8 = 0;
pub const RI_LOCAL_USAGE_MIN: u8 = 1;
pub const RI_LOCAL_USAGE_MAX: u8 = 2;

/// HID Usage Pages
pub const HID_USAGE_PAGE_DESKTOP: u16 = 0x01;
pub const HID_USAGE_PAGE_KEYBOARD: u16 = 0x07;
pub const HID_USAGE_PAGE_BUTTON: u16 = 0x09;
pub const HID_USAGE_PAGE_CONSUMER: u16 = 0x0C;

/// HID Desktop Usages
pub const HID_USAGE_DESKTOP_X: u16 = 0x30;
pub const HID_USAGE_DESKTOP_Y: u16 = 0x31;
pub const HID_USAGE_DESKTOP_WHEEL: u16 = 0x38;
pub const HID_USAGE_DESKTOP_MOUSE: u16 = 0x02;
pub const HID_USAGE_DESKTOP_KEYBOARD: u16 = 0x06;

/// Parsed report value descriptor — describes where a value lives in a HID report
#[derive(Debug, Clone, Copy, Default)]
pub struct ReportVal {
    pub offset: u16,      // In bits
    pub offset_idx: u16,  // In bytes
    pub size: u16,        // In bits
    pub usage_min: i32,
    pub usage_max: i32,
    pub item_type: u8,    // DATA or CONSTANT
    pub data_type: u8,    // VARIABLE or ARRAY
    pub report_id: u8,
    pub global_usage: u16,
    pub usage_page: u16,
    pub usage: u16,
}

/// Data type constants
pub const DATA: u8 = 0;
pub const CONSTANT: u8 = 1;
pub const ARRAY: u8 = 2;
pub const VARIABLE: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_from_byte() {
        // 0x05 = 0000_01_01 -> size=1, type=1, tag=0
        let h = Header::from_byte(0x05);
        assert_eq!(h.size, 1);
        assert_eq!(h.htype, 1);
        assert_eq!(h.tag, 0);

        // 0xA1 = 1010_00_01 -> size=1, type=0, tag=10
        let h2 = Header::from_byte(0xA1);
        assert_eq!(h2.size, 1);
        assert_eq!(h2.htype, 0);
        assert_eq!(h2.tag, 10);
    }

    #[test]
    fn test_size_lookup() {
        assert_eq!(size_bytes(0), 0);
        assert_eq!(size_bytes(1), 1);
        assert_eq!(size_bytes(2), 2);
        assert_eq!(size_bytes(3), 4);
    }

    #[test]
    fn test_get_descriptor_value_8bit() {
        assert_eq!(get_descriptor_value(&[0x42], 1), 0x42);
    }

    #[test]
    fn test_get_descriptor_value_16bit() {
        // Little-endian: 0x34, 0x12 => 0x1234
        assert_eq!(get_descriptor_value(&[0x34, 0x12], 2), 0x1234);
    }

    #[test]
    fn test_get_descriptor_value_32bit() {
        assert_eq!(
            get_descriptor_value(&[0x78, 0x56, 0x34, 0x12], 3),
            0x12345678
        );
    }

    #[test]
    fn test_get_descriptor_value_zero() {
        assert_eq!(get_descriptor_value(&[], 0), 0);
    }

    #[test]
    fn test_get_descriptor_value_short_buffer() {
        // 16-bit requested but only 1 byte available
        assert_eq!(get_descriptor_value(&[0x42], 2), 0);
    }

    #[test]
    fn test_report_val_default() {
        let val = ReportVal::default();
        assert_eq!(val.offset, 0);
        assert_eq!(val.size, 0);
        assert_eq!(val.report_id, 0);
    }
}
