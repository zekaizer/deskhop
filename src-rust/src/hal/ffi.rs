// FFI exports — functions callable from C.
// All functions here are #[no_mangle] pub extern "C" and form the
// Rust→C API boundary. Internal Rust functions should NOT be here.

use crate::app::{constants, crc, mouse, packet};

// ---- Checksum / CRC ----

#[no_mangle]
pub unsafe extern "C" fn rust_calc_checksum(data: *const u8, length: i32) -> u8 {
    if data.is_null() || length <= 0 {
        return 0;
    }
    crc::calc_checksum(core::slice::from_raw_parts(data, length as usize))
}

#[no_mangle]
pub unsafe extern "C" fn rust_calc_crc32(data: *const u8, length: usize) -> u32 {
    if data.is_null() {
        return 0;
    }
    crc::calc_crc32(core::slice::from_raw_parts(data, length))
}

#[no_mangle]
pub extern "C" fn rust_crc32_iter(crc: u32, byte: u8) -> u32 {
    crc::crc32_iter(crc, byte)
}

/// Verify checksum of a uart_packet_t.
/// Layout: [type(1) + data(8) + checksum(1)] = 10 bytes
#[no_mangle]
pub unsafe extern "C" fn rust_verify_checksum(packet: *const u8) -> bool {
    if packet.is_null() {
        return false;
    }
    let data = core::slice::from_raw_parts(packet.add(1), 8);
    let checksum = *packet.add(9);
    crc::calc_checksum(data) == checksum
}

// ---- Packet validation ----

/// Validate packet type for config endpoint.
/// Layout: [type(1) + data(8) + checksum(1)]
#[no_mangle]
pub unsafe extern "C" fn rust_validate_packet(packet: *const u8) -> bool {
    if packet.is_null() {
        return false;
    }
    let packet_type = *packet;
    let proxy_inner = *packet.add(1);
    constants::validate_packet_type(packet_type, proxy_inner)
}

// ---- Mouse math ----

#[no_mangle]
pub extern "C" fn rust_move_and_keep_on_screen(position: i32, offset: i32) -> i32 {
    mouse::move_and_keep_on_screen(position, offset)
}

#[no_mangle]
pub extern "C" fn rust_is_screen_switch_needed(position: i32, offset: i32, threshold: u16) -> i32 {
    mouse::is_screen_switch_needed(position, offset, threshold)
}

#[no_mangle]
pub extern "C" fn rust_calculate_mouse_acceleration_factor(
    offset_x: i32,
    offset_y: i32,
    enabled: bool,
) -> f32 {
    mouse::calculate_mouse_acceleration_factor(offset_x, offset_y, enabled)
}

#[no_mangle]
pub extern "C" fn rust_scale_y_coordinate(
    pointer_y: i16,
    from_top: i32,
    from_bottom: i32,
    to_top: i32,
    to_bottom: i32,
) -> i16 {
    mouse::scale_y_coordinate(pointer_y, (from_top, from_bottom), (to_top, to_bottom))
}

// ---- HID report ----

/// C-callable: get_report_value(report, len, val) -> int32_t
/// val points to report_val_t — we need offset (u16 at +0) and size (u16 at +4)
#[no_mangle]
pub unsafe extern "C" fn rust_get_report_value(
    report: *const u8,
    len: i32,
    val: *const u8,
) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 {
        return 0;
    }
    let report_slice = core::slice::from_raw_parts(report, len as usize);
    // ReportVal layout: offset(u16) at +0, offset_idx(u16) at +2, size(u16) at +4
    let offset = u16::from_le_bytes([*val, *val.add(1)]);
    let size = u16::from_le_bytes([*val.add(4), *val.add(5)]);
    crate::app::hid_report::get_report_value(report_slice, offset, size)
}

// ---- Keyboard ----

/// C-callable: key_in_report(key, report) -> bool
/// report points to hid_keyboard_report_t [modifier(1) + reserved(1) + keycode(6)]
#[no_mangle]
pub unsafe extern "C" fn rust_key_in_report(key: u8, report: *const u8) -> bool {
    if report.is_null() {
        return false;
    }
    // keycode starts at offset 2, length 6
    let keycode = core::slice::from_raw_parts(report.add(2), 6);
    keycode.iter().any(|&k| k == key)
}

// ---- Packet utilities ----

#[no_mangle]
pub unsafe extern "C" fn rust_write_raw_packet(dst: *mut u8, packet_ptr: *const u8) {
    if dst.is_null() || packet_ptr.is_null() {
        return;
    }
    let pkt = packet::UartPacket {
        ptype: *packet_ptr,
        data: {
            let mut d = [0u8; constants::PACKET_DATA_LENGTH];
            core::ptr::copy_nonoverlapping(packet_ptr.add(1), d.as_mut_ptr(), constants::PACKET_DATA_LENGTH);
            d
        },
        checksum: *packet_ptr.add(9),
    };
    let raw = packet::write_raw_packet(&pkt);
    core::ptr::copy_nonoverlapping(raw.as_ptr(), dst, constants::RAW_PACKET_LENGTH);
}

#[no_mangle]
pub extern "C" fn rust_get_ptr_delta(current: u32, saved: u32, buffer_size: u32) -> u32 {
    packet::get_ptr_delta(current, saved, buffer_size)
}
