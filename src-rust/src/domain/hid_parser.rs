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
// Must be packed to match C's TU_ATTR_PACKED report_val_t (23 bytes).
#[derive(Clone, Copy, Default)]
#[repr(C, packed)]
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

impl Default for ParserState {
    fn default() -> Self {
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
}

impl ParserState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset to the freshly-constructed state without materializing a
    /// temporary `ParserState` (~850B) on the stack — callers may keep this
    /// state in a static precisely because their stack cannot hold it.
    pub fn reset(&mut self) {
        self.report_id = 0;
        self.global_usage = 0;
        self.usage_count = 0;
        self.usages = [0; HID_MAX_USAGES];
        self.usage_ptr = 0;
        self.collection = Collection::default();
        self.report_offsets = [ReportOffset::default(); MAX_REPORTS];
        self.num_report_offsets = 0;
        self.globals = [Item::default(); 16];
        self.locals = [Item::default(); 16];
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

    /// Process a main INPUT item, writing the parsed ReportVals into `out`.
    /// Returns false when no report-offset slot is available (item dropped).
    /// Out-param instead of a by-value return: `ParsedInput` is ~380B and this
    /// runs on Core1's 2KB stack.
    fn handle_main_input(&mut self, item: &Item, out: &mut ParsedInput) -> bool {
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
            None => return false,
        };

        out.count = 0;
        out.uses_report_id = self.report_id != 0;

        for i in 0..(count as usize) {
            self.update_usage(i);
            let val = self.store_element(item.val, size, i);

            if out.count < 16 {
                out.vals[out.count] = val;
                out.count += 1;
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

        true
    }

    fn handle_main_item(&mut self, item: &Item, out: &mut ParsedInput) -> bool {
        let produced = match item.hdr.tag {
            RI_MAIN_COLLECTION => {
                self.collection.start += 1;
                false
            }
            RI_MAIN_COLLECTION_END => {
                self.collection.end += 1;
                false
            }
            RI_MAIN_INPUT => self.handle_main_input(item, out),
            _ => false,
        };

        self.usage_count = 0;
        self.locals = [Item::default(); 16];

        produced
    }
}

/// Result of parsing a main INPUT item — up to 16 report values
#[derive(Default)]
pub struct ParsedInput {
    pub vals: [ReportVal; 16],
    pub count: usize,
    pub uses_report_id: bool,
}

/// Parse a HID report descriptor, streaming each main INPUT item to `on_input`.
/// This is the pure-logic version that doesn't touch hid_interface_t directly.
///
/// Streaming (vs. returning accumulated results by value) is load-bearing: the
/// exported FFI wrapper runs inside the TinyUSB mount callback on Core1, whose
/// stack is only 2KB — this frame must stay at one `ParsedInput` (~380B).
/// `parser` is caller-provided for the same reason (it may live in a static).
pub fn parse_descriptor_with(
    report: &[u8],
    parser: &mut ParserState,
    mut on_input: impl FnMut(&ParsedInput),
) {
    let mut scratch = ParsedInput::default();
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

        let produced = match hdr.htype {
            RI_TYPE_MAIN => parser.handle_main_item(&item, &mut scratch),
            RI_TYPE_GLOBAL => {
                parser.handle_global_item(&item);
                false
            }
            RI_TYPE_LOCAL => {
                parser.handle_local_item(&item);
                false
            }
            _ => false,
        };
        if produced {
            on_input(&scratch);
        }

        pos += data_bytes;
    }
}

/// Accumulating convenience wrapper over [`parse_descriptor_with`] — returns
/// ~7KB by value, so it is for host-side tests only, never the firmware path.
pub fn parse_descriptor(report: &[u8]) -> (ParserState, alloc_free::ParseResults) {
    let mut parser = ParserState::new();
    let mut results = alloc_free::ParseResults::new();
    parse_descriptor_with(report, &mut parser, |input| results.push(input));
    (parser, results)
}

/// Alloc-free storage for parse results (no_std compatible)
pub mod alloc_free {
    use super::{ParsedInput, ReportVal};

    // Most HID descriptors have 3-8 INPUT items. 16 provides ample margin.
    const MAX_INPUTS: usize = 16;

    pub struct ParseResults {
        inputs: [Option<ParsedInputCompact>; MAX_INPUTS],
        count: usize,
    }

    pub struct ParsedInputCompact {
        pub vals: [ReportVal; 16],
        pub count: usize,
        pub uses_report_id: bool,
    }

    impl Default for ParseResults {
        fn default() -> Self {
            Self {
                inputs: [const { None }; MAX_INPUTS],
                count: 0,
            }
        }
    }

    impl ParseResults {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn push(&mut self, input: &ParsedInput) {
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

