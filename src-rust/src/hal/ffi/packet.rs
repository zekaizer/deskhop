// Packet FFI wrappers.

use crate::app::{constants, packet};

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

#[export_name = "process_uart_tx_task"]
pub unsafe extern "C" fn rust_process_uart_tx_task(dev: *mut core::ffi::c_void) {
    let hal = crate::hal::pico::PicoHal::new(dev);
    crate::app::tasks::flush_outbox(&hal);
}

#[export_name = "get_ptr_delta"]
pub unsafe extern "C" fn rust_get_ptr_delta(current: u32, dev: *mut core::ffi::c_void) -> u32 {
    let state = crate::app::structs::device_from_ptr(dev);
    packet::get_ptr_delta(current, state.dma_ptr, 1024)
}
