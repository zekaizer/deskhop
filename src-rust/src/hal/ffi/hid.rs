// HID report + parser FFI — direct exports replacing C hid_report.c wrappers.

#[export_name = "get_report_value"]
pub unsafe extern "C" fn rust_get_report_value(report: *const u8, len: i32, val: *const u8) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 { return 0; }
    let slice = core::slice::from_raw_parts(report, len as usize);
    let rv = core::ptr::read_unaligned(val as *const crate::app::hid_parser::ReportVal);
    crate::app::hid_report::get_report_value(slice, rv.offset, rv.size)
}

#[export_name = "extract_kbd_data"]
pub unsafe extern "C" fn rust_extract_kbd_data_export(
    raw_report: *mut u8, len: i32, itf: u8, iface: *mut core::ffi::c_void, out: *mut u8,
) -> i32 {
    super::kbd_extract::rust_extract_kbd_data(raw_report, len, itf, iface, out)
}

#[export_name = "extract_data"]
pub unsafe extern "C" fn rust_extract_data_export(
    iface: *mut core::ffi::c_void, val: *const u8,
) {
    super::extract_data::rust_extract_data(iface, val);
}

#[export_name = "parse_report_descriptor"]
pub unsafe extern "C" fn rust_parse_report_descriptor_export(
    iface: *mut core::ffi::c_void, report: *const u8, desc_len: i32,
) {
    super::hid_parser_ffi::rust_parse_report_descriptor(iface, report, desc_len);
}
