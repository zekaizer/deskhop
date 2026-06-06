// Config business logic — flash config load/save/init, pure domain functions.
// No FFI, no raw pointers. ConfigFlash trait abstracts hardware access.

use crate::domain::crc;
use crate::domain::structs::{Config, FLASH_PAGE_SIZE};

pub const MAGIC_HEADER: u32 = 0xB00B1E5;
pub const CURRENT_CONFIG_VERSION: u32 = 8;
pub const CONFIG_MODE_TIMEOUT: u64 = 300_000_000;

/// Abstraction for flash read/write operations on the config sector.
pub trait ConfigFlash {
    fn flash_read_config(&self, buf: &mut [u8]);
    fn flash_write_config(&self, buf: &[u8]);
}

/// Initialize DeviceConfig fields from SDK-probed values.
pub fn init_config(
    cfg: &mut super::structs::DeviceConfig,
    config_mode_active: bool,
    board_role: u8,
    timestamp: u64,
) {
    cfg.config_mode_active = config_mode_active;
    cfg.board_role = board_role;
    cfg.core1_last_loop_pass = timestamp;
}

/// Validate a config byte buffer against magic, version, and CRC.
/// Returns true if the config is valid.
pub fn validate_config(config_bytes: &[u8], config: &Config) -> bool {
    let size = core::mem::size_of::<Config>();
    if config_bytes.len() < size { return false; }
    let cs = crc::calc_crc32(&config_bytes[..checksum_offset()]);
    config.magic_header == MAGIC_HEADER
        && config.checksum == cs
        && config.version == CURRENT_CONFIG_VERSION
}

/// Byte offset of the `checksum` field. NOT `size_of - 4`: `Config` is
/// 8-byte aligned (it has u64 members), so `checksum` (the last declared
/// field, a u32) is followed by 4 bytes of trailing padding. The CRC must
/// cover everything BEFORE the checksum field; using `size - 4` would fold the
/// checksum into its own input, so a freshly saved config could never validate
/// and every boot fell back to defaults (config never persisted).
fn checksum_offset() -> usize {
    core::mem::offset_of!(Config, checksum)
}

/// Compute CRC32 checksum for config (over the bytes preceding the checksum
/// field; trailing padding and the checksum itself are excluded).
pub fn compute_config_checksum(config_bytes: &[u8]) -> u32 {
    crc::calc_crc32(&config_bytes[..checksum_offset()])
}

/// Reset config mode timer to now + timeout.
pub fn reset_config_timer(cfg: &mut super::structs::DeviceConfig, now_us: u64) {
    cfg.config_mode_timer = now_us + CONFIG_MODE_TIMEOUT;
}

/// Load config from flash into the provided byte buffer, validate, and
/// replace with default if invalid.
///
/// # Safety
/// `config_bytes` must be a valid mutable byte view over a `Config` struct.
/// The caller (FFI layer) is responsible for the pointer-to-slice conversion.
pub fn load_config_from_bytes(
    config_bytes: &mut [u8],
    config: &Config,
    hal: &impl ConfigFlash,
    default_config: &Config,
) -> Option<Config> {
    hal.flash_read_config(config_bytes);

    if validate_config(config_bytes, config) {
        None // config is valid, no replacement needed
    } else {
        Some(*default_config)
    }
}

/// Prepare a flash page for writing: compute checksum, copy config to page.
/// Returns the page buffer ready for flash_write_config.
pub fn prepare_save_page(config_bytes: &[u8]) -> [u8; FLASH_PAGE_SIZE] {
    let size = core::mem::size_of::<Config>();
    let mut page = [0u8; FLASH_PAGE_SIZE];
    page[..size].copy_from_slice(&config_bytes[..size]);
    page
}
