// FFI exports — functions callable from C.
// All functions here are #[no_mangle] pub extern "C" and form the
// Rust→C API boundary. Internal Rust functions should NOT be here.

use crate::app::constants;
use crate::app::crc;

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
