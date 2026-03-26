// Message bridge — handles messages received from the peer board via inter-board link.
// Processes keyboard/mouse reports, output selection, and border synchronization.

use crate::domain::actions::{get_border_position, BorderUpdate};
use crate::domain::constants::PacketType;
use crate::domain::kbd_state;
use crate::domain::msg_handlers;
use crate::domain::structs::Device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

/// Process a keyboard report received from the peer board.
/// Routes the combined state and always updates activity timestamp.
pub fn handle_kbd_from_peer(
    state: &mut Device,
    hal: &impl ReportRouter,
    data: &[u8; 8],
) {
    msg_handlers::handle_keyboard_uart(data, state);
    let combined = kbd_state::combine_kbd_states(state);
    hal.route_kbd(state, &combined as *const _ as *const u8);
    // UART keyboard: always update activity (even when routed to peer)
    if !state.is_active_output() {
        hal.touch_activity(state);
    }
}

/// Process a mouse report received from the peer board.
/// Queues locally and updates activity timestamp.
pub fn handle_mouse_from_peer(
    state: &mut Device,
    hal: &(impl ReportQueue + Timer),
    data: &[u8; 8],
) {
    hal.push_mouse_report(data.as_ptr());
    msg_handlers::handle_mouse_uart(data, state);
    // Update activity timestamp directly (no routing decision needed)
    let role = state.board_role as usize;
    if role < state.last_activity.len() {
        state.last_activity[role] = hal.now_us_64();
    }
}

/// Handle output selection from the peer board.
pub fn handle_output_select(
    state: &mut Device,
    hal: &(impl OutputControl + ReportQueue),
    output: u8,
) {
    state.active_output = output;
    if state.tud_connected {
        crate::service::backend::host_link::release_all_keys(state, hal);
    }
    hal.sync_leds();
}

/// Handle border synchronization with the peer board.
pub fn handle_sync_borders(
    state: &mut Device,
    hal: &(impl PeerLink + ConfigStore),
    remote_data: Option<&[u8; 8]>,
) {
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }

    if state.is_active_output() {
        // Local: calculate border from current pointer position
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        // Send to peer
        let b = &state.config.output[idx].border;
        let bytes = border_to_bytes(b.top, b.bottom);
        hal.send_packet(bytes.as_ptr(), PacketType::SyncBorders as u8, 8);
    } else if let Some(data) = remote_data {
        // Remote: apply border values from peer
        let border = &mut state.config.output[idx].border;
        border.top = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        border.bottom = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    }

    hal.save();
}

fn border_to_bytes(top: i32, bottom: i32) -> [u8; 8] {
    let t = top.to_le_bytes();
    let b = bottom.to_le_bytes();
    [t[0], t[1], t[2], t[3], b[0], b[1], b[2], b[3]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;
    use crate::service::router::ReportRouter;

    #[test]
    fn kbd_from_peer_routes_to_local() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0; // active

        handle_kbd_from_peer(&mut state, &hal, &[0u8; 8]);

        // Should route locally (active)
        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn kbd_from_peer_routes_to_peer_and_updates_activity() {
        let hal = MockHal::new();
        hal.set_time(5_000_000);
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 1; // NOT active

        handle_kbd_from_peer(&mut state, &hal, &[0u8; 8]);

        // Should route via peer link
        assert!(hal.kbd_reports.borrow().is_empty());
        assert_eq!(hal.sent_packets.borrow().len(), 1);
        // Activity should still be updated (UART always updates)
        assert_eq!(state.last_activity[0], 5_000_000);
    }

    #[test]
    fn mouse_from_peer_queues_and_updates_activity() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let mut state = Device::zeroed();
        state.board_role = 0;

        handle_mouse_from_peer(&mut state, &hal, &[1, 10, 0, 20, 0, 0, 0, 0]);

        assert_eq!(hal.mouse_reports.borrow().len(), 1);
        assert_eq!(state.last_activity[0], 1_000_000);
    }

    #[test]
    fn output_select_switches_and_syncs_leds() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.tud_connected = true;

        handle_output_select(&mut state, &hal, 1);

        assert_eq!(state.active_output, 1);
        assert_eq!(hal.leds_synced.get(), 1);
        // release_all_keys should have been called
        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn sync_borders_active_sends_to_peer() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0;
        state.pointer_y = 100;

        handle_sync_borders(&mut state, &hal, None);

        // Should send border sync packet to peer
        assert_eq!(hal.sent_packets.borrow().len(), 1);
        assert_eq!(hal.config_saved.get(), 1);
    }

    #[test]
    fn sync_borders_remote_applies_data() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 1; // NOT active

        let data = [
            100u8, 0, 0, 0, // top = 100
            200, 0, 0, 0,   // bottom = 200
        ];
        handle_sync_borders(&mut state, &hal, Some(&data));

        assert_eq!(state.config.output[1].border.top, 100);
        assert_eq!(state.config.output[1].border.bottom, 200);
        assert_eq!(hal.config_saved.get(), 1);
    }
}
