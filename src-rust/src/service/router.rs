// ReportRouter — dual-board routing abstraction.
// Routes HID reports to local queue (active output) or peer link (inactive).
// Centralizes the is_active_output() branching scattered across ffi/ modules.

use crate::domain::constants::PacketType;
use crate::domain::structs::Device;
use crate::hal::traits::*;

/// Automatic report routing based on active output state.
/// Blanket-implemented for any type satisfying the required HAL traits.
pub trait ReportRouter: ReportQueue + PacketQueue + PeerLink + Timer {
    fn route_mouse(&self, state: &mut Device, report: &[u8]) {
        if state.is_active_output() {
            self.push_mouse_report(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::MouseReport as u8);
        }
    }

    fn route_kbd(&self, state: &mut Device, report: &[u8]) {
        if state.is_active_output() {
            self.push_kbd_report(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::KeyboardReport as u8);
        }
    }

    fn route_consumer(&self, state: &mut Device, report: &[u8]) {
        if state.is_active_output() {
            self.push_consumer_control(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::ConsumerControl as u8);
        }
    }

    fn route_system(&self, state: &mut Device, report: &[u8]) {
        if state.is_active_output() {
            self.push_system_control(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::SystemControl as u8);
        }
    }

    /// Update last_activity timestamp for the local board role.
    fn touch_activity(&self, state: &mut Device) {
        let role = state.board_role as usize;
        if role < state.last_activity.len() {
            state.last_activity[role] = self.now_us_64();
        }
    }
}

impl<T: ReportQueue + PacketQueue + PeerLink + Timer> ReportRouter for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    #[test]
    fn route_mouse_active_output() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0; // active

        let report = [1u8, 10, 0, 20, 0, 0, 0, 0];
        hal.route_mouse(&mut state, &report);

        assert_eq!(hal.mouse_reports.borrow().len(), 1);
        assert_eq!(state.last_activity[0], 1_000_000);
        assert!(hal.sent_packets.borrow().is_empty());
    }

    #[test]
    fn route_mouse_inactive_output() {
        let hal = MockHal::new();
        hal.set_time(2_000_000);
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 1; // inactive

        let report = [1u8, 10, 0, 20, 0, 0, 0, 0];
        hal.route_mouse(&mut state, &report);

        assert!(hal.mouse_reports.borrow().is_empty());
        assert_eq!(hal.sent_packets.borrow().len(), 1);
        assert_eq!(state.last_activity[0], 0); // no timestamp update
    }

    #[test]
    fn route_kbd_active_output() {
        let hal = MockHal::new();
        hal.set_time(3_000_000);
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0; // active

        let report = [0x01u8, 0, 0x04, 0, 0, 0, 0, 0];
        hal.route_kbd(&mut state, &report);

        assert_eq!(hal.kbd_reports.borrow().len(), 1);
        assert_eq!(state.last_activity[0], 3_000_000);
        assert!(hal.sent_packets.borrow().is_empty());
    }

    #[test]
    fn touch_activity_updates_timestamp() {
        let hal = MockHal::new();
        hal.set_time(5_000_000);
        let mut state = Device::zeroed();
        state.board_role = 1;

        hal.touch_activity(&mut state);

        assert_eq!(state.last_activity[1], 5_000_000);
        assert_eq!(state.last_activity[0], 0); // other role untouched
    }
}
