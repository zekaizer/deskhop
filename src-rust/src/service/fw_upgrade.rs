// Firmware upgrade service — processes incoming firmware bytes and
// manages the upgrade state machine.

use crate::domain::crc;
use crate::domain::structs::DeviceState;
use crate::hal::traits::*;

const STAGING_IMAGE_SIZE: u32 = 262144;
const FLASH_SECTOR_SIZE: u32 = 4096;

/// Receive one firmware data word (4 bytes) during an upgrade.
/// Validates address, accumulates CRC, buffers page data.
pub fn receive_fw_byte(
    state: &mut DeviceState<'_>,
    hal: &impl Indicator,
    address: u32,
    fw_data: &[u8; 4],
) -> bool {
    // Address must match expected sequence
    if address != state.fw.fw.address {
        // Out-of-sequence word aborts an in-flight peer flash mid-transfer;
        // log the gap (expected value still intact) before resetting.
        crate::service::dlog::e(b"fw")
            .s(b"rx ABORT addr got=").u(address)
            .s(b" want=").u(state.fw.fw.address)
            .done();
        state.fw.fw.upgrade_in_progress = false;
        state.fw.fw.address = 0;
        return false;
    }

    // Blink indicator every 4KB boundary (also a natural progress checkpoint:
    // 64 lines for a full 256KB image, so the transfer is traceable / stalls
    // are locatable without flooding the ring per-word).
    if (address & 0xfff) == 0x000 {
        hal.toggle();
        crate::service::dlog::i(b"fw")
            .s(b"rx ").u(address).s(b"/").u(STAGING_IMAGE_SIZE).done();
    }

    // Accumulate CRC (skip last sector — contains CRC itself)
    if address < STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE {
        for &byte in fw_data {
            state.fw.fw.checksum = crc::crc32_iter(state.fw.fw.checksum, byte);
        }
    }

    // Buffer page data
    let offset = (address & 0xFF) as usize; // offset within 256-byte page
    if offset + 4 <= state.fw.page_buffer.len() {
        state.fw.page_buffer[offset..offset + 4].copy_from_slice(fw_data);
    }

    state.fw.fw.address += 4;
    state.fw.fw.byte_done = true;
    true
}

/// Read one firmware word and send it back to the peer as a response.
pub fn send_fw_byte(
    _state: &DeviceState<'_>,
    hal: &(impl ConfigStore + PeerLink),
    address: u32,
) -> Option<[u8; 8]> {
    if address >= STAGING_IMAGE_SIZE {
        // Peer requested the word past the image end — the normal transfer
        // terminator, so this doubles as a "source reached end" marker.
        crate::service::dlog::i(b"fw")
            .s(b"tx end addr=").u(address).s(b" max=").u(STAGING_IMAGE_SIZE).done();
        return None;
    }
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
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 0;
        state.fw.fw.upgrade_in_progress = true;

        assert!(receive_fw_byte(&mut state, &hal, 0, &[0xAA, 0xBB, 0xCC, 0xDD]));
        assert_eq!(state.fw.fw.address, 4);
        assert!(state.fw.fw.byte_done);
        // First address is 4KB boundary → toggle
        assert_eq!(hal.toggle_count.get(), 1);
    }

    #[test]
    fn receive_address_mismatch_aborts() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 100;
        state.fw.fw.upgrade_in_progress = true;

        assert!(!receive_fw_byte(&mut state, &hal, 200, &[0; 4]));
        assert!(!state.fw.fw.upgrade_in_progress);
        assert_eq!(state.fw.fw.address, 0);
    }

    #[test]
    fn receive_accumulates_crc() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 4;
        state.fw.fw.checksum = 0xFFFFFFFF;

        receive_fw_byte(&mut state, &hal, 4, &[0x01, 0x02, 0x03, 0x04]);

        // CRC should have changed from initial
        assert_ne!(state.fw.fw.checksum, 0xFFFFFFFF);
    }

    #[test]
    fn receive_skips_crc_for_last_sector() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let addr = STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE; // exactly at boundary
        state.fw.fw.address = addr;
        let initial_crc = state.fw.fw.checksum;

        receive_fw_byte(&mut state, &hal, addr, &[0xFF; 4]);

        // CRC should NOT change (last sector excluded)
        assert_eq!(state.fw.fw.checksum, initial_crc);
    }

    #[test]
    fn receive_toggles_at_4k_boundary() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 0x1000; // 4KB boundary

        receive_fw_byte(&mut state, &hal, 0x1000, &[0; 4]);

        assert_eq!(hal.toggle_count.get(), 1);
    }

    #[test]
    fn receive_no_toggle_mid_sector() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 0x1004; // NOT a boundary

        receive_fw_byte(&mut state, &hal, 0x1004, &[0; 4]);

        assert_eq!(hal.toggle_count.get(), 0);
    }

    #[test]
    fn send_fw_byte_valid_address() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };

        let result = send_fw_byte(&state, &hal, 0x100);
        assert!(result.is_some());
    }

    #[test]
    fn send_fw_byte_out_of_range() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };

        let result = send_fw_byte(&state, &hal, STAGING_IMAGE_SIZE);
        assert!(result.is_none());
    }
}
