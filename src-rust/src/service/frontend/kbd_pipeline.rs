// Keyboard input pipeline — hotkey detection, state combination, and routing.

use crate::domain::keyboard::{self, KeyboardReport, HotkeyMatch};
use crate::domain::kbd_state;
use crate::domain::structs::{Device, HidKeyboardReport};
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

/// Result of processing a keyboard report.
pub enum KbdAction {
    /// Hotkey consumed the report — do not pass to OS.
    HotkeyConsumed { acknowledge: bool },
    /// Hotkey matched but should still pass to OS.
    HotkeyPassthrough { acknowledge: bool },
    /// No hotkey — route the combined report normally.
    Route,
}

/// Process a keyboard report: check hotkeys, update state, decide routing.
/// Returns the action for the ffi layer to execute.
pub fn process_report(
    state: &mut Device,
    report_bytes: &[u8; 8],
    itf: u8,
) -> KbdAction {
    if state.reboot_requested {
        return KbdAction::HotkeyConsumed { acknowledge: false };
    }

    // Update keyboard state for this device
    let kbd = unsafe { &*(report_bytes.as_ptr() as *const HidKeyboardReport) };
    kbd_state::update_kbd_state(state, kbd, itf);

    // Check hotkeys
    let report_for_hotkey = KeyboardReport {
        modifier: report_bytes[0],
        reserved: report_bytes[1],
        keycode: [report_bytes[2], report_bytes[3], report_bytes[4],
                  report_bytes[5], report_bytes[6], report_bytes[7]],
    };

    if let Some(m) = keyboard::check_all_hotkeys(&report_for_hotkey) {
        if m.pass_to_os {
            return KbdAction::HotkeyPassthrough { acknowledge: m.acknowledge };
        } else {
            return KbdAction::HotkeyConsumed { acknowledge: m.acknowledge };
        }
    }

    KbdAction::Route
}

/// Route the combined keyboard state to the active output.
pub fn route_combined(
    state: &mut Device,
    hal: &impl ReportRouter,
) {
    let combined = kbd_state::combine_kbd_states(state);
    hal.route_kbd(state, &combined as *const _ as *const u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    #[test]
    fn process_report_normal_key() {
        let mut state = Device::zeroed();
        let report = [0u8, 0, 0x04, 0, 0, 0, 0, 0]; // key A, no modifier
        match process_report(&mut state, &report, 0) {
            KbdAction::Route => {} // expected — no hotkey
            _ => panic!("Expected Route"),
        }
    }

    #[test]
    fn process_report_reboot_requested() {
        let mut state = Device::zeroed();
        state.reboot_requested = true;
        let report = [0u8; 8];
        match process_report(&mut state, &report, 0) {
            KbdAction::HotkeyConsumed { acknowledge: false } => {}
            _ => panic!("Expected HotkeyConsumed when rebooting"),
        }
    }

    #[test]
    fn route_combined_active_output() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0;

        route_combined(&mut state, &hal);

        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn route_combined_peer_output() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 1; // NOT active

        route_combined(&mut state, &hal);

        assert!(hal.kbd_reports.borrow().is_empty());
        assert_eq!(hal.sent_packets.borrow().len(), 1);
    }
}
