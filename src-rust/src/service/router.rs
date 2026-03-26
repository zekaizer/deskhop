// ReportRouter — dual-board routing abstraction.
// Routes HID reports to local queue (active output) or peer link (inactive).
// Centralizes the is_active_output() branching scattered across ffi/ modules.

use crate::domain::constants::PacketType;
use crate::domain::structs::{Device, KBD_REPORT_LENGTH};
use crate::hal::traits::*;

/// Automatic report routing based on active output state.
/// Blanket-implemented for any type satisfying the required HAL traits.
pub trait ReportRouter: ReportQueue + PacketQueue + PeerLink + Timer {
    fn route_mouse(&self, state: &mut Device, report: *const u8) {
        if state.is_active_output() {
            self.push_mouse_report(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::MouseReport as u8, 8);
        }
    }

    fn route_kbd(&self, state: &mut Device, report: *const u8) {
        if state.is_active_output() {
            self.push_kbd_report(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::KeyboardReport as u8, KBD_REPORT_LENGTH as i32);
        }
    }

    fn route_consumer(&self, state: &mut Device, report: *const u8) {
        if state.is_active_output() {
            self.push_consumer_control(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::ConsumerControl as u8, 4);
        }
    }

    fn route_system(&self, state: &mut Device, report: *const u8) {
        if state.is_active_output() {
            self.push_system_control(report);
            self.touch_activity(state);
        } else {
            self.send_packet(report, PacketType::SystemControl as u8, 1);
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
