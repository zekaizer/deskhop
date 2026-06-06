// Firmware upgrade service — processes incoming firmware bytes and
// manages the upgrade state machine.

use crate::domain::crc;
use crate::domain::structs::DeviceState;
use crate::hal::traits::*;

const STAGING_IMAGE_SIZE: u32 = 262144;
const FLASH_SECTOR_SIZE: u32 = 4096;
const FLASH_PAGE_SIZE: u32 = 256;

/// One receiver tick's decision, computed purely from the current expected
/// `address`. Extracted from C (`firmware_upgrade_task_c`) so the page/sector/
/// terminal arithmetic — where the off-by-one shipped — is unit-testable.
/// `#[repr(C)]` so the C task can consume it directly via `rust_fw_next_step`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FwStep {
    /// Flush the just-completed 256B page at `page_offset` (image offset; caller
    /// adds the flash base). 0 = no page to write this tick.
    pub write_page: u8,
    /// Erase the 4KB sector before programming the page.
    pub erase_sector: u8,
    /// All bytes received: verify CRC and promote/reboot (do not request more).
    pub finalize: u8,
    pub _pad: u8,
    pub page_offset: u32,
    /// Address to request next when not finalizing.
    pub request_address: u32,
}

/// Pure decision for the fw-upgrade receiver given the current expected address.
///
/// - Flush the completed page on a 256B boundary, but NEVER at address 0 (no
///   page is complete yet; `(0-1) & !0xFF` would be a wild address — a latent
///   bug in the in-C version, fixed here by the explicit `address != 0` guard).
/// - Erase the sector when the page is sector-aligned.
/// - Finalize at `address >= STAGING_IMAGE_SIZE` (>=, not >: the address caps at
///   the image size since the source returns no byte past the end — a strict `>`
///   never fired and the upgrade never completed). The final page is flushed in
///   the same tick, BEFORE finalize, so the last 256B make it to flash.
pub fn next_step(address: u32) -> FwStep {
    let mut s = FwStep::default();
    if address != 0 && address.is_multiple_of(FLASH_PAGE_SIZE) {
        let offset = (address - 1) & !(FLASH_PAGE_SIZE - 1);
        s.write_page = 1;
        s.page_offset = offset;
        s.erase_sector = offset.is_multiple_of(FLASH_SECTOR_SIZE) as u8;
    }
    if address >= STAGING_IMAGE_SIZE {
        s.finalize = 1;
    } else {
        s.request_address = address;
    }
    s
}

/// FFI: fill `*out` with the receiver's next-tick decision for `address`.
///
/// # Safety
/// `out` must point to a valid `FwStep`.
#[no_mangle]
pub unsafe extern "C" fn rust_fw_next_step(address: u32, out: *mut FwStep) {
    if out.is_null() {
        return;
    }
    core::ptr::write(out, next_step(address));
}

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

    #[test]
    fn send_fw_byte_last_valid_word_is_some() {
        // The last in-range word must still be served (mirror of the receiver
        // terminal off-by-one on the source side).
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        assert!(send_fw_byte(&state, &hal, STAGING_IMAGE_SIZE - 4).is_some());
        assert!(send_fw_byte(&state, &hal, STAGING_IMAGE_SIZE + 4).is_none());
    }

    // -- next_step (the extracted, previously-untestable C step machine) --

    #[test]
    fn step_terminal_uses_ge_not_gt() {
        // THE test that would have caught the shipped off-by-one: address ==
        // STAGING_IMAGE_SIZE must finalize; one word short must not.
        assert_eq!(next_step(STAGING_IMAGE_SIZE).finalize, 1);
        assert_eq!(next_step(STAGING_IMAGE_SIZE - 4).finalize, 0);
        // ...and finalize must NOT request another byte (the wedge that left
        // the upgrade requesting the out-of-range address forever).
        assert_eq!(next_step(STAGING_IMAGE_SIZE).request_address, 0);
    }

    #[test]
    fn step_address_zero_writes_no_page() {
        // address 0 is a 256B "boundary" but no page is complete; writing would
        // target (0-1)&!0xFF = 0xFFFFFF00 (a wild flash address). Latent C bug.
        assert_eq!(next_step(0).write_page, 0);
        assert_eq!(next_step(0).finalize, 0);
        assert_eq!(next_step(0).request_address, 0);
    }

    #[test]
    fn step_page_write_address_is_completed_page() {
        // On a boundary, flush the page that just completed: offset = address-256.
        let s = next_step(0x100);
        assert_eq!(s.write_page, 1);
        assert_eq!(s.page_offset, 0x000);
        let s = next_step(0x200);
        assert_eq!(s.page_offset, 0x100);
        // Final boundary flushes the last page AND finalizes in the same tick.
        let s = next_step(STAGING_IMAGE_SIZE);
        assert_eq!(s.write_page, 1);
        assert_eq!(s.page_offset, STAGING_IMAGE_SIZE - FLASH_PAGE_SIZE);
        assert_eq!(s.finalize, 1);
    }

    #[test]
    fn step_non_boundary_just_requests() {
        let s = next_step(0x104);
        assert_eq!(s.write_page, 0);
        assert_eq!(s.finalize, 0);
        assert_eq!(s.request_address, 0x104);
    }

    #[test]
    fn step_sector_erase_only_on_first_page_of_sector() {
        // Erase fires when the flushed page is sector-aligned (offset % 4096 == 0):
        // the first page of each sector. 0x000 (addr 0x100), 0x1000 (addr 0x1100)...
        assert_eq!(next_step(0x100).erase_sector, 1); // offset 0x000
        assert_eq!(next_step(0x200).erase_sector, 0); // offset 0x100
        assert_eq!(next_step(0x1000).erase_sector, 0); // offset 0xF00
        assert_eq!(next_step(0x1100).erase_sector, 1); // offset 0x1000
        // Total erases over the full image == sector count.
        let erases = (1..=STAGING_IMAGE_SIZE / 4)
            .map(|w| w * 4)
            .filter(|&a| next_step(a).erase_sector == 1)
            .count() as u32;
        assert_eq!(erases, STAGING_IMAGE_SIZE / FLASH_SECTOR_SIZE);
    }
}
