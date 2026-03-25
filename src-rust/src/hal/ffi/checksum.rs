use crate::app::{constants, crc};

// Direct C name exports — replaces flash_config.c CRC wrappers

#[export_name = "calc_checksum"]
pub unsafe extern "C" fn rust_calc_checksum(data: *const u8, length: i32) -> u8 {
    if data.is_null() || length <= 0 { return 0; }
    crc::calc_checksum(core::slice::from_raw_parts(data, length as usize))
}

#[export_name = "calc_crc32"]
pub unsafe extern "C" fn rust_calc_crc32(data: *const u8, length: usize) -> u32 {
    if data.is_null() { return 0; }
    crc::calc_crc32(core::slice::from_raw_parts(data, length))
}

#[export_name = "crc32_iter"]
pub extern "C" fn rust_crc32_iter(crc: u32, byte: u8) -> u32 {
    crc::crc32_iter(crc, byte)
}

#[export_name = "verify_checksum"]
pub unsafe extern "C" fn rust_verify_checksum(packet: *const u8) -> bool {
    if packet.is_null() { return false; }
    let data = core::slice::from_raw_parts(packet.add(1), 8);
    crc::calc_checksum(data) == *packet.add(9)
}

#[export_name = "validate_packet"]
pub unsafe extern "C" fn rust_validate_packet(packet: *const u8) -> bool {
    if packet.is_null() { return false; }
    constants::validate_packet_type(*packet, *packet.add(1))
}
