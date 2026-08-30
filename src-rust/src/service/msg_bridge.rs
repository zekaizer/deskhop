// Message bridge — handles messages received from the peer board via inter-board link.
// Processes keyboard/mouse reports, output selection, and border synchronization.

use crate::domain::actions::{get_border_position, border_to_bytes, BorderUpdate};
use crate::domain::constants::PacketType;
use crate::domain::kbd_state;
use crate::domain::msg_handlers;
use crate::domain::structs::DeviceState;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

/// Process a keyboard report received from the peer board.
/// Routes the combined state and always updates activity timestamp.
pub fn handle_kbd_from_peer(
    state: &mut DeviceState<'_>,
    hal: &impl ReportRouter,
    data: &[u8; 8],
) {
    msg_handlers::handle_keyboard_uart(data, state);
    let combined = kbd_state::combine_kbd_states(state);
    let bytes = unsafe { core::slice::from_raw_parts(&combined as *const _ as *const u8, core::mem::size_of_val(&combined)) };
    hal.route_kbd(state, bytes);
    // UART keyboard: always update activity (even when routed to peer)
    if !state.is_active_output() {
        hal.touch_activity(state);
    }
}

/// Process a mouse report received from the peer board.
/// Queues locally and updates activity timestamp.
pub fn handle_mouse_from_peer(
    state: &mut DeviceState<'_>,
    hal: &(impl ReportQueue + Timer),
    data: &[u8; 8],
) {
    hal.push_mouse_report(data);
    msg_handlers::handle_mouse_uart(data, state);
    // Update activity timestamp directly (no routing decision needed)
    let role = state.cfg.board_role as usize;
    if role < state.cfg.last_activity.len() {
        state.cfg.last_activity[role] = hal.now_us_64();
    }
}

/// Handle output selection from the peer board.
pub fn handle_output_select(
    state: &mut DeviceState<'_>,
    hal: &(impl OutputControl + ReportQueue),
    output: u8,
) {
    // active_output indexes config.output[NUM_SCREENS] (unguarded in the
    // passthrough HID++ path) — never store an out-of-range value.
    if (output as usize) >= crate::domain::constants::NUM_SCREENS {
        return;
    }
    state.cfg.active_output = output;
    if state.cfg.tud_connected {
        crate::service::backend::host_link::release_all_keys(state, hal);
    }
    hal.sync_leds();
}

/// Handle border synchronization with the peer board.
pub fn handle_sync_borders(
    state: &mut DeviceState<'_>,
    hal: &(impl PeerLink + ConfigStore),
    remote_data: Option<&[u8; 8]>,
) {
    let idx = state.cfg.active_output as usize;
    if idx >= state.cfg.config.output.len() { return; }

    if state.is_active_output() {
        // Local: calculate border from current pointer position
        match get_border_position(state.hid.pointer_y) {
            BorderUpdate::Top(v) => state.cfg.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.cfg.config.output[idx].border.bottom = v,
        }
        // Send to peer
        let b = &state.cfg.config.output[idx].border;
        let bytes = border_to_bytes(b.top, b.bottom);
        hal.send_packet(&bytes, PacketType::SyncBorders as u8);
    } else if let Some(data) = remote_data {
        // Remote: apply border values from peer
        let border = &mut state.cfg.config.output[idx].border;
        border.top = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        border.bottom = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    }

    hal.save();
}

