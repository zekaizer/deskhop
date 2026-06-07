// Firmware upgrade service — processes incoming firmware bytes and
// manages the upgrade state machine.

use crate::domain::crc;
use crate::domain::structs::DeviceState;
use crate::hal::traits::*;

const STAGING_IMAGE_SIZE: u32 = 262144;
const FLASH_SECTOR_SIZE: u32 = 4096;
const FLASH_PAGE_SIZE: u32 = 256;

const UF2_MAGIC_START0: u32 = 0x0A32_4655;
const UF2_MAGIC_START1: u32 = 0x9E5D_5157;
const UF2_MAGIC_END: u32 = 0x0AB1_6F30;

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

/// One USB-MSC UF2 block's write decision (the config-mode drag-and-drop upgrade
/// path, which writes the new image straight to the RUNNING slot). Computed
/// purely from the UF2 header so the off-by-one-prone block arithmetic — final
/// block, page address, CRC-coverage gate — is unit-testable, the same treatment
/// `next_step` gives the UART auto-sync path. `#[repr(C)]` so tud_msc_write10_cb
/// can consume it via `rust_msc_write_step`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MscStep {
    /// The block carries valid UF2 magic. 0 = not a UF2 block (caller ignores it).
    pub is_uf2: u8,
    /// First block (blockNo 0): (re)initialize the running checksum + flag.
    pub is_first: u8,
    /// Payload is inside the CRC-protected region (everything but the final
    /// sector, which holds the metadata/CRC) — caller folds it into the checksum.
    pub accumulate_crc: u8,
    /// Last block: caller finalizes the checksum, verifies, and reboots/recovers.
    pub is_final: u8,
    /// Image offset of this page (caller adds the running-slot flash base).
    pub flash_offset: u32,
}

/// Pure decision for one UF2 block written over USB-MSC. `magic0`/`magic1`/
/// `magic_end` are the UF2 header's start0/start1/end words; a mismatch yields
/// `is_uf2 = 0` and the caller does nothing.
pub fn msc_write_step(block_no: u32, magic0: u32, magic1: u32, magic_end: u32) -> MscStep {
    let mut s = MscStep::default();
    if magic0 != UF2_MAGIC_START0 || magic1 != UF2_MAGIC_START1 || magic_end != UF2_MAGIC_END {
        return s;
    }
    s.is_uf2 = 1;
    s.is_first = (block_no == 0) as u8;
    // CRC covers every page except the final sector, matching the range of
    // calculate_firmware_crc32 (STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE).
    let last_block_with_checksum = (STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE) / FLASH_PAGE_SIZE;
    s.accumulate_crc = (block_no < last_block_with_checksum) as u8;
    s.is_final = (block_no == STAGING_IMAGE_SIZE / FLASH_PAGE_SIZE - 1) as u8;
    s.flash_offset = block_no * FLASH_PAGE_SIZE;
    s
}