        pub fn is_empty(&self) -> bool {
            self.count == 0
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
        assert_eq!({ val.offset }, 0);
        assert_eq!({ val.size }, 0);
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
        assert_eq!({ btn_val.size }, 3);
        assert_eq!(btn_val.data_type, VARIABLE);
        assert_eq!({ btn_val.usage_page }, HID_USAGE_PAGE_BUTTON);
        assert_eq!({ btn_val.global_usage }, HID_USAGE_DESKTOP_MOUSE);

        // Third input: X and Y (8 bits each, 2 items)
        let input2 = results.iter().nth(2).unwrap();
        assert_eq!(input2.count, 2);
        let x_val = &input2.vals[0];
        assert_eq!({ x_val.size }, 8);
        assert_eq!({ x_val.usage }, HID_USAGE_DESKTOP_X);
        assert_eq!(x_val.data_type, VARIABLE);

        let y_val = &input2.vals[1];
        assert_eq!({ y_val.usage }, HID_USAGE_DESKTOP_Y);
        assert_eq!({ y_val.size }, 8);
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
    fn test_parse_5button_wheel_mouse() {
        // 5-button mouse with wheel (common Logitech-style)
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0x05, 0x01,       // Usage Page (Generic Desktop)
            0x09, 0x02,       // Usage (Mouse)
            0xA1, 0x01,       // Collection (Application)
            0x09, 0x01,       //   Usage (Pointer)
            0xA1, 0x00,       //   Collection (Physical)
            0x05, 0x09,       //     Usage Page (Button)
            0x19, 0x01,       //     Usage Minimum (1)
            0x29, 0x05,       //     Usage Maximum (5)
            0x15, 0x00,       //     Logical Minimum (0)
            0x25, 0x01,       //     Logical Maximum (1)
            0x95, 0x05,       //     Report Count (5)
            0x75, 0x01,       //     Report Size (1)
            0x81, 0x02,       //     Input (Data, Variable, Abs) -> 5 buttons
            0x95, 0x01,       //     Report Count (1)
            0x75, 0x03,       //     Report Size (3)
            0x81, 0x01,       //     Input (Constant) -> 3-bit padding
            0x05, 0x01,       //     Usage Page (Generic Desktop)
            0x09, 0x30,       //     Usage (X)
            0x09, 0x31,       //     Usage (Y)
            0x15, 0x81,       //     Logical Minimum (-127)
            0x25, 0x7F,       //     Logical Maximum (127)
            0x75, 0x08,       //     Report Size (8)
            0x95, 0x02,       //     Report Count (2)
            0x81, 0x06,       //     Input (Data, Variable, Relative) -> X, Y
            0x09, 0x38,       //     Usage (Wheel)
            0x15, 0x81,       //     Logical Minimum (-127)
            0x25, 0x7F,       //     Logical Maximum (127)
            0x75, 0x08,       //     Report Size (8)
            0x95, 0x01,       //     Report Count (1)
            0x81, 0x06,       //     Input (Data, Variable, Relative) -> Wheel
            0xC0,             //   End Collection (Physical)
            0xC0,             // End Collection (Application)
        ];

        let (parser, results) = parse_descriptor(desc);

        // 4 INPUT items: 5 buttons, 3-bit padding, X+Y, Wheel
        assert_eq!(results.len(), 4);

        // Buttons: 5 bits (size=1, count=5 -> swapped to size=5, count=1)
        let buttons = results.iter().next().unwrap();
        assert_eq!({ buttons.vals[0].size }, 5);
        assert_eq!({ buttons.vals[0].usage_page }, HID_USAGE_PAGE_BUTTON);

        // X+Y: 2 items of 8 bits
        let xy = results.iter().nth(2).unwrap();
        assert_eq!(xy.count, 2);
        assert_eq!({ xy.vals[0].usage }, HID_USAGE_DESKTOP_X);
        assert_eq!({ xy.vals[1].usage }, HID_USAGE_DESKTOP_Y);

        // Wheel: 1 item of 8 bits
        let wheel = results.iter().nth(3).unwrap();
        assert_eq!(wheel.count, 1);
        assert_eq!({ wheel.vals[0].usage }, HID_USAGE_DESKTOP_WHEEL);

        // Nested collections tracked
        assert_eq!(parser.collection.start, 2);
        assert_eq!(parser.collection.end, 2);
    }

    #[test]
    fn test_parse_keyboard_with_nkro() {
        // Simplified keyboard descriptor with NKRO bitmap
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0x05, 0x01,       // Usage Page (Generic Desktop)
            0x09, 0x06,       // Usage (Keyboard)
            0xA1, 0x01,       // Collection (Application)
            // Modifier keys (8 bits)
            0x05, 0x07,       //   Usage Page (Keyboard)
            0x19, 0xE0,       //   Usage Minimum (Left Control)
            0x29, 0xE7,       //   Usage Maximum (Right GUI)
            0x15, 0x00,       //   Logical Minimum (0)
            0x25, 0x01,       //   Logical Maximum (1)
            0x75, 0x01,       //   Report Size (1)
            0x95, 0x08,       //   Report Count (8)
            0x81, 0x02,       //   Input (Data, Variable, Absolute) -> modifiers
            // Reserved byte
            0x75, 0x08,       //   Report Size (8)
            0x95, 0x01,       //   Report Count (1)
            0x81, 0x01,       //   Input (Constant) -> reserved
            // Key array (6 keys)
            0x05, 0x07,       //   Usage Page (Keyboard)
            0x19, 0x00,       //   Usage Minimum (0)
            0x29, 0xFF,       //   Usage Maximum (255)
            0x15, 0x00,       //   Logical Minimum (0)
            0x26, 0xFF, 0x00, //   Logical Maximum (255)
            0x75, 0x08,       //   Report Size (8)
            0x95, 0x06,       //   Report Count (6)
            0x81, 0x00,       //   Input (Data, Array) -> keycodes
            0xC0,             // End Collection
        ];

