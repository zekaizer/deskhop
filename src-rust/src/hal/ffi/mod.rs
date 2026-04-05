// FFI exports — all #[no_mangle] pub extern "C" functions callable from C.
// Split into sub-modules by domain for maintainability.

mod callbacks;
mod config;
pub(crate) mod tasks;
mod util;

// Passthrough HAL helpers — accessed by PicoHal::PassthroughHal impl.
// The actual state lives in tasks.rs as static globals.

use crate::domain::passthrough::MAX_CONFIG_DESC_SIZE;

static mut PT_CONFIG_DESC: [u8; MAX_CONFIG_DESC_SIZE] = [0; MAX_CONFIG_DESC_SIZE];
static mut PT_CONFIG_DESC_LEN: u16 = 0;

/// Build config descriptor from captured passthrough interfaces.
pub(crate) unsafe fn passthrough_hal_build_config_desc() -> bool {
    let pt = tasks::get_pt_state();
    let buf_ptr = core::ptr::addr_of_mut!(PT_CONFIG_DESC).cast::<u8>();
    let len_ptr = core::ptr::addr_of_mut!(PT_CONFIG_DESC_LEN);
    super::device::hal_passthrough_build_config_desc(
        buf_ptr, len_ptr,
        pt.ifaces.as_ptr() as *const u8, pt.iface_count,
    );
    *len_ptr > 0
}

/// Clear the config descriptor buffer.
pub(crate) unsafe fn passthrough_hal_clear_config_desc() {
    *core::ptr::addr_of_mut!(PT_CONFIG_DESC_LEN) = 0;
}

/// Get config descriptor pointer and length (for C FFI accessors).
#[allow(dead_code)]
pub(crate) unsafe fn pt_config_desc_ptr() -> (*const u8, u16) {
    let ptr = core::ptr::addr_of!(PT_CONFIG_DESC).cast::<u8>();
    let len = *core::ptr::addr_of!(PT_CONFIG_DESC_LEN);
    (ptr, len)
}
