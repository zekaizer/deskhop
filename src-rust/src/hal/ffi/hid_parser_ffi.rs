use core::ffi::c_void;
use crate::app::hid_parser;
use crate::hal::device;

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls extract_data for each
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

    // ReportVal is #[repr(C, packed)] = same layout as C report_val_t.
    // Pass pointer directly without manual serialization.
    for input in results.iter() {
        if input.uses_report_id {
            device::hal_iface_set_uses_report_id(iface, true);
        }

        for i in 0..input.count {
            let val = &input.vals[i];
            super::extract_data::rust_extract_data(iface, val as *const _ as *const u8);
        }
    }
}
