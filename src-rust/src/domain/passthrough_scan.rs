// HID++ feature scan state machine (debug hotkey only).
//
// Pure logic — split from domain::passthrough so the production code path
// stays clear of debug instrumentation. Types (HidppScan, HidppScanState)
// stay in domain::passthrough because they are fields of PassthroughState.

use super::passthrough::{
    HidppScanState, PassthroughOutQueue, PassthroughState,
    HIDPP_REPORT_ID_SHORT, HIDPP_SWID_DESKHOP,
};

/// Max feature indices to query per device.
pub const HIDPP_SCAN_MAX_FEATURES: u8 = 32;

/// Timeout to wait for a scan response before skipping to the next index.
const SCAN_TIMEOUT_US: u64 = 500_000;

/// Build an IFeatureSet.getFeatureID query for the scan state machine.
/// Returns Some(queue entry) if a query should be sent, None if out_queue is busy.
pub fn build_scan_query(state: &mut PassthroughState) -> Option<PassthroughOutQueue> {
    if state.out_queue.pending {
        return None;
    }

    let s = &state.hidpp_scan;

    // Find the first always_passthrough interface for sending
    let mut target_dev_addr = 0u8;
    let mut target_instance = 0u8;
    for i in 0..state.iface_count as usize {
        if state.ifaces[i].always_passthrough {
            target_dev_addr = state.ifaces[i].dev_addr;
            target_instance = state.ifaces[i].instance;
            break;
        }
    }

    let mut q = PassthroughOutQueue {
        dev_addr: target_dev_addr,
        instance: target_instance,
        report_id: HIDPP_REPORT_ID_SHORT,
        report_type: 2,
        ..Default::default()
    };
    q.data[0] = s.device_idx;
    q.data[1] = 0x01; // IFeatureSet is always at index 1
    q.data[2] = (0x01 << 4) | HIDPP_SWID_DESKHOP; // fn=1 | sw_id
    q.data[3] = s.query_idx;
    q.data[4] = 0x00;
    q.data[5] = 0x00;
    q.len = 6;
    q.pending = true;

    Some(q)
}

/// Start a HID++ feature scan. Toggles raw dump mode and begins scanning device 1.
pub fn start_hidpp_scan(state: &mut PassthroughState) {
    if !state.active {
        return;
    }

    let s = &mut state.hidpp_scan;
    s.raw_dump_enabled = !s.raw_dump_enabled;

    // Start full feature scan for device 1, then device 2
    s.device_idx = 1;
    s.next_device = 2;
    s.query_idx = 0;
    s.feature_count = HIDPP_SCAN_MAX_FEATURES;
    s.query_sent_us = 0;
    s.state = HidppScanState::QueryIRoot;
}

/// Advance the scan state machine. Call periodically from the passthrough task.
/// Returns Some(queue) if a query needs to be sent via HAL.
pub fn scan_step(state: &mut PassthroughState, now_us: u64) -> Option<PassthroughOutQueue> {
    let s = &mut state.hidpp_scan;
    if s.state == HidppScanState::Idle || s.state == HidppScanState::Done {
        return None;
    }

    // Timeout: skip to next query after 500ms
    if s.query_sent_us > 0 && (now_us.wrapping_sub(s.query_sent_us)) > SCAN_TIMEOUT_US {
        s.query_idx += 1;
        s.query_sent_us = 0;
    }

    if s.query_idx >= s.feature_count {
        // Move to next device or finish
        if s.next_device > 0 {
            s.device_idx = s.next_device;
            s.next_device = 0;
            s.query_idx = 0;
            s.feature_count = HIDPP_SCAN_MAX_FEATURES;
            s.query_sent_us = 0;
            return None;
        }
        s.state = HidppScanState::Done;
        return None;
    }

    if s.query_sent_us > 0 {
        return None; // waiting for response
    }

    if let Some(q) = build_scan_query(state) {
        state.hidpp_scan.query_sent_us = if now_us != 0 { now_us } else { 1 };
        Some(q)
    } else {
        None
    }
}

