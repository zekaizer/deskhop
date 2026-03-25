// Firmware upgrade + API message FFI handlers.

use crate::app::constants::PacketType;

#[no_mangle]
pub unsafe extern "C" fn rust_handle_response_byte(data: *const u8, dev: *mut core::ffi::c_void) {
    if data.is_null() { return; }
    let state = crate::app::structs::device_from_ptr(dev);

    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);

    if address != state.fw.address {
        state.fw.upgrade_in_progress = false;
        state.fw.address = 0;
        return;
    }

    if (address & 0xfff) == 0x000 {
        crate::hal::device::hal_toggle_led();
    }

    const STAGING_IMAGE_SIZE: u32 = 262144;
    const FLASH_SECTOR_SIZE: u32 = 4096;

    if address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE {
        for i in 0..4 {
            state.fw.checksum = crate::app::crc::crc32_iter(state.fw.checksum, *data.add(4 + i));
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
    let address = u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]);
    const STAGING_IMAGE_SIZE: u32 = 262144;
    if address >= STAGING_IMAGE_SIZE { return; }
    let fw_data = crate::hal::device::hal_read_fw_running_u32(address);
    let bytes = fw_data.to_le_bytes();
    *data.add(4) = bytes[0]; *data.add(5) = bytes[1];
    *data.add(6) = bytes[2]; *data.add(7) = bytes[3];
    crate::hal::device::hal_queue_packet(
        data, PacketType::ResponseByte as u8, 8,
    );
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_msgs(ptype: u8, data: *const u8, dev: *mut core::ffi::c_void) {
    if data.is_null() { return; }
    let api_idx = *data;
    let mut offset: u32 = 0;
    let mut len: u32 = 0;
    let mut readonly = false;

    if crate::hal::device::hal_get_field_map(api_idx, &mut offset, &mut len, &mut readonly) != 0 {
        return;
    }

    const SET_VAL: u8 = PacketType::SetVal as u8;
    const GET_VAL: u8 = PacketType::GetVal as u8;

    if ptype == SET_VAL {
        if readonly { return; }
        crate::hal::device::hal_api_write_field(offset, len, data.add(1));
    } else if ptype == GET_VAL {
        let mut response = [0u8; 10];
        response[0] = GET_VAL;
        response[1] = api_idx;
        crate::hal::device::hal_api_read_field(offset, len, response[2..].as_mut_ptr());
        crate::hal::device::hal_queue_cfg_packet(dev, response.as_ptr());
    }

    let state = crate::app::structs::device_from_ptr(dev);
    state.config_mode_timer = crate::hal::device::hal_time_us_64() + 300_000_000;
}

#[no_mangle]
pub unsafe extern "C" fn rust_handle_api_read_all_msgs(dev: *mut core::ffi::c_void) {
    let count = crate::hal::device::hal_get_field_map_length();
    for i in 0..count {
        let idx = crate::hal::device::hal_get_field_map_idx(i);
        let data = [idx, 0, 0, 0, 0, 0, 0, 0];
        rust_handle_api_msgs(PacketType::GetVal as u8, data.as_ptr(), dev);
    }
}
