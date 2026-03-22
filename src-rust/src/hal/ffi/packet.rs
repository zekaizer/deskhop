use crate::app::{constants, packet};

#[no_mangle]
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

#[no_mangle]
pub extern "C" fn rust_get_ptr_delta(current: u32, saved: u32, buffer_size: u32) -> u32 {
    packet::get_ptr_delta(current, saved, buffer_size)
}

/// Replace C's queue_packet — build uart_packet_t and queue via HAL
#[no_mangle]
pub unsafe extern "C" fn rust_queue_packet(
    data: *const u8, packet_type: u8, length: i32, dev: *mut core::ffi::c_void,
) {
    if data.is_null() || length <= 0 { return; }
    let mut pkt = [0u8; 10]; // uart_packet_t: type(1) + data(8) + checksum(1)
    pkt[0] = packet_type;
    let len = core::cmp::min(length as usize, 8);
    core::ptr::copy_nonoverlapping(data, pkt[1..].as_mut_ptr(), len);
    crate::hal::device::hal_queue_uart_packet(dev, pkt.as_ptr());
}

/// Replace C's send_value
#[no_mangle]
pub unsafe extern "C" fn rust_send_value(value: u8, packet_type: u8, dev: *mut core::ffi::c_void) {
    rust_queue_packet(&value as *const u8, packet_type, 1, dev);
}
