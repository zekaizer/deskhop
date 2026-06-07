// Passthrough (Semi-DDM) composite USB configuration descriptor assembly.
//
// Builds the configuration descriptor we re-present to the host once upstream
// HID interfaces have been captured: two DeskHop base HID interfaces, one HID
// interface per captured passthrough interface, and (DH_DEBUG only) a CDC
// debug interface. Previously hand-assembled in C (hal_passthrough_build_config_desc)
// using TinyUSB's TUD_HID_DESCRIPTOR / TUD_CDC_DESCRIPTOR macros; the per-block
// byte layouts are re-encoded here as const arrays so the interface/endpoint
// numbering, the CFG_TUD_HID brick-guard, offset tracking and wTotalLength
// backfill — the off-by-one-prone assembly logic — are unit-testable.

// ---- TinyUSB descriptor constants (mirror common/tusb_types.h + class headers) ----
const TUSB_DESC_CONFIGURATION: u8 = 0x02;
const TUSB_DESC_INTERFACE: u8 = 0x04;
const TUSB_DESC_ENDPOINT: u8 = 0x05;
const TUSB_DESC_INTERFACE_ASSOCIATION: u8 = 0x0B;
const TUSB_DESC_CS_INTERFACE: u8 = 0x24;
const TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP: u8 = 1 << 5;

const TUSB_CLASS_CDC: u8 = 2;
const TUSB_CLASS_HID: u8 = 3;
const TUSB_CLASS_CDC_DATA: u8 = 10;

const TUSB_XFER_BULK: u8 = 2;
const TUSB_XFER_INTERRUPT: u8 = 3;

const HID_SUBCLASS_BOOT: u8 = 1;
const HID_DESC_TYPE_HID: u8 = 0x21;
const HID_DESC_TYPE_REPORT: u8 = 0x22;

const CDC_COMM_SUBCLASS_ACM: u8 = 0x02;
const CDC_COMM_PROTOCOL_NONE: u8 = 0x00;
const CDC_FUNC_DESC_HEADER: u8 = 0x00;
const CDC_FUNC_DESC_CALL_MANAGEMENT: u8 = 0x01;
const CDC_FUNC_DESC_ACM: u8 = 0x02;
const CDC_FUNC_DESC_UNION: u8 = 0x06;

// ---- DeskHop layout constants ----
const ITF_NUM_HID: u8 = 0;
const ITF_NUM_HID_REL_M: u8 = 1;
const ITF_NUM_PT_BASE: u8 = 2;
const EPNUM_PT_BASE: u8 = 0x83;
const CFG_TUD_HID: usize = 8;
const HID_EP_BUFSIZE: u16 = 32; // CFG_TUD_HID_EP_BUFSIZE
const CDC_EP_BUFSIZE: u16 = 64;
const CDC_NOTIF_SIZE: u16 = 8;

// String-descriptor indices (see usb_descriptors.c STRID_* enum).
const STRID_PRODUCT: u8 = 2;
const STRID_MOUSE: u8 = 4;
const STRID_DEBUG: u8 = 7;

/// TUD_HID_DESC_LEN (interface 9 + HID 9 + endpoint 7).
const HID_DESC_LEN: usize = 25;
/// TUD_CDC_DESC_LEN (8 + 9 + 5 + 5 + 4 + 5 + 7 + 9 + 7 + 7).
const CDC_DESC_LEN: usize = 66;
const CONFIG_HDR_LEN: usize = 9;

/// Append one HID interface descriptor block (TUD_HID_DESCRIPTOR) at `off`.
/// Returns the new offset, or None if it would overflow `buf`.
fn append_hid_itf(
    buf: &mut [u8],
    off: usize,
    itf_num: u8,
    str_idx: u8,
    protocol: u8,
    report_desc_len: u16,
    ep_in: u8,
) -> Option<usize> {
    let subclass = if protocol != 0 { HID_SUBCLASS_BOOT } else { 0 };
    let rl = report_desc_len.to_le_bytes();
    let eps = HID_EP_BUFSIZE.to_le_bytes();
    let d: [u8; HID_DESC_LEN] = [
        // Interface descriptor
        9, TUSB_DESC_INTERFACE, itf_num, 0, 1, TUSB_CLASS_HID, subclass, protocol, str_idx,
        // HID descriptor (bcdHID 0x0111, 1 report descriptor)
        9, HID_DESC_TYPE_HID, 0x11, 0x01, 0, 1, HID_DESC_TYPE_REPORT, rl[0], rl[1],
        // Endpoint IN (interrupt)
        7, TUSB_DESC_ENDPOINT, ep_in, TUSB_XFER_INTERRUPT, eps[0], eps[1], 1,
    ];
    write_block(buf, off, &d)
}

