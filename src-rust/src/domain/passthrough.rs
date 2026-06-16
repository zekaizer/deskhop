// Semi-DDM USB Passthrough — descriptor capture, HID++ protocol, and state management.
// Pure domain logic, no HAL dependencies.

use crate::domain::structs::MouseReportC;

// ================================================================
// Constants
// ================================================================

pub const MAX_PASSTHROUGH_IFACES: usize = 6;
pub const MAX_HID_DESC_SIZE: usize = 512;
pub const MAX_CONFIG_DESC_SIZE: usize = 280;

/// Device-side passthrough interface base (after DeskHop's ITF 0, 1)
pub const ITF_NUM_PT_BASE: u8 = 2;
/// Device-side passthrough endpoint base (IN direction)
pub const EPNUM_PT_BASE: u8 = 0x83;

/// HID++ protocol constants
pub const HIDPP_SWID_DESKHOP: u8 = 0x0F;
pub const HIDPP_REPORT_ID_SHORT: u8 = 0x10;
pub const HIDPP_REPORT_ID_LONG: u8 = 0x11;

/// SmartShift button CID — used for double-click detection on MX Master.
pub const SMARTSHIFT_CID: u8 = 0xC4;

/// MX gesture/thumb button CID. Diverted by Options+ and surfaced as a
/// divertedButtonsEvent (fn=0) bitmap entry; the canonical definition lives here
/// (the lower module) and is re-exported by domain::hidpp_keymap.
pub const GESTURE_CID: u8 = 0xC3;

/// Maximum buffered events while in SmartShift double-click pending state.
/// 8 entries is enough for the typical Down1/Up1/Down2/Up2 plus a small
/// burst of unrelated HID++ events (scroll, etc.) within the window.
pub const SMARTSHIFT_BUF_SIZE: usize = 8;

/// Per-event capacity in the SmartShift buffer (matches HID++ short/long
/// report sizes; long is 20 bytes plus a small margin).
pub const SMARTSHIFT_BUF_ENTRY_CAP: usize = 32;

/// HID interface protocol: vendor/HID++ (always passthrough)
const HID_ITF_PROTOCOL_NONE: u8 = 0;

/// HiRes scroll/thumbwheel normalization divisor.
/// ~6-7 events/notch × delta~2 = ~12-14 per notch; divide by 12 → ~1 tick/notch.
const HIRES_SCROLL_DIVISOR: i16 = 12;

// ================================================================
// Types
// ================================================================

#[derive(Clone, Copy)]
pub struct PassthroughIface {
    pub dev_addr: u8,
    pub instance: u8,
    pub itf_protocol: u8,
    pub desc_len: u16,
    pub desc: [u8; MAX_HID_DESC_SIZE],
    pub always_passthrough: bool,
}

