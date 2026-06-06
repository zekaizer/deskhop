// HID report descriptor decoder for readable debug dumps. Pure logic: walks the
// short-item structure (HID 1.11 §6.2.2) and yields one decoded `Item` at a
// time via a callback, with the current collection-nesting depth. Formatting
// and usage-name lookup live in the caller (so this stays no_std + testable).

/// Decoded item kind. Items we don't specifically name map to `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    UsagePage,
    Usage,
    UsageMin,
    UsageMax,
    Collection,
    EndCollection,
    ReportId,
    ReportSize,
    ReportCount,
    LogicalMin,
    LogicalMax,
    Input,
    Output,
    Feature,
    Other,
}

/// One decoded descriptor item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    /// Item data, little-endian (0 for items with no data, e.g. EndCollection).
    pub value: u32,
    /// Collection nesting depth at this item (for indentation). Collection is
    /// reported at the depth it opens; EndCollection at the depth it closes to.
    pub depth: u8,
}

/// Decode a HID report descriptor, calling `emit` once per short item.
/// Long items (prefix 0xFE) and trailing truncated items are skipped safely.
pub fn decode(desc: &[u8], mut emit: impl FnMut(Item)) {
    let mut i = 0usize;
    let mut depth: u8 = 0;
    while i < desc.len() {
        let prefix = desc[i];
        i += 1;
        if prefix == 0xFE {
            // Long item: [0xFE][dataSize][tag][data...]
            if i >= desc.len() {
                break;
            }
            let data_size = desc[i] as usize;
            i = i.saturating_add(1 + 1 + data_size); // dataSize + tag + data
            continue;
        }
        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let b_type = (prefix >> 2) & 0x03;
        let b_tag = (prefix >> 4) & 0x0F;

        let mut value: u32 = 0;
        for k in 0..size {
            if i + k < desc.len() {
                value |= (desc[i + k] as u32) << (8 * k);
            }
        }
        i = i.saturating_add(size);

        let kind = match (b_type, b_tag) {
            // Main
            (0, 0x8) => Kind::Input,
            (0, 0x9) => Kind::Output,
            (0, 0xB) => Kind::Feature,
            (0, 0xA) => Kind::Collection,
            (0, 0xC) => Kind::EndCollection,
            // Global
            (1, 0x0) => Kind::UsagePage,
            (1, 0x1) => Kind::LogicalMin,
            (1, 0x2) => Kind::LogicalMax,
            (1, 0x7) => Kind::ReportSize,
            (1, 0x8) => Kind::ReportId,
            (1, 0x9) => Kind::ReportCount,
            // Local
            (2, 0x0) => Kind::Usage,
            (2, 0x1) => Kind::UsageMin,
            (2, 0x2) => Kind::UsageMax,
            _ => Kind::Other,
        };

        match kind {
            Kind::Collection => {
                emit(Item { kind, value, depth });
                depth = depth.saturating_add(1);
            }
            Kind::EndCollection => {
                depth = depth.saturating_sub(1);
                emit(Item { kind, value, depth });
            }
            _ => emit(Item { kind, value, depth }),
        }
    }
}

/// Common HID Usage Page name, or empty if unknown (caller falls back to hex).
pub fn page_name(page: u16) -> &'static [u8] {
    match page {
        0x01 => b"GenericDesktop",
        0x02 => b"Simulation",
        0x07 => b"Keyboard",
        0x08 => b"LED",
        0x09 => b"Button",
        0x0C => b"Consumer",
        0x0D => b"Digitizer",
        p if p >= 0xFF00 => b"Vendor",
        _ => b"",
    }
}

/// Common Usage name within a page, or empty if unknown.
pub fn usage_name(page: u16, usage: u32) -> &'static [u8] {
    match (page, usage) {
        (0x01, 0x01) => b"Pointer",
        (0x01, 0x02) => b"Mouse",
        (0x01, 0x06) => b"Keyboard",
        (0x01, 0x07) => b"Keypad",
        (0x01, 0x80) => b"System",
        (0x01, 0x30) => b"X",
        (0x01, 0x31) => b"Y",
        (0x01, 0x38) => b"Wheel",
        (0x0C, 0x01) => b"ConsumerControl",
        (0x0C, 0x238) => b"ACPan",
        _ => b"",
    }
}

