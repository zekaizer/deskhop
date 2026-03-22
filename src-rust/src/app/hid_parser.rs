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

/// Consumer usage
pub const HID_USAGE_CONSUMER_CONTROL: u16 = 0x01;
pub const HID_USAGE_CONSUMER_AC_PAN: u16 = 0x238;
pub const HID_USAGE_DESKTOP_SYSTEM_CONTROL: u16 = 0x80;

const HID_MAX_USAGES: usize = 128;
const MAX_REPORTS: usize = 24;

/// A parsed item: header + value
#[derive(Debug, Clone, Copy, Default)]
pub struct Item {
    pub hdr: Header,
    pub val: u32,
}

/// Collection nesting tracker
#[derive(Debug, Clone, Copy, Default)]
pub struct Collection {
    pub start: u8,
    pub end: u8,
}

/// Report offset tracker — maps report_id to current bit offset
#[derive(Debug, Clone, Copy, Default)]
pub struct ReportOffset {
    pub report_id: u8,
    pub offset_in_bits: u32,
}

/// Full parser state
pub struct ParserState {
    pub report_id: u8,
    pub global_usage: u16,
    pub usage_count: usize,
    pub usages: [u16; HID_MAX_USAGES],
    pub usage_ptr: usize, // index into usages array (replaces p_usage pointer)
    pub collection: Collection,
    pub report_offsets: [ReportOffset; MAX_REPORTS],
    pub num_report_offsets: usize,
    pub globals: [Item; 16],
    pub locals: [Item; 16],
}

impl ParserState {
    pub fn new() -> Self {
        Self {
            report_id: 0,
            global_usage: 0,
            usage_count: 0,
            usages: [0; HID_MAX_USAGES],
            usage_ptr: 0,
            collection: Collection::default(),
            report_offsets: [ReportOffset::default(); MAX_REPORTS],
            num_report_offsets: 0,
            globals: [Item::default(); 16],
            locals: [Item::default(); 16],
        }
    }

    fn is_block_end(&self) -> bool {
        self.collection.start == self.collection.end
    }

    fn get_or_create_report_offset(&mut self, report_id: u8) -> Option<usize> {
        for i in 0..self.num_report_offsets {
            if self.report_offsets[i].report_id == report_id {
                return Some(i);
            }
        }

        if self.num_report_offsets < MAX_REPORTS {
            let idx = self.num_report_offsets;
            self.report_offsets[idx].report_id = report_id;
            self.report_offsets[idx].offset_in_bits = 0;
            self.num_report_offsets += 1;
            Some(idx)
        } else {
            None
        }
    }

    fn get_current_offset(&mut self) -> u32 {
        let rid = self.report_id;
        match self.get_or_create_report_offset(rid) {
            Some(idx) => self.report_offsets[idx].offset_in_bits,
            None => 0,
        }
    }

    fn update_usage(&mut self, i: usize) {
        let ptr = self.usage_ptr;
        if i > 0 && i >= self.usage_count && (ptr + i) < HID_MAX_USAGES {
            self.usages[ptr + i] = self.usages[ptr + i - 1];
        }
    }

    fn store_element(&mut self, item_val: u32, size: u32, i: usize) -> ReportVal {
        let current_offset = self.get_current_offset();
        let ptr = self.usage_ptr;
        let usage = if ptr + i < HID_MAX_USAGES {
            self.usages[ptr + i]
        } else {
            0
        };

        ReportVal {
            offset: current_offset as u16,
            offset_idx: (current_offset >> 3) as u16,
            size: size as u16,
            usage_max: self.locals[RI_LOCAL_USAGE_MAX as usize].val as i32,
            usage_min: self.locals[RI_LOCAL_USAGE_MIN as usize].val as i32,
            item_type: if item_val & 0x01 != 0 { CONSTANT } else { DATA },
            data_type: if item_val & 0x02 != 0 { VARIABLE } else { ARRAY },
            usage,
            usage_page: self.globals[RI_GLOBAL_USAGE_PAGE as usize].val as u16,
            global_usage: self.global_usage,
            report_id: self.report_id,
        }
    }

    fn handle_global_item(&mut self, item: &Item) {
        if item.hdr.tag == RI_GLOBAL_REPORT_ID {
            self.report_id = item.val as u8;
        }
        self.globals[item.hdr.tag as usize] = *item;
    }

    fn handle_local_item(&mut self, item: &Item) {
        self.locals[item.hdr.tag as usize] = *item;

        if item.hdr.tag == RI_LOCAL_USAGE {
            if self.is_block_end() {
                self.global_usage = item.val as u16;
            } else if self.usage_count < HID_MAX_USAGES - 1 {
                let idx = self.usage_ptr + self.usage_count;
                if idx < HID_MAX_USAGES {
                    self.usages[idx] = item.val as u16;
                    self.usage_count += 1;
                }
            }
        }
    }