/// Handle a scan response from IFeatureSet.getFeatureID.
pub fn handle_scan_response(state: &mut PassthroughState, report: &[u8]) {
    if report.len() < 7 {
        return;
    }

    let s = &mut state.hidpp_scan;
    if s.state != HidppScanState::QueryIRoot {
        return;
    }

    // Response: report[4] = featureID high, report[5] = featureID low, report[6] = type
    let feature_id = ((report[4] as u16) << 8) | report[5] as u16;

    if feature_id == 0 && s.query_idx > 0 {
        // End of feature table
        s.feature_count = s.query_idx;
    }

    s.query_idx += 1;
    s.query_sent_us = 0;
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::passthrough::{capture_descriptor, PassthroughState};

    const HID_ITF_PROTOCOL_NONE: u8 = 0;

    fn new_state() -> PassthroughState {
        PassthroughState::default()
    }

    #[test]
    fn scan_start_activates() {
        let mut state = new_state();
        state.active = true;
        start_hidpp_scan(&mut state);
        assert_eq!(state.hidpp_scan.state, HidppScanState::QueryIRoot);
        assert_eq!(state.hidpp_scan.device_idx, 1);
        assert_eq!(state.hidpp_scan.next_device, 2);
        assert!(state.hidpp_scan.raw_dump_enabled);
    }

    #[test]
    fn scan_start_inactive_noop() {
        let mut state = new_state();
        start_hidpp_scan(&mut state);
        assert_eq!(state.hidpp_scan.state, HidppScanState::Idle);
    }

    #[test]
    fn scan_step_sends_query() {
        let mut state = new_state();
        state.active = true;
        let desc = [0x06, 0x00, 0xFF];
        capture_descriptor(&mut state, 1, 0, HID_ITF_PROTOCOL_NONE, &desc);

        start_hidpp_scan(&mut state);
        let query = scan_step(&mut state, 1000);
        assert!(query.is_some());
        let q = query.unwrap();
        assert_eq!(q.report_id, HIDPP_REPORT_ID_SHORT);
        assert_eq!(q.data[0], 1); // device_idx
        assert_eq!(q.data[1], 0x01); // IFeatureSet
    }

    #[test]
    fn scan_response_advances_query() {
        let mut state = new_state();
        state.hidpp_scan.state = HidppScanState::QueryIRoot;
        state.hidpp_scan.query_idx = 0;

        // Response: featureID=0x0001, type=0
        let report = [0x10, 0x01, 0x01, 0x1F, 0x00, 0x01, 0x00];
        handle_scan_response(&mut state, &report);
        assert_eq!(state.hidpp_scan.query_idx, 1);
    }

    #[test]
    fn scan_response_end_of_table() {
        let mut state = new_state();
        state.hidpp_scan.state = HidppScanState::QueryIRoot;
        state.hidpp_scan.query_idx = 5;
        state.hidpp_scan.feature_count = HIDPP_SCAN_MAX_FEATURES;

        // Response: featureID=0x0000 → end of table
        let report = [0x10, 0x01, 0x01, 0x1F, 0x00, 0x00, 0x00];
        handle_scan_response(&mut state, &report);
        assert_eq!(state.hidpp_scan.feature_count, 5);
    }

    #[test]
    fn scan_timeout_skips_query() {
        let mut state = new_state();
        state.hidpp_scan.state = HidppScanState::QueryIRoot;
        state.hidpp_scan.query_idx = 3;
        state.hidpp_scan.query_sent_us = 1000;
        state.hidpp_scan.feature_count = HIDPP_SCAN_MAX_FEATURES;
        // Mark out_queue as pending to prevent immediate re-query
        state.out_queue.pending = true;

        // 600ms later (> 500ms timeout)
        scan_step(&mut state, 601_000);
        assert_eq!(state.hidpp_scan.query_idx, 4);
        assert_eq!(state.hidpp_scan.query_sent_us, 0);
    }

    #[test]
    fn scan_completes_after_all_features() {
        let mut state = new_state();
        state.active = true;
        let desc = [0x06, 0x00, 0xFF];
        capture_descriptor(&mut state, 1, 0, HID_ITF_PROTOCOL_NONE, &desc);

        start_hidpp_scan(&mut state);
        state.hidpp_scan.query_idx = HIDPP_SCAN_MAX_FEATURES;
        state.hidpp_scan.next_device = 0; // already scanned both
        scan_step(&mut state, 1000);
        assert_eq!(state.hidpp_scan.state, HidppScanState::Done);
    }

    #[test]
    fn scan_moves_to_next_device() {
        let mut state = new_state();
        state.active = true;
        let desc = [0x06, 0x00, 0xFF];
        capture_descriptor(&mut state, 1, 0, HID_ITF_PROTOCOL_NONE, &desc);

        start_hidpp_scan(&mut state);
        state.hidpp_scan.query_idx = HIDPP_SCAN_MAX_FEATURES;
        // next_device = 2 (set by start_hidpp_scan)
        scan_step(&mut state, 1000);
        assert_eq!(state.hidpp_scan.device_idx, 2);
        assert_eq!(state.hidpp_scan.next_device, 0);
        assert_eq!(state.hidpp_scan.query_idx, 0);
    }
}