/// Handle USB SET_REPORT callback (keyboard LED state update).
/// Updates LED state for the "other" board role, and syncs if needed.
pub fn handle_set_report(
    state: &mut DeviceState<'_>,
    hal: &impl OutputControl,
    led_value: u8,
) {
    let other = 1usize.wrapping_sub(state.cfg.board_role as usize);
    if other < state.cfg.keyboard_leds_desired.len() {
        state.cfg.keyboard_leds_desired[other] = led_value;
    }
    if state.cfg.keyboard_connected && !state.is_active_output() {
        hal.sync_leds();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;
    use crate::service::router::ReportRouter;

    #[test]
    fn kbd_from_peer_routes_to_local() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active

        handle_kbd_from_peer(&mut state, &hal, &[0u8; 8]);

        // Should route locally (active)
        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn kbd_from_peer_routes_to_peer_and_updates_activity() {
        let hal = MockHal::new();
        hal.set_time(5_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1; // NOT active

        handle_kbd_from_peer(&mut state, &hal, &[0u8; 8]);

        // Should route via peer link
        assert!(hal.kbd_reports.borrow().is_empty());
        assert_eq!(hal.sent_packets.borrow().len(), 1);
        // Activity should still be updated (UART always updates)
        assert_eq!(state.cfg.last_activity[0], 5_000_000);
    }

    #[test]
    fn mouse_from_peer_queues_and_updates_activity() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;

        handle_mouse_from_peer(&mut state, &hal, &[1, 10, 0, 20, 0, 0, 0, 0]);

        assert_eq!(hal.mouse_reports.borrow().len(), 1);
        assert_eq!(state.cfg.last_activity[0], 1_000_000);
    }

    #[test]
    fn output_select_switches_and_syncs_leds() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.tud_connected = true;

        handle_output_select(&mut state, &hal, 1);

        assert_eq!(state.cfg.active_output, 1);
        assert_eq!(hal.leds_synced.get(), 1);
        // release_all_keys should have been called
        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn sync_borders_active_sends_to_peer() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;
        state.hid.pointer_y = 100;

        handle_sync_borders(&mut state, &hal, None);

        // Should send border sync packet to peer
        assert_eq!(hal.sent_packets.borrow().len(), 1);
        assert_eq!(hal.config_saved.get(), 1);
    }

    #[test]
    fn sync_borders_remote_applies_data() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1; // NOT active

        let data = [
            100u8, 0, 0, 0, // top = 100
            200, 0, 0, 0,   // bottom = 200
        ];
        handle_sync_borders(&mut state, &hal, Some(&data));

        assert_eq!(state.cfg.config.output[1].border.top, 100);
        assert_eq!(state.cfg.config.output[1].border.bottom, 200);
        assert_eq!(hal.config_saved.get(), 1);
    }

    #[test]
    fn set_report_updates_leds_and_syncs_when_inactive() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1; // NOT active
        state.cfg.keyboard_connected = true;

        handle_set_report(&mut state, &hal, 0x07);

        assert_eq!(state.cfg.keyboard_leds_desired[1], 0x07); // other = 1 - 0 = 1
        assert_eq!(hal.leds_synced.get(), 1);
    }

    #[test]
    fn set_report_no_sync_when_active() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active
        state.cfg.keyboard_connected = true;

        handle_set_report(&mut state, &hal, 0x03);

        assert_eq!(state.cfg.keyboard_leds_desired[1], 0x03);
        assert_eq!(hal.leds_synced.get(), 0); // no sync when active
    }

    #[test]
    fn test_output_select_not_connected() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.tud_connected = false;

        handle_output_select(&mut state, &hal, 1);

        assert_eq!(state.cfg.active_output, 1);
        // release_all_keys NOT called (no kbd report queued)
        assert!(hal.kbd_reports.borrow().is_empty());
        // sync_leds IS called regardless of tud_connected
        assert_eq!(hal.leds_synced.get(), 1);
    }

    #[test]
    fn test_sync_borders_out_of_range_output() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.active_output = 99; // out of range

        // Should not crash and should not save
        handle_sync_borders(&mut state, &hal, None);

        assert_eq!(hal.config_saved.get(), 0);
        assert!(hal.sent_packets.borrow().is_empty());
    }

    #[test]
    fn test_kbd_from_peer_always_updates_activity() {
        let hal = MockHal::new();
        hal.set_time(7_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active output

        handle_kbd_from_peer(&mut state, &hal, &[0u8; 8]);

        // route_kbd calls touch_activity for active output
        assert_eq!(state.cfg.last_activity[0], 7_000_000);

        // Now test inactive: activity should also be updated
        let hal2 = MockHal::new();
        hal2.set_time(9_000_000);
        let (mut hid2, mut cfg2, mut fw2, mut led2) = DeviceState::zeroed_for_test();
        let mut state2 = DeviceState { hid: &mut hid2, cfg: &mut cfg2, fw: &mut fw2, led: &mut led2 };
        state2.cfg.board_role = 0;
        state2.cfg.active_output = 1; // NOT active

        handle_kbd_from_peer(&mut state2, &hal2, &[0u8; 8]);

        assert_eq!(state2.cfg.last_activity[0], 9_000_000);
    }

    #[test]
    fn test_mouse_from_peer_updates_state() {
        let hal = MockHal::new();
        hal.set_time(2_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;

        // buttons=3, x=0x0100(256), y=0x0200(512)
        let data = [3, 0x00, 0x01, 0x00, 0x02, 0, 0, 0];
        handle_mouse_from_peer(&mut state, &hal, &data);

        // handle_mouse_uart should have updated pointer state
        assert_eq!(state.hid.mouse_buttons, 3);
        assert_eq!(state.hid.pointer_x, 256);
        assert_eq!(state.hid.pointer_y, 512);
        // Activity updated
        assert_eq!(state.cfg.last_activity[0], 2_000_000);
    }

    #[test]
    fn test_kbd_from_peer_combined_routing() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active — routes locally

        // Pre-set local kbd state with modifier=0x01 (LeftCtrl)
        state.hid.local_kbd_states[0].modifier = 0x01;
        state.hid.max_kbd_idx = 0;

        // Peer sends report with modifier=0x02 (LeftShift)
        let peer_report: [u8; 8] = [0x02, 0, 0, 0, 0, 0, 0, 0];
        handle_kbd_from_peer(&mut state, &hal, &peer_report);

        // Combined should have modifier = 0x01 | 0x02 = 0x03
        let reports = hal.kbd_reports.borrow();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][0], 0x03); // modifier byte = OR'd
    }

    #[test]
    fn test_handle_mouse_from_peer_role_out_of_range() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 5; // out of range (last_activity has NUM_SCREENS=2 entries)

        let data = [1, 10, 0, 20, 0, 0, 0, 0];
        // Should not panic — the role bounds check prevents out-of-bounds write
        handle_mouse_from_peer(&mut state, &hal, &data);

        // Report should still be pushed to queue regardless of role
        assert_eq!(hal.mouse_reports.borrow().len(), 1);
        // Activity NOT updated (role out of range)
        assert_eq!(state.cfg.last_activity[0], 0);
        assert_eq!(state.cfg.last_activity[1], 0);
    }

    #[test]
    fn test_sync_borders_active_max_coord_is_bottom() {
        use crate::domain::constants::MAX_SCREEN_COORD;

        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active
        state.hid.pointer_y = MAX_SCREEN_COORD; // at bottom edge

        handle_sync_borders(&mut state, &hal, None);

        // pointer_y > MAX_SCREEN_COORD/2 → Bottom border
        // get_border_position(MAX_SCREEN_COORD) = BorderUpdate::Bottom(32767)
        assert_eq!(state.cfg.config.output[0].border.bottom, MAX_SCREEN_COORD as i32);
        // Top border should remain at default (0)
        assert_eq!(state.cfg.config.output[0].border.top, 0);
        // Packet sent to peer
        assert_eq!(hal.sent_packets.borrow().len(), 1);
        assert_eq!(hal.config_saved.get(), 1);
    }

    // ---- additional edge-case tests ----

    #[test]
    fn test_set_report_role_1_updates_index_0() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 1; // other = 1 - 1 = 0
        state.cfg.active_output = 0; // NOT active (board_role=1 != active_output=0)
        state.cfg.keyboard_connected = true;

        handle_set_report(&mut state, &hal, 0x05);

        // "other" index = 1 - 1 = 0
        assert_eq!(state.cfg.keyboard_leds_desired[0], 0x05);
        assert_eq!(hal.leds_synced.get(), 1);
    }

    #[test]
    fn test_mouse_from_peer_zero_report() {
        let hal = MockHal::new();
        hal.set_time(3_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        // Pre-set some values
        state.hid.pointer_x = 100;
        state.hid.pointer_y = 200;
        state.hid.mouse_buttons = 3;

        handle_mouse_from_peer(&mut state, &hal, &[0u8; 8]);

        // Mouse report still queued
        assert_eq!(hal.mouse_reports.borrow().len(), 1);
        // State reset to zero
        assert_eq!(state.hid.mouse_buttons, 0);
        assert_eq!(state.hid.pointer_x, 0);
        assert_eq!(state.hid.pointer_y, 0);
        // Activity still updated
        assert_eq!(state.cfg.last_activity[0], 3_000_000);
    }

    #[test]
    fn test_output_select_same_output() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.active_output = 1;
        state.cfg.tud_connected = true;

        // Select the already-selected output
        handle_output_select(&mut state, &hal, 1);

        assert_eq!(state.cfg.active_output, 1);
        // LED sync should still happen
        assert_eq!(hal.leds_synced.get(), 1);
    }

    #[test]
    fn test_sync_borders_halfway_point() {
        use crate::domain::constants::MAX_SCREEN_COORD;

        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;
        state.hid.pointer_y = MAX_SCREEN_COORD / 2; // exactly at midpoint

        handle_sync_borders(&mut state, &hal, None);

        // Exactly at half → Top border (not > half)
        assert_eq!(state.cfg.config.output[0].border.top, (MAX_SCREEN_COORD / 2) as i32);
        assert_eq!(state.cfg.config.output[0].border.bottom, 0);
    }

    #[test]
    fn test_kbd_from_peer_with_keys_routes_combined() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active

        // Local has key 'A' (0x04) in slot 2
        state.hid.local_kbd_states[0].keycode[0] = 0x04;
        state.hid.max_kbd_idx = 0;

        // Peer sends key 'B' (0x05) with LeftShift modifier
        let peer_data: [u8; 8] = [0x02, 0, 0x05, 0, 0, 0, 0, 0];
        handle_kbd_from_peer(&mut state, &hal, &peer_data);

        let reports = hal.kbd_reports.borrow();
        assert_eq!(reports.len(), 1);
        // Combined modifier should include peer's LeftShift (0x02)
        // and combined keycodes should include both 0x04 and 0x05
        let report = &reports[0];
        assert!(report[0] & 0x02 != 0); // LeftShift present
    }
}