/// Append the CDC debug interface block (TUD_CDC_DESCRIPTOR) at `off`.
/// Returns the new offset, or None if it would overflow `buf`.
fn append_cdc(
    buf: &mut [u8],
    off: usize,
    itf_num: u8,
    str_idx: u8,
    ep_notif: u8,
    ep_out: u8,
    ep_in: u8,
) -> Option<usize> {
    let notif = CDC_NOTIF_SIZE.to_le_bytes();
    let eps = CDC_EP_BUFSIZE.to_le_bytes();
    let d: [u8; CDC_DESC_LEN] = [
        // Interface Association
        8, TUSB_DESC_INTERFACE_ASSOCIATION, itf_num, 2, TUSB_CLASS_CDC, CDC_COMM_SUBCLASS_ACM, CDC_COMM_PROTOCOL_NONE, 0,
        // CDC Control Interface
        9, TUSB_DESC_INTERFACE, itf_num, 0, 1, TUSB_CLASS_CDC, CDC_COMM_SUBCLASS_ACM, CDC_COMM_PROTOCOL_NONE, str_idx,
        // CDC Header (bcdCDC 0x0120)
        5, TUSB_DESC_CS_INTERFACE, CDC_FUNC_DESC_HEADER, 0x20, 0x01,
        // CDC Call Management (data interface = itf_num + 1)
        5, TUSB_DESC_CS_INTERFACE, CDC_FUNC_DESC_CALL_MANAGEMENT, 0, itf_num + 1,
        // CDC Abstract Control Management (line request + send break)
        4, TUSB_DESC_CS_INTERFACE, CDC_FUNC_DESC_ACM, 6,
        // CDC Union (control = itf_num, sub = itf_num + 1)
        5, TUSB_DESC_CS_INTERFACE, CDC_FUNC_DESC_UNION, itf_num, itf_num + 1,
        // Endpoint Notification (interrupt, interval 16)
        7, TUSB_DESC_ENDPOINT, ep_notif, TUSB_XFER_INTERRUPT, notif[0], notif[1], 16,
        // CDC Data Interface
        9, TUSB_DESC_INTERFACE, itf_num + 1, 0, 2, TUSB_CLASS_CDC_DATA, 0, 0, 0,
        // Endpoint OUT (bulk)
        7, TUSB_DESC_ENDPOINT, ep_out, TUSB_XFER_BULK, eps[0], eps[1], 0,
        // Endpoint IN (bulk)
        7, TUSB_DESC_ENDPOINT, ep_in, TUSB_XFER_BULK, eps[0], eps[1], 0,
    ];
    write_block(buf, off, &d)
}

#[inline]
fn write_block(buf: &mut [u8], off: usize, block: &[u8]) -> Option<usize> {
    let end = off.checked_add(block.len())?;
    if end > buf.len() {
        return None;
    }
    buf[off..end].copy_from_slice(block);
    Some(end)
}