impl Default for PassthroughIface {
    fn default() -> Self {
        Self {
            dev_addr: 0,
            instance: 0,
            itf_protocol: 0,
            desc_len: 0,
            desc: [0; MAX_HID_DESC_SIZE],
            always_passthrough: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct HidppDiscovery {
    pub device_idx: u8,
    pub fi_reprog_controls: u8,
    pub fi_hires_scroll: u8,
    pub fi_thumbwheel: u8,
    pub button_state: u8,
    pub wheel_acc: i16,
    pub pan_acc: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum HidppScanState {
    #[default]
    Idle = 0,
    QueryIRoot = 1,
    Done = 2,
}

#[derive(Clone, Copy, Default)]
pub struct HidppScan {
    pub state: HidppScanState,
    pub device_idx: u8,
    pub next_device: u8,
    pub query_idx: u8,
    pub feature_count: u8,
    pub query_sent_us: u64,
    pub raw_dump_enabled: bool,
    pub pipe_debug_enabled: bool,
}

/// One buffered HID++ event awaiting flush by the SmartShift double-click
/// state machine. Holds enough metadata to replay via send_hid_report.
#[derive(Clone, Copy)]
pub struct SmartShiftBufEntry {
    pub dev_inst: u8,
    pub len: u8,
    pub data: [u8; SMARTSHIFT_BUF_ENTRY_CAP],
}

impl Default for SmartShiftBufEntry {
    fn default() -> Self {
        Self { dev_inst: 0, len: 0, data: [0; SMARTSHIFT_BUF_ENTRY_CAP] }
    }
}

#[derive(Clone, Copy, Default)]
pub struct PassthroughOutQueue {
    pub dev_addr: u8,
    pub instance: u8,
    pub report_id: u8,
    pub report_type: u8,
    pub data: [u8; 32],
    pub len: u16,
    pub pending: bool,
}

#[derive(Clone, Copy)]
pub struct PassthroughState {
    pub iface_count: u8,
    pub ifaces: [PassthroughIface; MAX_PASSTHROUGH_IFACES],

    pub active: bool,
    pub last_capture_us: u64,
    pub reconnect_at_us: u64,

    /// Set when the boot-time fallback fires after the absolute timeout.
    /// Locks out late vendor captures from triggering a disconnect/reconnect
    /// cycle — passthrough activation is deferred to the next reboot.
    pub gave_up: bool,

    // Upstream device identity for VID/PID spoofing
    pub upstream_vid: u16,
    pub upstream_pid: u16,

    // Deferred HID++ output report
    pub out_queue: PassthroughOutQueue,

    // HID++ feature discovery (learned from observed events)
    pub hidpp_disc: HidppDiscovery,

    // HID++ feature scan (debug hotkey)
    pub hidpp_scan: HidppScan,

    // SmartShift double-click state machine.
    /// Timestamp of the first SmartShift press currently being deferred.
    /// 0 = idle (not in double-click window).
    pub smartshift_window_us: u64,
    pub smartshift_buf_count: u8,
    pub smartshift_buf: [SmartShiftBufEntry; SMARTSHIFT_BUF_SIZE],
    /// Number of subsequent SmartShift events to consume after a switch
    /// trigger (so the release of the second press isn't forwarded as an
    /// orphan event).
    pub smartshift_consume: u8,

    /// Set by on_device_unmount (Core1) to request the device-side
    /// disconnect/reconnect cycle. The actual tud_disconnect is performed by
    /// passthrough_task on Core0 — Core1 must never touch the device stack.
    pub disconnect_requested: bool,

    /// Edge-detect state for the HID++->key remap: whether the gesture button
    /// (CID 0xC3) was present in the last divertedButtons bitmap. See
    /// domain::hidpp_keymap.
    pub gesture_pressed: bool,

    /// Timestamp of the current gesture-button press (us). Set on the press
    /// edge; on release the held duration classifies the gesture as a tap
    /// (Alt+Tab) or a hold (app drawer). 0 = no press in progress. See
    /// domain::hidpp_keymap.
    pub gesture_press_us: u64,

    /// Debug: last forwarded mouse button byte, for button-transition logging
    /// (pointer movement is not logged). DH_DEBUG observability only.
    pub dbg_last_buttons: u8,

    /// Debug: rate-limited accumulators for the high-frequency wheel /
    /// thumbwheel / pointer streams (logging every event would flood the
    /// peer_log ring). DH_DEBUG observability only.
    pub dbg_stream: DbgStreamLog,
}

/// DH_DEBUG stream-summary accumulators. Each stream sums its delta + event
/// count and is flushed to the log at most once per interval (see
/// passthrough_service), so continuous scrolling/movement is summarized rather
/// than logged per event.
#[derive(Clone, Copy, Default)]
pub struct DbgStreamLog {
    pub wheel_sum: i32,
    pub wheel_n: u16,
    pub wheel_last_us: u64,
    pub thumb_sum: i32,
    pub thumb_n: u16,
    pub thumb_last_us: u64,
    pub ptr_dx: i32,
    pub ptr_dy: i32,
    pub ptr_n: u16,
    pub ptr_last_us: u64,
}

impl Default for PassthroughState {
    fn default() -> Self {
        Self {
            iface_count: 0,
            ifaces: [PassthroughIface::default(); MAX_PASSTHROUGH_IFACES],
            active: false,
            last_capture_us: 0,
            reconnect_at_us: 0,
            gave_up: false,
            upstream_vid: 0,
            upstream_pid: 0,
            out_queue: PassthroughOutQueue::default(),
            hidpp_disc: HidppDiscovery::default(),
            hidpp_scan: HidppScan::default(),
            smartshift_window_us: 0,
            smartshift_buf_count: 0,
            smartshift_buf: [SmartShiftBufEntry::default(); SMARTSHIFT_BUF_SIZE],
            smartshift_consume: 0,
            disconnect_requested: false,
            gesture_pressed: false,
            gesture_press_us: 0,
            dbg_last_buttons: 0,
            dbg_stream: DbgStreamLog::default(),
        }
    }
}

// ================================================================
// Core functions
// ================================================================

/// Initialize passthrough state to zero.
pub fn init(state: &mut PassthroughState) {
    *state = PassthroughState::default();
}

/// Capture a HID report descriptor from a mounted host device.
/// Returns true if successfully captured, false on error or overflow.
pub fn capture_descriptor(
    state: &mut PassthroughState,
    dev_addr: u8,
    instance: u8,
    itf_protocol: u8,
    desc: &[u8],
) -> bool {
    if desc.is_empty() || desc.len() > MAX_HID_DESC_SIZE {
        return false;
    }

    // Check for duplicate (same dev_addr + instance) and overwrite
    let mut slot_idx = None;
    for i in 0..state.iface_count as usize {
        if state.ifaces[i].dev_addr == dev_addr && state.ifaces[i].instance == instance {
            slot_idx = Some(i);
            break;
        }
    }

    // No existing entry — allocate new slot
    let idx = match slot_idx {
        Some(i) => i,
        None => {
            if state.iface_count as usize >= MAX_PASSTHROUGH_IFACES {
                return false;
            }
            let i = state.iface_count as usize;
            state.iface_count += 1;
            i
        }
    };

    let iface = &mut state.ifaces[idx];
    iface.dev_addr = dev_addr;
    iface.instance = instance;
    iface.itf_protocol = itf_protocol;
    iface.desc_len = desc.len() as u16;
    iface.desc[..desc.len()].copy_from_slice(desc);
    iface.always_passthrough = itf_protocol == HID_ITF_PROTOCOL_NONE;

    true
}

/// Remove all interfaces belonging to the given device address.
/// Compacts the array and resets passthrough state when no interfaces remain.
pub fn remove_device(state: &mut PassthroughState, dev_addr: u8) {
    let mut write = 0usize;
    for read in 0..state.iface_count as usize {
        if state.ifaces[read].dev_addr != dev_addr {
            if write != read {
                state.ifaces[write] = state.ifaces[read];
            }
            write += 1;
        }
    }

    // Zero out vacated slots
    for i in write..state.iface_count as usize {
        state.ifaces[i] = PassthroughIface::default();
    }

    state.iface_count = write as u8;

    // Reset passthrough state when no interfaces remain
    if state.iface_count == 0 {
        state.upstream_vid = 0;
        state.upstream_pid = 0;
        state.active = false;
        state.hidpp_disc = HidppDiscovery::default();
    }
}

/// Activate passthrough mode. Returns false if no interfaces captured
/// or config descriptor was not built.
/// Caller must invoke HAL build_config_desc before calling this and
/// pass its return value as `config_desc_ready`.
pub fn activate(state: &mut PassthroughState, config_desc_ready: bool) -> bool {
    if state.iface_count == 0 {
        return false;
    }

    if !config_desc_ready {
        return false;
    }

    state.active = true;
    true
}

/// Get the HID report descriptor for a device-side passthrough instance.
/// Returns (descriptor slice, length) or None if instance is invalid.
pub fn get_report_desc(state: &PassthroughState, device_instance: u8) -> Option<(&[u8], u16)> {
    if device_instance < ITF_NUM_PT_BASE {
        return None;
    }

    let idx = (device_instance - ITF_NUM_PT_BASE) as usize;
    if idx >= state.iface_count as usize {
        return None;
    }

    let len = state.ifaces[idx].desc_len;
    Some((&state.ifaces[idx].desc[..len as usize], len))
}

/// Returns true if at least one captured interface is a vendor/HID++ interface.
/// Used to gate passthrough activation — standard keyboard/mouse don't need it.
pub fn has_vendor_interface(state: &PassthroughState) -> bool {
    for i in 0..state.iface_count as usize {
        if state.ifaces[i].always_passthrough {
            return true;
        }
    }
    false
}

/// Map a host-side (dev_addr, instance) to a device-side instance number.
/// Returns None if not found.
pub fn host_to_device_instance(
    state: &PassthroughState,
    dev_addr: u8,
    instance: u8,
) -> Option<u8> {
    for i in 0..state.iface_count as usize {
        if state.ifaces[i].dev_addr == dev_addr && state.ifaces[i].instance == instance {
            return Some(ITF_NUM_PT_BASE + i as u8);
        }
    }
    None
}

/// Map a device-side instance number back to the host-side array index.
/// Returns None if out of range.
pub fn device_to_host_index(state: &PassthroughState, device_instance: u8) -> Option<u8> {
    if device_instance < ITF_NUM_PT_BASE {
        return None;
    }
    let idx = device_instance - ITF_NUM_PT_BASE;
    if idx >= state.iface_count {
        return None;
    }
    Some(idx)
}

// ================================================================
// HID++ Protocol
// ================================================================

/// Check if a raw HID++ report is an input event (sw_id == 0).
/// Returns false for protocol responses (sw_id != 0) or non-HID++ reports.
pub fn is_hidpp_input_event(report: &[u8]) -> bool {
    if report.len() < 4 {
        return false;
    }
    if report[0] != HIDPP_REPORT_ID_SHORT && report[0] != HIDPP_REPORT_ID_LONG {
        return false;
    }
    (report[3] & 0x0F) == 0
}

/// True if a divertedButtonsEvent (fn=0) bitmap contains the given CID (low
/// byte, usage page 0x00). The bitmap is report[4..], laid out as (cid_hi,
/// cid_lo) pairs.
pub fn diverted_has_cid(report: &[u8], cid_lo: u8) -> bool {
    let mut i = 4;
    while i + 1 < report.len() {
        if report[i] == 0x00 && report[i + 1] == cid_lo {
            return true;
        }
        i += 2;
    }
    false
}

/// True if a divertedButtonsEvent (fn=0) bitmap contains the SmartShift CID.
fn diverted_has_smartshift(report: &[u8]) -> bool {
    diverted_has_cid(report, SMARTSHIFT_CID)
}

/// Returns true if the report is a SmartShift button-DOWN event (CID 0xC4).
/// Handles both HID++ shapes Logitech mice emit (the exact one depends on how
/// the button is configured/diverted by Options+):
/// - analyticsKeyEvent (fn=2): `[.., fn|sw, 0x00, cid_lo, action]` — down when action!=0.
/// - divertedButtonsEvent (fn=0): `[.., fn|sw, cid_hi, cid_lo, ...]` bitmap — down when 0xC4 present.
///
/// Caller must already know the report is a HID++ input event.
pub fn is_smartshift_press(report: &[u8]) -> bool {
    if report.len() < 7 { return false; }
    match (report[3] >> 4) & 0x0F {
        2 => report[4] == 0x00 && report[5] == SMARTSHIFT_CID && report[6] != 0,
        0 => diverted_has_smartshift(report),
        _ => false,
    }
}

/// Returns true if the report is a SmartShift button-UP event. For fn=2 this is
/// the action=0 event for CID 0xC4; for fn=0 it is a divertedButtons bitmap that
/// no longer contains 0xC4 (only checked in the post-trigger consume window).
pub fn is_smartshift_release(report: &[u8]) -> bool {
    if report.len() < 7 { return false; }
    match (report[3] >> 4) & 0x0F {
        2 => report[4] == 0x00 && report[5] == SMARTSHIFT_CID && report[6] == 0,
        0 => !diverted_has_smartshift(report),
        _ => false,
    }
}

/// Append an event to the SmartShift buffer. Returns false if the buffer
/// is full (caller should flush + abandon double-click detection).
pub fn smartshift_buf_push(state: &mut PassthroughState, dev_inst: u8, report: &[u8]) -> bool {
    if state.smartshift_buf_count as usize >= SMARTSHIFT_BUF_SIZE { return false; }
    if report.len() > SMARTSHIFT_BUF_ENTRY_CAP { return false; }
    let idx = state.smartshift_buf_count as usize;
    let entry = &mut state.smartshift_buf[idx];
    entry.dev_inst = dev_inst;
    entry.len = report.len() as u8;
    entry.data[..report.len()].copy_from_slice(report);
    state.smartshift_buf_count += 1;
    true
}

/// Reset SmartShift state (idle the window, clear buffer, clear consume counter).
pub fn smartshift_reset(state: &mut PassthroughState) {
    state.smartshift_window_us = 0;
    state.smartshift_buf_count = 0;
    state.smartshift_consume = 0;
}

/// Accumulate a scroll delta and scale by HIRES_SCROLL_DIVISOR.
/// Returns Some(tick) if accumulated value crossed a notch boundary, None otherwise.
fn accumulate_and_scale(acc: &mut i16, delta: i16) -> Option<i8> {
    *acc = acc.saturating_add(delta);
    let raw = *acc / HIRES_SCROLL_DIVISOR;
    if raw == 0 {
        return None;
    }
    *acc -= raw * HIRES_SCROLL_DIVISOR;
    Some(raw.clamp(-128, 127) as i8)
}

/// Map a Logitech CID (low byte) to a mouse button bit.
/// Returns 0 if not a recognized mouse button CID.
pub fn cid_to_button_bit(cid_lo: u8) -> u8 {
    match cid_lo {
        0x50 => 0x01, // Left
        0x51 => 0x02, // Right
        0x52 => 0x04, // Middle
        0x53 => 0x08, // Back
        0x56 => 0x10, // Forward
        _ => 0,
    }
}

/// True if a divertedButtonsEvent bitmap (`params` = report[4..], laid out as
/// (cid_hi, cid_lo) pairs) carries a CID that only ReprogControls emits — a
/// standard button or the gesture/SmartShift key. Used to learn the feature
/// index from fn=0 events.
fn params_have_reprog_cid(params: &[u8]) -> bool {
    let mut i = 0;
    while i + 1 < params.len() {
        if params[i] == 0x00 {
            let cid = params[i + 1];
            if cid == GESTURE_CID || cid == SMARTSHIFT_CID || cid_to_button_bit(cid) != 0 {
                return true;
            }
        }
        i += 2;
    }
    false
}

/// Auto-learn ReprogControls feature index from observed button events (sw_id=0).
/// Called when fi_reprog_controls is not yet known.
pub fn autolearn_feature(d: &mut HidppDiscovery, fi: u8, fn_: u8, params: &[u8]) {
    if fi == 0 || d.fi_reprog_controls == fi {
        return;
    }
    // fn=2 (analyticsKeyEvent) with a recognizable standard-button CID.
    if fn_ == 2 && params.len() >= 3 && params[0] == 0x00 && cid_to_button_bit(params[1]) != 0 {
        d.fi_reprog_controls = fi;
        return;
    }
    // fn=0 (divertedButtonsEvent) bitmap carrying a known ReprogControls CID.
    // Required for mice whose buttons Options+ diverts as fn=0 only — they never
    // emit the fn=2 analyticsKeyEvent the branch above relies on, so without this
    // fi_reprog_controls stays 0 and the gesture remap silently no-ops. Exclude
    // the HiRes-scroll / thumbwheel indices (their fn=0 deltas could alias a CID).
    if fn_ == 0 && fi != d.fi_hires_scroll && fi != d.fi_thumbwheel
        && params_have_reprog_cid(params)
    {
        d.fi_reprog_controls = fi;
    }
}

/// Sniff host-side output reports to learn HID++ feature indices.
/// Called from tud_hid_set_report_cb when report is HID++ short.
pub fn sniff_feature_indices(d: &mut HidppDiscovery, report: &[u8]) {
    if report.len() < 4 {
        return;
    }
    // report[0] = device_idx, report[1] = fi, report[2] = fn|sw_id, report[3..] = params
    let device_idx = report[0];
    let fi = report[1];
    let fn_ = (report[2] >> 4) & 0x0F;
    let sw_id = report[2] & 0x0F;
    let p0 = if report.len() > 3 { report[3] } else { 0 };

    // Only protocol requests (sw_id != 0)
    if sw_id == 0 {
        return;
    }

    // Detect setWheelMode(fn=2, p0=0x03) → HiResScroll
    if d.fi_hires_scroll == 0 && fn_ == 2 && p0 == 0x03 {
        d.fi_hires_scroll = fi;
        d.device_idx = device_idx;
    }

    // Detect setThumbwheelReporting(fn=2, p0=0x01) → Thumbwheel
    // Must already know HiResScroll and be a different feature index, same device
    if d.fi_thumbwheel == 0
        && d.fi_hires_scroll != 0
        && fn_ == 2
        && p0 == 0x01
        && fi != d.fi_hires_scroll
        && device_idx == d.device_idx
    {
        d.fi_thumbwheel = fi;
    }
}

/// Convert a HID++ input event to a standard mouse report.
/// Returns true if a mouse report was generated (scroll/pan),
/// false if only button_state was updated (caller merges separately).
pub fn convert_hidpp_to_mouse(
    state: &mut PassthroughState,
    report: &[u8],
    out_mouse: &mut MouseReportC,
) -> bool {
    if report.len() < 7 {
        return false;
    }

    let d = &mut state.hidpp_disc;
    let feature_idx = report[2];
    let fn_ = (report[3] >> 4) & 0x0F;
    let params = &report[4..];

    // Auto-learn ReprogControls from event patterns
    if d.fi_reprog_controls == 0 {
        autolearn_feature(d, feature_idx, fn_, params);
    }

    // ReprogControls V4 analyticsKeyEvent (fn=2):
    // [rid, dev, fi, fn|sw, 0x00, cid_lo, action, ...]
    if feature_idx == d.fi_reprog_controls && fn_ == 2 && params.len() >= 3 {
        let cid_lo = params[1];
        let action = params[2];

        let bit = cid_to_button_bit(cid_lo);
        if bit != 0 {
            if action != 0 {
                d.button_state |= bit;
            } else {
                d.button_state &= !bit;
            }
        }
        return false; // button_state updated; merged via output_mouse_report
    }

    // ReprogControls V4 divertedButtonsEvent (fn=0):
    // Bitmap of ALL currently pressed CIDs.
    if feature_idx == d.fi_reprog_controls && fn_ == 0 && params.len() >= 2 {
        let mut buttons: u8 = 0;
        let max_cids = (params.len() / 2).min(4);
        for i in 0..max_cids {
            let cid = ((params[i * 2] as u16) << 8) | params[i * 2 + 1] as u16;
            if cid == 0 {
                continue;
            }
            buttons |= cid_to_button_bit((cid & 0xFF) as u8);
        }
        d.button_state = buttons;
        return false; // button_state updated; merged via output_mouse_report
    }

    // HiResScroll event (fn=0)
    if d.fi_hires_scroll != 0 && feature_idx == d.fi_hires_scroll && fn_ == 0 {
        let delta_v = ((params[1] as i16) << 8) | params[2] as i16;
        if let Some(tick) = accumulate_and_scale(&mut d.wheel_acc, delta_v) {
            out_mouse.wheel = tick;
            return true;
        }
        return false;
    }

    // Thumbwheel event (fn=0)
    if d.fi_thumbwheel != 0 && feature_idx == d.fi_thumbwheel && fn_ == 0 {
        let delta_h = ((params[1] as i16) << 8) | params[2] as i16;
        if let Some(tick) = accumulate_and_scale(&mut d.pan_acc, delta_h) {
            out_mouse.pan = tick;
            return true;
        }
        return false;
    }

    false
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn new_state() -> PassthroughState {
        PassthroughState::default()
    }

    // -- Init tests --

    #[test]
    fn init_zeroes_state() {
        let mut state = new_state();
        state.iface_count = 3;
        state.active = true;
        init(&mut state);
        assert_eq!(state.iface_count, 0);
        assert!(!state.active);
    }

    // -- Descriptor capture tests --

    #[test]
    fn capture_single_descriptor() {
        let mut state = new_state();
        let desc = [0x05, 0x01, 0x09, 0x06]; // minimal HID desc
        assert!(capture_descriptor(&mut state, 1, 0, 1, &desc));
        assert_eq!(state.iface_count, 1);
        assert_eq!(state.ifaces[0].dev_addr, 1);
        assert_eq!(state.ifaces[0].instance, 0);
        assert_eq!(state.ifaces[0].itf_protocol, 1);
        assert_eq!(state.ifaces[0].desc_len, 4);
        assert_eq!(&state.ifaces[0].desc[..4], &desc);
        assert!(!state.ifaces[0].always_passthrough);
    }

    #[test]
    fn capture_vendor_is_always_passthrough() {
        let mut state = new_state();
        let desc = [0x06, 0x00, 0xFF];
        assert!(capture_descriptor(&mut state, 1, 0, HID_ITF_PROTOCOL_NONE, &desc));
        assert!(state.ifaces[0].always_passthrough);
    }

    #[test]
    fn capture_empty_desc_rejected() {
        let mut state = new_state();
        assert!(!capture_descriptor(&mut state, 1, 0, 1, &[]));
        assert_eq!(state.iface_count, 0);
    }

    #[test]
    fn capture_oversized_desc_rejected() {
        let mut state = new_state();
        let desc = [0u8; MAX_HID_DESC_SIZE + 1];
        assert!(!capture_descriptor(&mut state, 1, 0, 1, &desc));
        assert_eq!(state.iface_count, 0);
    }

    #[test]
    fn capture_max_size_desc_accepted() {
        let mut state = new_state();
        let desc = [0xAA; MAX_HID_DESC_SIZE];
        assert!(capture_descriptor(&mut state, 1, 0, 1, &desc));
        assert_eq!(state.ifaces[0].desc_len, MAX_HID_DESC_SIZE as u16);
    }

    #[test]
    fn capture_overflow_rejected() {
        let mut state = new_state();
        let desc = [0x01];
        for i in 0..MAX_PASSTHROUGH_IFACES {
            assert!(capture_descriptor(&mut state, 1, i as u8, 1, &desc));
        }
        assert_eq!(state.iface_count, MAX_PASSTHROUGH_IFACES as u8);
        // 7th capture should fail
        assert!(!capture_descriptor(&mut state, 1, 99, 1, &desc));
        assert_eq!(state.iface_count, MAX_PASSTHROUGH_IFACES as u8);
    }

    #[test]
    fn capture_duplicate_overwrites() {
        let mut state = new_state();
        let desc1 = [0x01, 0x02];
        let desc2 = [0x03, 0x04, 0x05];
        assert!(capture_descriptor(&mut state, 1, 0, 1, &desc1));
        assert!(capture_descriptor(&mut state, 1, 0, 2, &desc2));
        assert_eq!(state.iface_count, 1); // not 2
        assert_eq!(state.ifaces[0].itf_protocol, 2);
        assert_eq!(state.ifaces[0].desc_len, 3);
        assert_eq!(&state.ifaces[0].desc[..3], &desc2);
    }

    #[test]
    fn capture_multiple_devices() {
        let mut state = new_state();
        let desc = [0x01];
        assert!(capture_descriptor(&mut state, 1, 0, 1, &desc));
        assert!(capture_descriptor(&mut state, 1, 1, 0, &desc));
        assert!(capture_descriptor(&mut state, 2, 0, 2, &desc));
        assert_eq!(state.iface_count, 3);
        assert_eq!(state.ifaces[0].dev_addr, 1);
        assert_eq!(state.ifaces[1].dev_addr, 1);
        assert_eq!(state.ifaces[2].dev_addr, 2);
    }

    // -- Remove device tests --

    #[test]
    fn remove_device_compacts_array() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        capture_descriptor(&mut state, 2, 0, 1, &desc);
        capture_descriptor(&mut state, 1, 1, 1, &desc);
        assert_eq!(state.iface_count, 3);

        remove_device(&mut state, 1);
        assert_eq!(state.iface_count, 1);
        assert_eq!(state.ifaces[0].dev_addr, 2);
    }

    #[test]
    fn remove_device_resets_when_empty() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        state.upstream_vid = 0x046D;
        state.upstream_pid = 0xC548;
        state.active = true;

        remove_device(&mut state, 1);
        assert_eq!(state.iface_count, 0);
        assert_eq!(state.upstream_vid, 0);
        assert_eq!(state.upstream_pid, 0);
        assert!(!state.active);
    }

    #[test]
    fn remove_nonexistent_device_noop() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        remove_device(&mut state, 99);
        assert_eq!(state.iface_count, 1);
    }

    // -- Instance mapping tests --

    #[test]
    fn host_to_device_mapping() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        capture_descriptor(&mut state, 1, 1, 0, &desc);
        capture_descriptor(&mut state, 2, 0, 2, &desc);

        assert_eq!(host_to_device_instance(&state, 1, 0), Some(ITF_NUM_PT_BASE));
        assert_eq!(host_to_device_instance(&state, 1, 1), Some(ITF_NUM_PT_BASE + 1));
        assert_eq!(host_to_device_instance(&state, 2, 0), Some(ITF_NUM_PT_BASE + 2));
        assert_eq!(host_to_device_instance(&state, 99, 0), None);
    }