/// FFI: fill `*out` with the write decision for one UF2 block.
///
/// # Safety
/// `out` must point to a valid `MscStep`.
#[no_mangle]
pub unsafe extern "C" fn rust_msc_write_step(
    block_no: u32,
    magic0: u32,
    magic1: u32,
    magic_end: u32,
    out: *mut MscStep,
) {
    if out.is_null() {
        return;
    }
    core::ptr::write(out, msc_write_step(block_no, magic0, magic1, magic_end));
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

    #[test]
    fn step_full_image_invariants() {
        // write_page fires exactly SIZE/256 == 1024 times across the full sweep:
        // every 256B boundary except address 0 (incl. the final SIZE boundary).
        let writes = (0..=STAGING_IMAGE_SIZE)
            .step_by(4)
            .filter(|&a| next_step(a).write_page == 1)
            .count();
        assert_eq!(writes, (STAGING_IMAGE_SIZE / FLASH_PAGE_SIZE) as usize); // 1024
        assert_eq!(next_step(0).write_page, 0);
        // Every in-range word requests itself and does not finalize.
        for a in (0..STAGING_IMAGE_SIZE).step_by(4) {
            assert_eq!(next_step(a).finalize, 0);
            assert_eq!(next_step(a).request_address, a);
        }
        // The cap word is the sole finalize and requests nothing further.
        assert_eq!(next_step(STAGING_IMAGE_SIZE).finalize, 1);
        assert_eq!(next_step(STAGING_IMAGE_SIZE).request_address, 0);
    }

    // -- msc_write_step (the USB-MSC drag-drop UF2 write decision) --

    const M0: u32 = UF2_MAGIC_START0;
    const M1: u32 = UF2_MAGIC_START1;
    const ME: u32 = UF2_MAGIC_END;

    #[test]
    fn msc_rejects_non_uf2_block() {
        // No UF2 magic → ignore the block entirely (caller returns bufsize).
        let s = msc_write_step(0, 0, 0, 0);
        assert_eq!(s.is_uf2, 0);
        assert_eq!(s.is_first, 0);
        assert_eq!(s.is_final, 0);
        assert_eq!(s.accumulate_crc, 0);
        // A single wrong magic word is enough to reject.
        assert_eq!(msc_write_step(0, M0, M1, ME ^ 1).is_uf2, 0);
        assert_eq!(msc_write_step(0, M0 ^ 1, M1, ME).is_uf2, 0);
    }

    #[test]
    fn msc_first_block_inits_and_writes_offset_zero() {
        let s = msc_write_step(0, M0, M1, ME);
        assert_eq!(s.is_uf2, 1);
        assert_eq!(s.is_first, 1);
        assert_eq!(s.flash_offset, 0);
        assert_eq!(s.accumulate_crc, 1);
        assert_eq!(s.is_final, 0);
    }

    #[test]
    fn msc_flash_offset_is_block_times_page() {
        let s = msc_write_step(5, M0, M1, ME);
        assert_eq!(s.flash_offset, 5 * FLASH_PAGE_SIZE);
        assert_eq!(s.is_first, 0);
    }

    #[test]
    fn msc_final_block_is_last_page_only() {
        let last = STAGING_IMAGE_SIZE / FLASH_PAGE_SIZE - 1; // 1023
        let s = msc_write_step(last, M0, M1, ME);
        assert_eq!(s.is_final, 1);
        assert_eq!(s.flash_offset, last * FLASH_PAGE_SIZE);
        // One block short must NOT finalize (the off-by-one the C had no test for).
        assert_eq!(msc_write_step(last - 1, M0, M1, ME).is_final, 0);
    }

    #[test]
    fn msc_crc_excludes_final_metadata_sector() {
        // CRC covers blocks 0..1007; the final sector (blocks 1008..1023, the last
        // 4KB holding the metadata/CRC) is excluded — mirrors the C gate
        // `blockNo < (STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE) / FLASH_PAGE_SIZE`.
        let last_crc = (STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE) / FLASH_PAGE_SIZE; // 1008
        assert_eq!(msc_write_step(last_crc - 1, M0, M1, ME).accumulate_crc, 1);
        assert_eq!(msc_write_step(last_crc, M0, M1, ME).accumulate_crc, 0);
        // The final block is inside the excluded sector.
        let last = STAGING_IMAGE_SIZE / FLASH_PAGE_SIZE - 1;
        assert_eq!(msc_write_step(last, M0, M1, ME).accumulate_crc, 0);
    }

    // -- CRC lower-edge + byte-order contract --

    #[test]
    fn receive_accumulates_last_word_before_metadata() {
        // 258044 = SIZE - FLASH_SECTOR_SIZE - 4: the LAST word inside the
        // CRC-protected region (complement of receive_skips_crc_for_last_sector,
        // which checks the first EXCLUDED word at SIZE - FLASH_SECTOR_SIZE).
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let addr = STAGING_IMAGE_SIZE - FLASH_SECTOR_SIZE - 4; // 258044
        state.fw.fw.address = addr;
        state.fw.fw.checksum = 0xFFFF_FFFF;

        receive_fw_byte(&mut state, &hal, addr, &[0x01, 0x02, 0x03, 0x04]);

        // Must STILL accumulate. Exact running (non-inverted) CRC32 after folding
        // 01 02 03 04 from the seed — locks crc32_iter byte order + polynomial.
        assert_eq!(state.fw.fw.checksum, 0x49C3_0432);
    }

    #[test]
    fn receive_crc_matches_flat_calc_crc32() {
        // The source<->receiver byte-order contract: feeding N words via
        // receive_fw_byte must yield the SAME CRC as calc_crc32 over the flat
        // byte buffer the source emits (send_fw_byte little-endian words).
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 0;
        state.fw.fw.upgrade_in_progress = true;
        state.fw.fw.checksum = 0xFFFF_FFFF;

        let words: [u32; 4] = [0x1122_3344, 0xAABB_CCDD, 0x0000_0001, 0xFFFF_0000];
        let mut buffer = [0u8; 16];
        for (i, w) in words.iter().enumerate() {
            let le = w.to_le_bytes();
            buffer[i * 4..i * 4 + 4].copy_from_slice(&le);
            assert!(receive_fw_byte(&mut state, &hal, (i * 4) as u32, &le));
        }
        // Receiver keeps the running (non-inverted) CRC; calc_crc32 inverts.
        assert_eq!(!state.fw.fw.checksum, crc::calc_crc32(&buffer));
    }

    // -- page_buffer placement + upper-bound guard --

    #[test]
    fn receive_places_word_at_intra_page_offset() {
        // 0x1008 is not a 4KB boundary; offset within the 256B page = 0x08.
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 0x1008;
        receive_fw_byte(&mut state, &hal, 0x1008, &[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(&state.fw.page_buffer[8..12], &[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(&state.fw.page_buffer[0..8], &[0u8; 8]); // bytes before stay untouched
    }

    #[test]
    fn receive_writes_final_word_of_page() {
        // The last word of a page: offset 252, offset+4 == page_buffer.len() (256).
        // Pins the `offset + 4 <= len()` guard — a regression to `<` would silently
        // drop the last word of every page while the over-the-wire CRC still passes.
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.address = 0xFC; // 0xFC = 252
        receive_fw_byte(&mut state, &hal, 0xFC, &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&state.fw.page_buffer[252..256], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    // -- send_fw_byte ResponseByte layout (address echo + served word) --

    #[test]
    fn send_fw_byte_encodes_address_little_endian() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };

        // 0x030201 (< SIZE) — distinct low bytes catch a >>8/>>16 swap.
        let r = send_fw_byte(&state, &hal, 0x030201).unwrap();
        assert_eq!(&r[0..4], &[0x01, 0x02, 0x03, 0x00]);
        // Round-trip with the receiver's reconstruction (packet_dispatch.rs).
        assert_eq!(u32::from_le_bytes([r[0], r[1], r[2], r[3]]), 0x030201);
    }

    #[test]
    fn send_fw_byte_serves_running_word_le() {
        // response[4..8] must equal read_running_fw(address).to_le_bytes().
        let hal = MockHal::new();
        hal.running_fw.set(0xDEAD_BEEF);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let r = send_fw_byte(&state, &hal, 0x100).unwrap();
        assert_eq!(&r[4..8], &[0xEF, 0xBE, 0xAD, 0xDE]);
        assert_eq!(&r[0..4], &[0x00, 0x01, 0x00, 0x00]); // 0x100 little-endian echo
    }

    #[test]
    fn abort_then_restart_reinitializes_crc() {
        // Abort (sequence mismatch) leaves a stale CRC; a fresh trigger via the
        // real handler+apply glue must re-seed checksum to 0xFFFFFFFF (no carry-over).
        use crate::domain::msg_handlers::{handle_simple_msg, apply_action, HandlerAction};
        use crate::domain::constants::PacketType;
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };

        state.fw.fw.address = 100;
        state.fw.fw.checksum = 0x1234_5678;
        state.fw.fw.upgrade_in_progress = true;
        state.fw._running_fw.version = 100;

        assert!(!receive_fw_byte(&mut state, &hal, 200, &[0; 4]));
        assert_eq!(state.fw.fw.address, 0);
        assert!(!state.fw.fw.upgrade_in_progress);
        assert_eq!(state.fw.fw.checksum, 0x1234_5678); // abort does NOT clear CRC

        let data = [200u8, 0, 0, 0, 0, 0, 0, 0]; // newer peer version
        let action = handle_simple_msg(PacketType::Heartbeat as u8, &data, &state);
        assert!(matches!(action, HandlerAction::StartFwUpgrade(_)));
        apply_action(&action, &mut state);

        assert_eq!(state.fw.fw.checksum, 0xFFFF_FFFF); // restart re-seeds — no stale carry-over
        assert_eq!(state.fw.fw.address, 0);
        assert!(state.fw.fw.upgrade_in_progress);
    }
}