    /// Process a main INPUT item. Returns the parsed ReportVals for the caller
    /// to use for populating interface structures.
    fn handle_main_input(&mut self, item: &Item) -> ParsedInput {
        let mut size = self.globals[RI_GLOBAL_REPORT_SIZE as usize].val;
        let mut count = self.globals[RI_GLOBAL_REPORT_COUNT as usize].val;

        // Swap count and size for 1-bit variables (e.g. NKRO bitmaps)
        if size == 1 && self.usage_count <= 1 {
            size = count;
            count = 1;
        }

        let rid = self.report_id;
        let offset_idx = match self.get_or_create_report_offset(rid) {
            Some(idx) => idx,
            None => return ParsedInput { vals: [ReportVal::default(); 16], count: 0, uses_report_id: false },
        };

        let mut result = ParsedInput {
            vals: [ReportVal::default(); 16],
            count: 0,
            uses_report_id: self.report_id != 0,
        };

        for i in 0..(count as usize) {
            self.update_usage(i);
            let val = self.store_element(item.val, size, i);

            if result.count < 16 {
                result.vals[result.count] = val;
                result.count += 1;
            }

            self.report_offsets[offset_idx].offset_in_bits += size;
        }

        // Advance usage pointer
        let old_count = self.usage_count;
        self.usage_ptr += old_count;

        // Carry the last usage to the new location
        if old_count > 0 && self.usage_ptr < HID_MAX_USAGES {
            let carry = self.usages[self.usage_ptr - old_count];
            // Avoid out-of-bounds, but C code doesn't check
            if self.usage_ptr < HID_MAX_USAGES {
                self.usages[self.usage_ptr] = carry;
            }
        }

        result
    }

    fn handle_main_item(&mut self, item: &Item) -> Option<ParsedInput> {
        let result = match item.hdr.tag {
            RI_MAIN_COLLECTION => {
                self.collection.start += 1;
                None
            }
            RI_MAIN_COLLECTION_END => {
                self.collection.end += 1;
                None
            }
            RI_MAIN_INPUT => Some(self.handle_main_input(item)),
            _ => None,
        };

        self.usage_count = 0;
        self.locals = [Item::default(); 16];

        result
    }
}

/// Result of parsing a main INPUT item — up to 16 report values
pub struct ParsedInput {
    pub vals: [ReportVal; 16],
    pub count: usize,
    pub uses_report_id: bool,
}

/// Parse a HID report descriptor and return all extracted ReportVals.
/// This is the pure-logic version that doesn't touch hid_interface_t directly.
/// The caller is responsible for populating interface structures from the results.
pub fn parse_descriptor(report: &[u8]) -> (ParserState, alloc_free::ParseResults) {
    let mut parser = ParserState::new();
    let mut results = alloc_free::ParseResults::new();
    let mut pos = 0;

    while pos < report.len() {
        let hdr = Header::from_byte(report[pos]);
        pos += 1;

        let data_bytes = size_bytes(hdr.size) as usize;
        let val = if pos + data_bytes <= report.len() {
            get_descriptor_value(&report[pos..], hdr.size)
        } else {
            0
        };

        let item = Item { hdr, val };

        match hdr.htype {
            RI_TYPE_MAIN => {
                if let Some(parsed) = parser.handle_main_item(&item) {
                    results.push(parsed);
                }
            }
            RI_TYPE_GLOBAL => parser.handle_global_item(&item),
            RI_TYPE_LOCAL => parser.handle_local_item(&item),
            _ => {}
        }

        pos += data_bytes;
    }

    (parser, results)
}

/// Alloc-free storage for parse results (no_std compatible)
pub mod alloc_free {
    use super::{ParsedInput, ReportVal};

    const MAX_INPUTS: usize = 32;

    pub struct ParseResults {
        inputs: [Option<ParsedInputCompact>; MAX_INPUTS],
        count: usize,
    }

    pub struct ParsedInputCompact {
        pub vals: [ReportVal; 16],
        pub count: usize,
        pub uses_report_id: bool,
    }

    impl ParseResults {
        pub fn new() -> Self {
            Self {
                inputs: [const { None }; MAX_INPUTS],
                count: 0,
            }
        }

        pub fn push(&mut self, input: ParsedInput) {
            if self.count < MAX_INPUTS {
                self.inputs[self.count] = Some(ParsedInputCompact {
                    vals: input.vals,
                    count: input.count,
                    uses_report_id: input.uses_report_id,
                });
                self.count += 1;
            }
        }

        pub fn iter(&self) -> impl Iterator<Item = &ParsedInputCompact> {
            self.inputs[..self.count].iter().filter_map(|x| x.as_ref())
        }