    #[test]
    fn device_to_host_mapping() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        capture_descriptor(&mut state, 1, 1, 0, &desc);

        assert_eq!(device_to_host_index(&state, ITF_NUM_PT_BASE), Some(0));
        assert_eq!(device_to_host_index(&state, ITF_NUM_PT_BASE + 1), Some(1));
        assert_eq!(device_to_host_index(&state, ITF_NUM_PT_BASE + 2), None); // out of range
        assert_eq!(device_to_host_index(&state, 0), None); // below base
        assert_eq!(device_to_host_index(&state, 1), None); // below base
    }

    #[test]
    fn get_report_desc_valid() {
        let mut state = new_state();
        let desc = [0x05, 0x01, 0x09, 0x06];
        capture_descriptor(&mut state, 1, 0, 1, &desc);

        let result = get_report_desc(&state, ITF_NUM_PT_BASE);
        assert!(result.is_some());
        let (d, len) = result.unwrap();
        assert_eq!(len, 4);
        assert_eq!(d, &desc);
    }

    #[test]
    fn get_report_desc_invalid_instance() {
        let state = new_state();
        assert!(get_report_desc(&state, 0).is_none());
        assert!(get_report_desc(&state, ITF_NUM_PT_BASE).is_none());
    }

    // -- VID/PID tests (FR-PT-009) --

    #[test]
    fn upstream_vid_pid_stored() {
        let mut state = new_state();
        state.upstream_vid = 0x046D;
        state.upstream_pid = 0xC548;
        assert_eq!(state.upstream_vid, 0x046D);
        assert_eq!(state.upstream_pid, 0xC548);
    }

    #[test]
    fn upstream_vid_pid_cleared_on_remove() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        state.upstream_vid = 0x046D;
        state.upstream_pid = 0xC548;
        remove_device(&mut state, 1);
        assert_eq!(state.upstream_vid, 0);
        assert_eq!(state.upstream_pid, 0);
    }

    // -- Activate tests --

    #[test]
    fn activate_empty_fails() {
        let mut state = new_state();
        assert!(!activate(&mut state, true));
    }

    #[test]
    fn activate_no_config_desc_fails() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        assert!(!activate(&mut state, false));
        assert!(!state.active);
    }

    #[test]
    fn activate_success() {
        let mut state = new_state();
        let desc = [0x01];
        capture_descriptor(&mut state, 1, 0, 1, &desc);
        assert!(activate(&mut state, true));
        assert!(state.active);
    }

    // -- HID++ classification tests --

    #[test]
    fn hidpp_short_input_event() {
        let report = [HIDPP_REPORT_ID_SHORT, 0x01, 0x05, 0x20]; // sw_id=0
        assert!(is_hidpp_input_event(&report));
    }

    #[test]
    fn hidpp_long_input_event() {
        let report = [HIDPP_REPORT_ID_LONG, 0x01, 0x05, 0x20]; // sw_id=0
        assert!(is_hidpp_input_event(&report));
    }

    #[test]
    fn hidpp_protocol_response() {
        let report = [HIDPP_REPORT_ID_SHORT, 0x01, 0x05, 0x21]; // sw_id=1
        assert!(!is_hidpp_input_event(&report));
    }

    #[test]
    fn hidpp_deskhop_swid() {
        let report = [HIDPP_REPORT_ID_SHORT, 0x01, 0x05, 0x2F]; // sw_id=0xF
        assert!(!is_hidpp_input_event(&report));
    }

    #[test]
    fn hidpp_non_hidpp_report() {
        let report = [0x01, 0x00, 0x00, 0x00]; // report_id=1
        assert!(!is_hidpp_input_event(&report));
    }

    #[test]
    fn hidpp_too_short() {
        let report = [HIDPP_REPORT_ID_SHORT, 0x01, 0x05];
        assert!(!is_hidpp_input_event(&report));
    }

    // -- CID to button bit tests --

    #[test]
    fn cid_button_mapping() {
        assert_eq!(cid_to_button_bit(0x50), 0x01); // Left
        assert_eq!(cid_to_button_bit(0x51), 0x02); // Right
        assert_eq!(cid_to_button_bit(0x52), 0x04); // Middle
        assert_eq!(cid_to_button_bit(0x53), 0x08); // Back
        assert_eq!(cid_to_button_bit(0x56), 0x10); // Forward
        assert_eq!(cid_to_button_bit(0x00), 0x00); // Unknown
        assert_eq!(cid_to_button_bit(0xFF), 0x00); // Unknown
    }

    // -- Autolearn tests --

    #[test]
    fn autolearn_reprog_controls() {
        let mut d = HidppDiscovery::default();
        // fn=2, params=[0x00, 0x50(Left), action]
        autolearn_feature(&mut d, 0x05, 2, &[0x00, 0x50, 0x01]);
        assert_eq!(d.fi_reprog_controls, 0x05);
    }

    #[test]
    fn autolearn_ignores_non_button() {
        let mut d = HidppDiscovery::default();
        autolearn_feature(&mut d, 0x05, 2, &[0x00, 0xAA, 0x01]); // unknown CID
        assert_eq!(d.fi_reprog_controls, 0);
    }

    #[test]
    fn autolearn_from_fn0_standard_button() {
        // fn=0 divertedButtons bitmap carrying a standard-button CID (Back) — a
        // mouse whose buttons Options+ diverts as fn=0 (no fn=2) must still learn.
        let mut d = HidppDiscovery::default();
        autolearn_feature(&mut d, 0x05, 0, &[0x00, 0x53, 0x00]);
        assert_eq!(d.fi_reprog_controls, 0x05);
    }

    #[test]
    fn autolearn_from_fn0_gesture_cid() {
        // Pressing only the gesture button (0xC3) must be enough to learn the
        // ReprogControls index so the Android remap fires on the first press.
        let mut d = HidppDiscovery::default();
        autolearn_feature(&mut d, 0x05, 0, &[0x00, GESTURE_CID, 0x00]);
        assert_eq!(d.fi_reprog_controls, 0x05);
    }

    #[test]
    fn autolearn_from_fn0_smartshift_cid() {
        let mut d = HidppDiscovery::default();
        autolearn_feature(&mut d, 0x05, 0, &[0x00, SMARTSHIFT_CID, 0x00]);
        assert_eq!(d.fi_reprog_controls, 0x05);
    }

    #[test]
    fn autolearn_fn0_ignores_unknown_cid() {
        let mut d = HidppDiscovery::default();
        autolearn_feature(&mut d, 0x05, 0, &[0x00, 0xAA, 0x00]); // unknown CID
        assert_eq!(d.fi_reprog_controls, 0);
    }

    #[test]
    fn autolearn_fn0_excludes_scroll_index() {
        // A HiRes-scroll fn=0 delta could alias a CID byte; never learn the
        // already-known scroll/thumbwheel index as ReprogControls.
        let mut d = HidppDiscovery::default();
        d.fi_hires_scroll = 0x06;
        autolearn_feature(&mut d, 0x06, 0, &[0x00, 0x53, 0x00]); // looks like Back, but on scroll fi
        assert_eq!(d.fi_reprog_controls, 0);
    }

    // -- Feature sniffing tests --

    #[test]
    fn sniff_hires_scroll() {
        let mut d = HidppDiscovery::default();
        // device_idx=1, fi=0x08, fn=2|sw=1, p0=0x03
        let report = [0x01, 0x08, 0x21, 0x03, 0x00, 0x00];
        sniff_feature_indices(&mut d, &report);
        assert_eq!(d.fi_hires_scroll, 0x08);
        assert_eq!(d.device_idx, 1);
    }

    #[test]
    fn sniff_thumbwheel_after_hires() {
        let mut d = HidppDiscovery::default();
        d.fi_hires_scroll = 0x08;
        d.device_idx = 1;
        // device_idx=1, fi=0x09, fn=2|sw=1, p0=0x01
        let report = [0x01, 0x09, 0x21, 0x01, 0x00, 0x00];
        sniff_feature_indices(&mut d, &report);
        assert_eq!(d.fi_thumbwheel, 0x09);
    }

    #[test]
    fn sniff_thumbwheel_ignored_without_hires() {
        let mut d = HidppDiscovery::default();
        let report = [0x01, 0x09, 0x21, 0x01, 0x00, 0x00];
        sniff_feature_indices(&mut d, &report);
        assert_eq!(d.fi_thumbwheel, 0);
    }

    #[test]
    fn sniff_ignores_input_events() {
        let mut d = HidppDiscovery::default();
        // sw_id=0 (input event, not protocol)
        let report = [0x01, 0x08, 0x20, 0x03, 0x00, 0x00];
        sniff_feature_indices(&mut d, &report);
        assert_eq!(d.fi_hires_scroll, 0);
    }

    // -- HID++ → mouse conversion tests --

    #[test]
    fn convert_analytics_key_press() {
        let mut state = new_state();
        state.hidpp_disc.fi_reprog_controls = 0x05;
        // fn=2, sw=0, params=[0x00, 0x50(Left), 0x01(press)]
        let report = [0x11, 0x01, 0x05, 0x20, 0x00, 0x50, 0x01];
        let mut mouse = MouseReportC::default();
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(!result); // button update only
        assert_eq!(state.hidpp_disc.button_state, 0x01);
    }

    #[test]
    fn convert_analytics_key_release() {
        let mut state = new_state();
        state.hidpp_disc.fi_reprog_controls = 0x05;
        state.hidpp_disc.button_state = 0x01;
        let report = [0x11, 0x01, 0x05, 0x20, 0x00, 0x50, 0x00]; // release
        let mut mouse = MouseReportC::default();
        convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert_eq!(state.hidpp_disc.button_state, 0x00);
    }

    #[test]
    fn convert_diverted_buttons_bitmap() {
        let mut state = new_state();
        state.hidpp_disc.fi_reprog_controls = 0x05;
        // fn=0, sw=0, params: CID 0x0050(Left) + CID 0x0051(Right)
        let report = [0x11, 0x01, 0x05, 0x00, 0x00, 0x50, 0x00, 0x51, 0x00, 0x00, 0x00, 0x00];
        let mut mouse = MouseReportC::default();
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(!result);
        assert_eq!(state.hidpp_disc.button_state, 0x01 | 0x02);
    }

    #[test]
    fn convert_hires_scroll_accumulator() {
        let mut state = new_state();
        state.hidpp_disc.fi_hires_scroll = 0x08;
        let mut mouse = MouseReportC::default();

        // delta = 2 per event, need 6 events to accumulate 12 → 1 tick
        for _ in 0..5 {
            let report = [0x11, 0x01, 0x08, 0x00, 0x00, 0x00, 0x02];
            let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
            assert!(!result); // accumulated < 12
        }

        // 6th event: accumulator = 12 → emit tick
        let report = [0x11, 0x01, 0x08, 0x00, 0x00, 0x00, 0x02];
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(result);
        assert_eq!(mouse.wheel, 1);
    }

    #[test]
    fn convert_hires_scroll_negative() {
        let mut state = new_state();
        state.hidpp_disc.fi_hires_scroll = 0x08;
        let mut mouse = MouseReportC::default();

        // delta = -12 (0xFFF4 as i16)
        let report = [0x11, 0x01, 0x08, 0x00, 0x00, 0xFF, 0xF4];
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(result);
        assert_eq!(mouse.wheel, -1);
    }

    #[test]
    fn convert_thumbwheel() {
        let mut state = new_state();
        state.hidpp_disc.fi_thumbwheel = 0x09;
        let mut mouse = MouseReportC::default();

        // delta = 12
        let report = [0x11, 0x01, 0x09, 0x00, 0x00, 0x00, 0x0C];
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(result);
        assert_eq!(mouse.pan, 1);
    }

    #[test]
    fn convert_unknown_feature_ignored() {
        let mut state = new_state();
        state.hidpp_disc.fi_reprog_controls = 0x05;
        state.hidpp_disc.fi_hires_scroll = 0x08;
        // Unknown feature index 0x0A
        let report = [0x11, 0x01, 0x0A, 0x00, 0x00, 0x00, 0x00];
        let mut mouse = MouseReportC::default();
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(!result);
    }

    #[test]
    fn convert_too_short_rejected() {
        let mut state = new_state();
        let report = [0x10, 0x01, 0x05, 0x20, 0x00, 0x50]; // 6 bytes < 7
        let mut mouse = MouseReportC::default();
        let result = convert_hidpp_to_mouse(&mut state, &report, &mut mouse);
        assert!(!result);
    }

}
