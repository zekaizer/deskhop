// Firmware upgrade + API message FFI handlers.

use crate::domain::constants::PacketType;
use crate::hal::traits::*;

#[no_mangle]
pub unsafe extern "C" fn rust_handle_response_byte(data: *const u8, dev: *mut core::ffi::c_void) {
    if data.is_null() { return; }
    let hal = crate::hal::pico::PicoHal::new(dev);
    let state = crate::domain::structs::device_from_ptr(dev);

    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);

    if address != state.fw.address {
        state.fw.upgrade_in_progress = false;
        state.fw.address = 0;
        return;
    }

    if (address & 0xfff) == 0x000 {
        hal.toggle();
    }

    const STAGING_IMAGE_SIZE: u32 = 262144;
    const FLASH_SECTOR_SIZE: u32 = 4096;

    if address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE {
        for i in 0..4 {
            state.fw.checksum = crate::domain::crc::crc32_iter(state.fw.checksum, *data.add(4 + i));
        }
    }

    let offset = *data as usize;
    if offset + 4 <= state.page_buffer.len() {
        let fw_data = core::slice::from_raw_parts(data.add(4), 4);
        state.page_buffer[offset..offset + 4].copy_from_slice(fw_data);
    }

    state.fw.address += 4;
    state.fw.byte_done = true;
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_request_byte(data: *mut u8) {
    if data.is_null() { return; }
    // No dev pointer — use global device for PicoHal
    let state = crate::domain::structs::get_global_device();
    let dev = state as *mut _ as *mut core::ffi::c_void;
    let hal = crate::hal::pico::PicoHal::new(dev);

    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
    const STAGING_IMAGE_SIZE: u32 = 262144;
    if address >= STAGING_IMAGE_SIZE { return; }
    let fw_data = hal.read_running_fw(address);
    let bytes = fw_data.to_le_bytes();
    *data.add(4) = bytes[0]; *data.add(5) = bytes[1];
    *data.add(6) = bytes[2]; *data.add(7) = bytes[3];
    hal.send_packet(data, PacketType::ResponseByte as u8, 8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_msgs(ptype: u8, data: *const u8, dev: *mut core::ffi::c_void) {
    if data.is_null() { return; }
    super::api_config::handle_api_msg(ptype, *data, data.add(1), dev);
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_read_all_msgs(dev: *mut core::ffi::c_void) {
    super::api_config::handle_api_read_all(dev);
}
