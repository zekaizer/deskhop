// Misc utility FFI — checksum, screensaver, packet helpers.

use crate::domain::{constants, crc, packet};
use crate::domain::screensaver::{PongState, JitterState, MouseReport as SSMouseReport};

// --- Checksum ---

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

// --- Screensaver ---

static mut PONG_STATE: PongState = PongState::new();
static mut JITTER_STATE: JitterState = JitterState::new();

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong(out: *mut u8) {
    let state = &mut *core::ptr::addr_of_mut!(PONG_STATE);
    write_report(out, &state.step());
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter(out: *mut u8) {
    let state = &mut *core::ptr::addr_of_mut!(JITTER_STATE);
    write_report(out, &state.step());
}

unsafe fn write_report(out: *mut u8, r: &SSMouseReport) {
    if out.is_null() { return; }
    *out = r.buttons;
    let xb = r.x.to_le_bytes(); *out.add(1) = xb[0]; *out.add(2) = xb[1];
    let yb = r.y.to_le_bytes(); *out.add(3) = yb[0]; *out.add(4) = yb[1];
    *out.add(5) = r.wheel as u8; *out.add(6) = r.pan as u8; *out.add(7) = r.mode;
}

// --- Packet helpers ---

#[export_name = "write_raw_packet"]
pub unsafe extern "C" fn rust_write_raw_packet(dst: *mut u8, packet_ptr: *const u8) {
    if dst.is_null() || packet_ptr.is_null() { return; }
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

/* get_ptr_delta FFI wrapper removed — C callers migrated to Rust
   service::tasks::packet_receive_tick via DmaRx trait. The pure
   function packet::get_ptr_delta() remains available. */