/// Build the passthrough composite configuration descriptor into `buf`.
///
/// - `base_report_len`: report-descriptor lengths of the two DeskHop base HID
///   interfaces — `[0]` = main (kbd/abs-mouse/consumer/system), `[1]` = relative
///   mouse helper.
/// - `pt_ifaces`: `(bInterfaceProtocol, report_descriptor_len)` for each captured
///   passthrough interface.
/// - `with_cdc`: append the CDC debug interface (DH_DEBUG builds).
///
/// Returns the descriptor length, or 0 if it would not fit `buf` OR the device
/// HID-interface pool would overflow: TinyUSB's device HID class can claim at
/// most `CFG_TUD_HID` interfaces (CDC consumes none). Declaring more makes the
/// host's SET_CONFIGURATION stall and the device never mounts, so we refuse to
/// build it and the caller keeps the default DeskHop identity (recoverable via
/// the config-mode hotkey) instead of bricking.
pub fn build_config_desc(
    buf: &mut [u8],
    base_report_len: [u16; 2],
    pt_ifaces: &[(u8, u16)],
    with_cdc: bool,
) -> usize {
    let iface_count = pt_ifaces.len();

    // Brick-guard: 2 DeskHop base HID interfaces + one per passthrough interface.
    if 2 + iface_count > CFG_TUD_HID {
        return 0;
    }

    let mut num_itf = 2 + iface_count;
    if with_cdc {
        num_itf += 2; // CDC communication + data interfaces
    }

    // Configuration descriptor header; wTotalLength (bytes 2..4) backfilled below.
    let header: [u8; CONFIG_HDR_LEN] = [
        9,
        TUSB_DESC_CONFIGURATION,
        0,
        0,
        num_itf as u8,
        1, // bConfigurationValue
        0, // iConfiguration
        0x80 | TUSB_DESC_CONFIG_ATT_REMOTE_WAKEUP,
        250, // bMaxPower: 500mA / 2
    ];
    let mut off = match write_block(buf, 0, &header) {
        Some(o) => o,
        None => return 0,
    };

    // DeskHop base interfaces: main HID then relative-mouse helper.
    off = match append_hid_itf(buf, off, ITF_NUM_HID, STRID_PRODUCT, 0, base_report_len[0], 0x81) {
        Some(o) => o,
        None => return 0,
    };
    off = match append_hid_itf(buf, off, ITF_NUM_HID_REL_M, STRID_MOUSE, 0, base_report_len[1], 0x82) {
        Some(o) => o,
        None => return 0,
    };

    // Captured passthrough interfaces.
    for (i, &(protocol, report_len)) in pt_ifaces.iter().enumerate() {
        let i = i as u8;
        off = match append_hid_itf(buf, off, ITF_NUM_PT_BASE + i, 0, protocol, report_len, EPNUM_PT_BASE + i) {
            Some(o) => o,
            None => return 0,
        };
    }

    // CDC debug interface (endpoints follow the passthrough endpoints).
    if with_cdc {
        let ic = iface_count as u8;
        let cdc_itf = ITF_NUM_PT_BASE + ic;
        let ep_notif = 0x80 | (3 + ic);
        let ep_out = 3 + ic + 1;
        let ep_in = 0x80 | (3 + ic + 1);
        off = match append_cdc(buf, off, cdc_itf, STRID_DEBUG, ep_notif, ep_out, ep_in) {
            Some(o) => o,
            None => return 0,
        };
    }

    // Backfill wTotalLength.
    buf[2] = (off & 0xFF) as u8;
    buf[3] = (off >> 8) as u8;
    off
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte offsets within a 25-byte HID block, asserted independently of the
    // builder against the USB HID descriptor structure.
    fn assert_hid_block(b: &[u8], itf: u8, str_idx: u8, protocol: u8, report_len: u16, ep: u8) {
        // Interface descriptor
        assert_eq!(b[0], 9, "bLength");
        assert_eq!(b[1], 0x04, "INTERFACE");
        assert_eq!(b[2], itf, "bInterfaceNumber");
        assert_eq!(b[4], 1, "bNumEndpoints");
        assert_eq!(b[5], 0x03, "HID class");
        assert_eq!(b[6], if protocol != 0 { 1 } else { 0 }, "boot subclass");
        assert_eq!(b[7], protocol, "bInterfaceProtocol");
        assert_eq!(b[8], str_idx, "iInterface");
        // HID descriptor
        assert_eq!(b[9], 9);
        assert_eq!(b[10], 0x21, "HID desc type");
        assert_eq!(b[15], 0x22, "REPORT desc type");
        assert_eq!(u16::from_le_bytes([b[16], b[17]]), report_len, "report desc len LE");
        // Endpoint
        assert_eq!(b[18], 7);
        assert_eq!(b[19], 0x05, "ENDPOINT");
        assert_eq!(b[20], ep, "bEndpointAddress");
        assert_eq!(b[21], 0x03, "interrupt");
        assert_eq!(u16::from_le_bytes([b[22], b[23]]), 32, "ep size");
    }

    #[test]
    fn one_passthrough_no_cdc_layout() {
        let mut buf = [0u8; 280];
        let n = build_config_desc(&mut buf, [100, 50], &[(1, 200)], false);
        // header + 3 HID blocks
        assert_eq!(n, CONFIG_HDR_LEN + 3 * HID_DESC_LEN); // 9 + 75 = 84

        // Config header
        assert_eq!(buf[0], 9);
        assert_eq!(buf[1], 0x02, "CONFIGURATION");
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), n as u16, "wTotalLength");
        assert_eq!(buf[4], 3, "bNumInterfaces (2 base + 1 pt)");
        assert_eq!(buf[5], 1, "bConfigurationValue");
        assert_eq!(buf[7], 0x80 | 0x20, "attrs: bus-powered + remote wakeup");
        assert_eq!(buf[8], 250, "bMaxPower");

        // ITF0 main HID, ITF1 rel mouse, pt iface 0
        assert_hid_block(&buf[9..34], ITF_NUM_HID, STRID_PRODUCT, 0, 100, 0x81);
        assert_hid_block(&buf[34..59], ITF_NUM_HID_REL_M, STRID_MOUSE, 0, 50, 0x82);
        assert_hid_block(&buf[59..84], ITF_NUM_PT_BASE, 0, 1, 200, EPNUM_PT_BASE);
    }

    #[test]
    fn brick_guard_refuses_too_many_hid_interfaces() {
        let mut buf = [0u8; 280];
        // 2 base + 7 passthrough = 9 HID interfaces > CFG_TUD_HID (8) → refuse.
        let seven = [(0u8, 30u16); 7];
        assert_eq!(build_config_desc(&mut buf, [10, 10], &seven, false), 0);
        // 2 base + 6 passthrough = 8 == CFG_TUD_HID → allowed.
        let six = [(0u8, 30u16); 6];
        assert_ne!(build_config_desc(&mut buf, [10, 10], &six, false), 0);
    }

    #[test]
    fn endpoint_addresses_are_sequential() {
        let mut buf = [0u8; 280];
        build_config_desc(&mut buf, [10, 10], &[(0, 30), (0, 30), (0, 30)], false);
        // Base eps 0x81, 0x82; passthrough eps 0x83, 0x84, 0x85.
        assert_eq!(buf[9 + 20], 0x81);
        assert_eq!(buf[34 + 20], 0x82);
        assert_eq!(buf[59 + 20], 0x83);
        assert_eq!(buf[84 + 20], 0x84);
        assert_eq!(buf[109 + 20], 0x85);
    }

    #[test]
    fn with_cdc_appends_iad_and_endpoints() {
        let mut buf = [0u8; 280];
        let n = build_config_desc(&mut buf, [10, 10], &[(0, 30)], true);
        // header + 3 HID + CDC
        assert_eq!(n, CONFIG_HDR_LEN + 3 * HID_DESC_LEN + CDC_DESC_LEN); // 84 + 66 = 150
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), n as u16);
        assert_eq!(buf[4], 5, "bNumInterfaces (2 base + 1 pt + 2 CDC)");

        // CDC block (66 bytes) starts at 84. Sub-block offsets within it:
        // IAD 0, ctrl-itf 8, header 17, call 22, acm 27, union 31, ep-notif 36,
        // data-itf 43, ep-out 52, ep-in 59.
        let cdc = &buf[84..150];
        assert_eq!(cdc[0], 8, "IAD bLength");
        assert_eq!(cdc[1], 0x0B, "INTERFACE_ASSOCIATION");
        // CDC interface number = ITF_NUM_PT_BASE + iface_count = 2 + 1 = 3.
        assert_eq!(cdc[2], 3, "IAD bFirstInterface");
        // EP Notification descriptor (block offset 36).
        assert_eq!(cdc[36], 7);
        assert_eq!(cdc[37], 0x05, "ENDPOINT");
        // Notification endpoint = 0x80 | (3 + iface_count) = 0x84, interrupt.
        assert_eq!(cdc[38], 0x84, "notif ep");
        assert_eq!(cdc[39], 0x03, "interrupt");
        // Data OUT endpoint (block offset 52) = (3 + 1 + 1) = 5, bulk.
        assert_eq!(cdc[54], 0x05, "data out ep");
        assert_eq!(cdc[55], 0x02, "bulk");
        // Data IN endpoint (block offset 59) = 0x80 | 5 = 0x85.
        assert_eq!(cdc[61], 0x85, "data in ep");
    }

    #[test]
    fn zero_passthrough_still_builds_base_only() {
        let mut buf = [0u8; 280];
        let n = build_config_desc(&mut buf, [120, 60], &[], false);
        assert_eq!(n, CONFIG_HDR_LEN + 2 * HID_DESC_LEN); // 59
        assert_eq!(buf[4], 2, "two base interfaces");
        assert_hid_block(&buf[9..34], ITF_NUM_HID, STRID_PRODUCT, 0, 120, 0x81);
        assert_hid_block(&buf[34..59], ITF_NUM_HID_REL_M, STRID_MOUSE, 0, 60, 0x82);
    }

    #[test]
    fn too_small_buffer_returns_zero() {
        let mut buf = [0u8; 40]; // only fits header + one HID block
        assert_eq!(build_config_desc(&mut buf, [10, 10], &[], false), 0);
    }
}