        pub fn len(&self) -> usize {
            self.count
        }
    }
}

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

    #[test]
    fn test_parser_state_new() {
        let parser = ParserState::new();
        assert_eq!(parser.report_id, 0);
        assert_eq!(parser.usage_count, 0);
        assert_eq!(parser.num_report_offsets, 0);
    }

    #[test]
    fn test_parse_simple_mouse_descriptor() {
        // Simplified HID descriptor for a 3-button mouse with X, Y
        // Usage Page (Generic Desktop)
        // Usage (Mouse)
        // Collection (Application)
        //   Usage Page (Button)
        //   Usage Minimum (1)
        //   Usage Maximum (3)
        //   Logical Minimum (0)
        //   Logical Maximum (1)
        //   Report Count (3)
        //   Report Size (1)
        //   Input (Data, Variable, Absolute)  -- 3 buttons
        //   Report Count (1)
        //   Report Size (5)
        //   Input (Constant)                  -- 5-bit padding
        //   Usage Page (Generic Desktop)
        //   Usage (X)
        //   Usage (Y)
        //   Logical Minimum (-127)
        //   Logical Maximum (127)
        //   Report Size (8)
        //   Report Count (2)
        //   Input (Data, Variable, Relative)  -- X, Y
        // End Collection
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0x05, 0x01,       // Usage Page (Generic Desktop)
            0x09, 0x02,       // Usage (Mouse)
            0xA1, 0x01,       // Collection (Application)
            0x05, 0x09,       //   Usage Page (Button)
            0x19, 0x01,       //   Usage Minimum (1)
            0x29, 0x03,       //   Usage Maximum (3)
            0x15, 0x00,       //   Logical Minimum (0)
            0x25, 0x01,       //   Logical Maximum (1)
            0x95, 0x03,       //   Report Count (3)
            0x75, 0x01,       //   Report Size (1)
            0x81, 0x02,       //   Input (Data, Variable, Absolute) -> buttons
            0x95, 0x01,       //   Report Count (1)
            0x75, 0x05,       //   Report Size (5)
            0x81, 0x01,       //   Input (Constant) -> padding
            0x05, 0x01,       //   Usage Page (Generic Desktop)
            0x09, 0x30,       //   Usage (X)
            0x09, 0x31,       //   Usage (Y)
            0x15, 0x81,       //   Logical Minimum (-127)
            0x25, 0x7F,       //   Logical Maximum (127)
            0x75, 0x08,       //   Report Size (8)
            0x95, 0x02,       //   Report Count (2)
            0x81, 0x06,       //   Input (Data, Variable, Relative) -> X, Y
            0xC0,             // End Collection
        ];

        let (_parser, results) = parse_descriptor(desc);

        // Should have parsed 3 main INPUT items: buttons, padding, X+Y
        assert_eq!(results.len(), 3);

        // First input: buttons (3 bits, variable)
        let input0 = results.iter().next().unwrap();
        assert_eq!(input0.count, 1); // size=1, count=3 -> swapped to size=3, count=1
        let btn_val = &input0.vals[0];
        assert_eq!(btn_val.size, 3);
        assert_eq!(btn_val.data_type, VARIABLE);
        assert_eq!(btn_val.usage_page, HID_USAGE_PAGE_BUTTON);
        assert_eq!(btn_val.global_usage, HID_USAGE_DESKTOP_MOUSE);

        // Third input: X and Y (8 bits each, 2 items)
        let input2 = results.iter().nth(2).unwrap();
        assert_eq!(input2.count, 2);
        let x_val = &input2.vals[0];
        assert_eq!(x_val.size, 8);
        assert_eq!(x_val.usage, HID_USAGE_DESKTOP_X);
        assert_eq!(x_val.data_type, VARIABLE);

        let y_val = &input2.vals[1];
        assert_eq!(y_val.usage, HID_USAGE_DESKTOP_Y);
        assert_eq!(y_val.size, 8);
    }

    #[test]
    fn test_parse_empty_descriptor() {
        let (_parser, results) = parse_descriptor(&[]);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_collection_tracking() {
        // Collection start + end
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0xA1, 0x01,  // Collection (Application)
            0xC0,        // End Collection
        ];

        let (parser, _) = parse_descriptor(desc);
        assert_eq!(parser.collection.start, 1);
        assert_eq!(parser.collection.end, 1);
    }

    #[test]
    fn test_report_id_tracking() {
        // Report ID (1)
        // Report Size (8)
        // Report Count (1)
        // Input (Data)
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0xA1, 0x01,  // Collection
            0x85, 0x01,  // Report ID (1)
            0x75, 0x08,  // Report Size (8)
            0x95, 0x01,  // Report Count (1)
            0x81, 0x00,  // Input (Data)
            0xC0,        // End Collection
        ];

        let (_parser, results) = parse_descriptor(desc);
        assert_eq!(results.len(), 1);
        let input = results.iter().next().unwrap();
        assert!(input.uses_report_id);
        assert_eq!(input.vals[0].report_id, 1);
    }
}
