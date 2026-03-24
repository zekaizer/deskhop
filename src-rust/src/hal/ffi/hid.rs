#[no_mangle]
pub unsafe extern "C" fn rust_get_descriptor_value(report: *const u8, size: i32) -> u32 {
    if report.is_null() { return 0; }
    let max_len = match size { 1 => 1, 2 => 2, 3 => 4, _ => 0 };
    let data = core::slice::from_raw_parts(report, max_len);
    crate::app::hid_parser::get_descriptor_value(data, size as u8)
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_report_value(report: *const u8, len: i32, val: *const u8) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 { return 0; }
    let slice = core::slice::from_raw_parts(report, len as usize);
    // ReportVal is #[repr(C, packed)] = same as C report_val_t
    let rv = core::ptr::read_unaligned(val as *const crate::app::hid_parser::ReportVal);
    crate::app::hid_report::get_report_value(slice, rv.offset, rv.size)
}
