use core::ffi::c_void;
use crate::app::hid_parser;
use crate::hal::device;

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls C extract_data for each
/// parsed INPUT item to populate hid_interface_t.
#[no_mangle]
pub unsafe extern "C" fn rust_parse_report_descriptor(
    iface: *mut c_void,  // hid_interface_t*
    report: *const u8,
    desc_len: i32,
) {
    if iface.is_null() || report.is_null() || desc_len <= 0 {
        return;
    }

    let desc = core::slice::from_raw_parts(report, desc_len as usize);
    let (_parser, results) = hid_parser::parse_descriptor(desc);

    // For each parsed INPUT item, serialize ReportVal to packed C layout
    // and call extract_data to populate hid_interface_t
    for input in results.iter() {
        if input.uses_report_id {
            device::hal_iface_set_uses_report_id(iface, true);
        }

        for i in 0..input.count {
            let val = &input.vals[i];
            // Serialize to C's packed report_val_t layout (23 bytes)
            let mut packed = [0u8; 24]; // slightly over to be safe
            packed[0..2].copy_from_slice(&val.offset.to_le_bytes());
            packed[2..4].copy_from_slice(&val.offset_idx.to_le_bytes());
            packed[4..6].copy_from_slice(&val.size.to_le_bytes());
            packed[6..10].copy_from_slice(&val.usage_min.to_le_bytes());
            packed[10..14].copy_from_slice(&val.usage_max.to_le_bytes());
            packed[14] = val.item_type;
            packed[15] = val.data_type;
            packed[16] = val.report_id;
            packed[17..19].copy_from_slice(&val.global_usage.to_le_bytes());
            packed[19..21].copy_from_slice(&val.usage_page.to_le_bytes());
            packed[21..23].copy_from_slice(&val.usage.to_le_bytes());

            super::extract_data::rust_extract_data(iface, packed.as_ptr());
        }
    }
}
