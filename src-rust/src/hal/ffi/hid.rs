/// Extract boot protocol keyboard report — pure logic, no hid_interface_t.
#[no_mangle]
pub unsafe extern "C" fn rust_extract_kbd_boot(
    raw_report: *const u8, len: i32, out_report: *mut u8,
) -> i32 {
    if raw_report.is_null() || out_report.is_null() || len < 8 { return 0; }
    let src = if len == 9 { raw_report.add(1) } else { raw_report };
    core::ptr::copy_nonoverlapping(src, out_report, 8);
    8
}

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
    let offset = u16::from_le_bytes([*val, *val.add(1)]);
    let size = u16::from_le_bytes([*val.add(4), *val.add(5)]);
    crate::app::hid_report::get_report_value(slice, offset, size)
}

#[no_mangle]
pub unsafe extern "C" fn rust_extract_bit_variable(
    kbd: *const u8, raw_report: *const u8, len: i32, dst: *mut u8,
) -> i32 {
    if kbd.is_null() || raw_report.is_null() || dst.is_null() || len <= 0 { return 0; }
    let report = core::slice::from_raw_parts(raw_report, len as usize);
    let dst_slice = core::slice::from_raw_parts_mut(dst, len as usize);
    let offset = u16::from_le_bytes([*kbd, *kbd.add(1)]);
    let usage_min = i32::from_le_bytes([*kbd.add(6), *kbd.add(7), *kbd.add(8), *kbd.add(9)]);
    let usage_max = i32::from_le_bytes([*kbd.add(10), *kbd.add(11), *kbd.add(12), *kbd.add(13)]);
    crate::app::hid_report::extract_bit_variable(report, usage_min, usage_max, offset, dst_slice) as i32
}