/// Collection type name.
pub fn collection_name(v: u32) -> &'static [u8] {
    match v {
        0x00 => b"Physical",
        0x01 => b"Application",
        0x02 => b"Logical",
        0x03 => b"Report",
        0x04 => b"NamedArray",
        _ => b"",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect decoded items into a fixed array (no_std-friendly, no Vec).
    fn items(desc: &[u8]) -> ([Item; 32], usize) {
        let mut arr = [Item { kind: Kind::Other, value: 0, depth: 0 }; 32];
        let mut n = 0;
        decode(desc, |it| {
            if n < arr.len() {
                arr[n] = it;
                n += 1;
            }
        });
        (arr, n)
    }

    #[test]
    fn decodes_boot_mouse_skeleton() {
        // Usage Page (Generic Desktop), Usage (Mouse), Collection (Application),
        //   Report ID (2), End Collection
        let desc = [
            0x05, 0x01, // UsagePage GD
            0x09, 0x02, // Usage Mouse
            0xA1, 0x01, // Collection Application
            0x85, 0x02, // ReportID 2
            0xC0, // End Collection
        ];
        let (v, _n) = items(&desc);
        assert_eq!(v[0], Item { kind: Kind::UsagePage, value: 0x01, depth: 0 });
        assert_eq!(v[1], Item { kind: Kind::Usage, value: 0x02, depth: 0 });
        assert_eq!(v[2], Item { kind: Kind::Collection, value: 0x01, depth: 0 });
        assert_eq!(v[3], Item { kind: Kind::ReportId, value: 0x02, depth: 1 });
        assert_eq!(v[4], Item { kind: Kind::EndCollection, value: 0, depth: 0 });
    }

    #[test]
    fn multibyte_value_is_little_endian() {
        // Usage Page (2-byte) 0xFF00 (vendor): 0x06, 0x00, 0xFF
        let (v, _n) = items(&[0x06, 0x00, 0xFF]);
        assert_eq!(v[0], Item { kind: Kind::UsagePage, value: 0xFF00, depth: 0 });
    }

    #[test]
    fn nested_collection_depth() {
        let desc = [
            0xA1, 0x01, // Collection App      (depth 0, then ->1)
            0xA1, 0x00, // Collection Physical (depth 1, then ->2)
            0x75, 0x08, // ReportSize 8        (depth 2)
            0xC0, // End                 (->1)
            0xC0, // End                 (->0)
        ];
        let (v, _n) = items(&desc);
        assert_eq!(v[0].depth, 0); // outer collection opens at 0
        assert_eq!(v[1].depth, 1); // inner collection opens at 1
        assert_eq!(v[2].depth, 2); // report size inside both
        assert_eq!(v[3], Item { kind: Kind::EndCollection, value: 0, depth: 1 });
        assert_eq!(v[4], Item { kind: Kind::EndCollection, value: 0, depth: 0 });
    }

    #[test]
    fn truncated_item_does_not_panic() {
        // Prefix claims 2 data bytes but only 1 present.
        let (v, n) = items(&[0x06, 0x00]);
        assert_eq!(n, 1);
        assert_eq!(v[0].kind, Kind::UsagePage);
    }

    #[test]
    fn input_output_main_items() {
        // ReportCount 5, ReportSize 1, Input (Data,Var,Abs = 0x02)
        let (v, _n) = items(&[0x95, 0x05, 0x75, 0x01, 0x81, 0x02]);
        assert_eq!(v[0], Item { kind: Kind::ReportCount, value: 5, depth: 0 });
        assert_eq!(v[1], Item { kind: Kind::ReportSize, value: 1, depth: 0 });
        assert_eq!(v[2], Item { kind: Kind::Input, value: 0x02, depth: 0 });
    }
}