        let (_parser, results) = parse_descriptor(desc);

        // 3 INPUT items: modifiers, reserved, key array
        assert_eq!(results.len(), 3);

        // Modifiers: 8 bits (1-bit × 8, swapped)
        let modifiers = results.iter().next().unwrap();
        assert_eq!({ modifiers.vals[0].size }, 8);
        assert_eq!(modifiers.vals[0].data_type, VARIABLE);
        assert_eq!({ modifiers.vals[0].usage_page }, HID_USAGE_PAGE_KEYBOARD);

        // Key array: 6 items, each 8 bits, type ARRAY
        let keys = results.iter().nth(2).unwrap();
        assert_eq!(keys.count, 6);
        assert_eq!({ keys.vals[0].size }, 8);
        assert_eq!(keys.vals[0].data_type, ARRAY);
    }

    #[test]
    fn test_parse_multiple_report_ids() {
        // Two reports with different IDs
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0xA1, 0x01,       // Collection
            0x85, 0x01,       //   Report ID (1)
            0x75, 0x08,       //   Report Size (8)
            0x95, 0x02,       //   Report Count (2)
            0x81, 0x00,       //   Input
            0x85, 0x02,       //   Report ID (2)
            0x75, 0x10,       //   Report Size (16)
            0x95, 0x01,       //   Report Count (1)
            0x81, 0x00,       //   Input
            0xC0,             // End Collection
        ];

        let (_parser, results) = parse_descriptor(desc);
        assert_eq!(results.len(), 2);

        let r1 = results.iter().next().unwrap();
        assert_eq!(r1.vals[0].report_id, 1);
        assert_eq!({ r1.vals[0].size }, 8);
        assert_eq!(r1.count, 2);

        let r2 = results.iter().nth(1).unwrap();
        assert_eq!(r2.vals[0].report_id, 2);
        assert_eq!({ r2.vals[0].size }, 16);
        assert_eq!(r2.count, 1);

        // Offsets should be independent per report ID
        assert_eq!({ r1.vals[0].offset }, 0);
        assert_eq!({ r2.vals[0].offset }, 0);
    }

    #[test]
    fn test_parse_descriptor_with_streams_same_items() {
        // Streaming entry point must deliver the same items the accumulating
        // wrapper stores (wrapper is itself built on the streaming core, so
        // assert against hand-computed expectations, not just each other).
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0xA1, 0x01,       // Collection
            0x85, 0x01,       //   Report ID (1)
            0x75, 0x08,       //   Report Size (8)
            0x95, 0x02,       //   Report Count (2)
            0x81, 0x00,       //   Input -> 2 items of 8 bits
            0x85, 0x02,       //   Report ID (2)
            0x75, 0x10,       //   Report Size (16)
            0x95, 0x01,       //   Report Count (1)
            0x81, 0x00,       //   Input -> 1 item of 16 bits
            0xC0,             // End Collection
        ];

        let mut parser = ParserState::new();
        let mut seen = [(0usize, 0u16, 0u8, false); 4];
        let mut n = 0;
        parse_descriptor_with(desc, &mut parser, |input| {
            seen[n] = (
                input.count,
                { input.vals[0].size },
                input.vals[0].report_id,
                input.uses_report_id,
            );
            n += 1;
        });

        assert_eq!(n, 2);
        assert_eq!(seen[0], (2, 8, 1, true));
        assert_eq!(seen[1], (1, 16, 2, true));
        assert_eq!(parser.collection.start, 1);
        assert_eq!(parser.collection.end, 1);
    }

    #[test]
    fn test_parser_state_reset_equals_fresh() {
        let mut parser = ParserState::new();
        let mut sink = |_: &ParsedInput| {};
        parse_descriptor_with(&[0xA1, 0x01, 0x85, 0x07, 0xC0], &mut parser, &mut sink);
        assert_eq!(parser.report_id, 7);
        assert_eq!(parser.collection.start, 1);

        parser.reset();
        assert_eq!(parser.report_id, 0);
        assert_eq!(parser.collection.start, 0);
        assert_eq!(parser.num_report_offsets, 0);

        // A reset parser must parse identically to a fresh one
        let (_, results) = parse_descriptor(&[0x75, 0x08, 0x95, 0x01, 0x81, 0x00]);
        let mut count_after_reset = 0;
        parse_descriptor_with(&[0x75, 0x08, 0x95, 0x01, 0x81, 0x00], &mut parser, |_| {
            count_after_reset += 1;
        });
        assert_eq!(count_after_reset, results.len());
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
