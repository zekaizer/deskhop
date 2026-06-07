// FFI exports — all #[no_mangle] pub extern "C" functions callable from C.
// Split into sub-modules by domain for maintainability.

mod callbacks;
mod config;
pub(crate) mod tasks;
mod util;

// Passthrough HAL helpers — accessed by PicoHal::PassthroughHal impl.
// The actual state lives in tasks.rs as static globals.

use crate::domain::passthrough::{MAX_CONFIG_DESC_SIZE, MAX_PASSTHROUGH_IFACES};

static mut PT_CONFIG_DESC: [u8; MAX_CONFIG_DESC_SIZE] = [0; MAX_CONFIG_DESC_SIZE];
static mut PT_CONFIG_DESC_LEN: u16 = 0;

extern "C" {
    // Report-descriptor sizes of the two DeskHop base HID interfaces (built with
    // TinyUSB macros in usb_descriptors.c, so their sizes stay SDK-owned).
    static desc_hid_report_size: u16;
    static desc_hid_report_relmouse_size: u16;
}

/// Build the passthrough composite config descriptor from the captured
/// interfaces. The assembly (interface/endpoint numbering, CFG_TUD_HID
/// brick-guard, wTotalLength) is the unit-tested domain::usb_config_desc; this
/// only gathers the inputs and owns the output buffer.
pub(crate) unsafe fn passthrough_hal_build_config_desc() -> bool {
    let pt = tasks::get_pt_state();

    // Snapshot (protocol, report_desc_len) for each captured passthrough iface.
    let mut ifaces = [(0u8, 0u16); MAX_PASSTHROUGH_IFACES];
    let n = (pt.iface_count as usize).min(MAX_PASSTHROUGH_IFACES);
    for (slot, src) in ifaces[..n].iter_mut().zip(pt.ifaces.iter()) {
        *slot = (src.itf_protocol, src.desc_len);
    }

    let base = [desc_hid_report_size, desc_hid_report_relmouse_size];
    let buf = &mut *core::ptr::addr_of_mut!(PT_CONFIG_DESC);
    let len = crate::domain::usb_config_desc::build_config_desc(
        buf,
        base,
        &ifaces[..n],
        cfg!(feature = "dh_debug"),
    );
    *core::ptr::addr_of_mut!(PT_CONFIG_DESC_LEN) = len as u16;
    len > 0
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
