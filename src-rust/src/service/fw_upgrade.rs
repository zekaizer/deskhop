// Firmware upgrade service — processes incoming firmware bytes and
// manages the upgrade state machine.

use crate::domain::crc;
use crate::domain::structs::Device;
use crate::hal::traits::*;

const STAGING_IMAGE_SIZE: u32 = 262144;
const FLASH_SECTOR_SIZE: u32 = 4096;

/// Receive one firmware data word (4 bytes) during an upgrade.
/// Validates address, accumulates CRC, buffers page data.
pub fn receive_fw_byte(
    state: &mut Device,
    hal: &impl Indicator,
    address: u32,
    fw_data: &[u8; 4],
) -> bool {
    // Address must match expected sequence
    if address != state.fw.address {
        state.fw.upgrade_in_progress = false;
        state.fw.address = 0;
        return false;
    }

    // Blink indicator every 4KB boundary
    if (address & 0xfff) == 0x000 {
        hal.toggle();
    }

    // Accumulate CRC (skip last sector — contains CRC itself)
    if address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE {
        for &byte in fw_data {
            state.fw.checksum = crc::crc32_iter(state.fw.checksum, byte);
        }
    }

    // Buffer page data
    let offset = (address & 0xFF) as usize; // offset within 256-byte page
    if offset + 4 <= state.page_buffer.len() {
        state.page_buffer[offset..offset + 4].copy_from_slice(fw_data);
    }

    state.fw.address += 4;
    state.fw.byte_done = true;
    true
}

/// Read one firmware word and send it back to the peer as a response.
pub fn send_fw_byte(
    _state: &Device,
    hal: &(impl ConfigStore + PeerLink),
    address: u32,
) -> Option<[u8; 8]> {
    if address >= STAGING_IMAGE_SIZE { return None; }
    let fw_data = hal.read_running_fw(address);
    let bytes = fw_data.to_le_bytes();
    let mut response = [0u8; 8];
    response[0] = (address & 0xFF) as u8;
    response[1] = ((address >> 8) & 0xFF) as u8;
    response[2] = ((address >> 16) & 0xFF) as u8;
    response[3] = ((address >> 24) & 0xFF) as u8;
    response[4] = bytes[0];
    response[5] = bytes[1];
    response[6] = bytes[2];
    response[7] = bytes[3];
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    #[test]
    fn receive_first_byte_ok() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.fw.address = 0;
        state.fw.upgrade_in_progress = true;

        assert!(receive_fw_byte(&mut state, &hal, 0, &[0xAA, 0xBB, 0xCC, 0xDD]));
        assert_eq!(state.fw.address, 4);
        assert!(state.fw.byte_done);
        // First address is 4KB boundary → toggle
        assert_eq!(hal.toggle_count.get(), 1);
    }

    #[test]
    fn receive_address_mismatch_aborts() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.fw.address = 100;
        state.fw.upgrade_in_progress = true;

        assert!(!receive_fw_byte(&mut state, &hal, 200, &[0; 4]));
        assert!(!state.fw.upgrade_in_progress);
        assert_eq!(state.fw.address, 0);
    }

    #[test]
    fn receive_accumulates_crc() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.fw.address = 4;
        state.fw.checksum = 0xFFFFFFFF;

        receive_fw_byte(&mut state, &hal, 4, &[0x01, 0x02, 0x03, 0x04]);

        // CRC should have changed from initial
        assert_ne!(state.fw.checksum, 0xFFFFFFFF);
    }

    #[test]
    fn receive_skips_crc_for_last_sector() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        let addr = STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE; // exactly at boundary
        state.fw.address = addr;
        let initial_crc = state.fw.checksum;

        receive_fw_byte(&mut state, &hal, addr, &[0xFF; 4]);

        // CRC should NOT change (last sector excluded)
        assert_eq!(state.fw.checksum, initial_crc);
    }

    #[test]
    fn receive_toggles_at_4k_boundary() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.fw.address = 0x1000; // 4KB boundary

        receive_fw_byte(&mut state, &hal, 0x1000, &[0; 4]);

        assert_eq!(hal.toggle_count.get(), 1);
    }

    #[test]
    fn receive_no_toggle_mid_sector() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.fw.address = 0x1004; // NOT a boundary

        receive_fw_byte(&mut state, &hal, 0x1004, &[0; 4]);

        assert_eq!(hal.toggle_count.get(), 0);
    }

    #[test]
    fn send_fw_byte_valid_address() {
        let hal = MockHal::new();
        let state = Device::zeroed();

        let result = send_fw_byte(&state, &hal, 0x100);
        assert!(result.is_some());
    }

    #[test]
    fn send_fw_byte_out_of_range() {
        let hal = MockHal::new();
        let state = Device::zeroed();

        let result = send_fw_byte(&state, &hal, STAGING_IMAGE_SIZE);
        assert!(result.is_none());
    }
}
