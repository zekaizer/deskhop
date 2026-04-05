// Keyboard input pipeline — hotkey detection, state combination, and routing.

use crate::domain::keyboard::{self, HotkeyAction};
use crate::domain::kbd_state;
use crate::domain::structs::{DeviceState, HidKeyboardReport};
use crate::service::router::ReportRouter;

/// Result of processing a keyboard report.
pub enum KbdAction {
    /// Hotkey consumed the report — do not pass to OS.
    HotkeyConsumed { action: HotkeyAction, acknowledge: bool },
    /// Hotkey matched but should still pass to OS.
    HotkeyPassthrough { action: HotkeyAction, acknowledge: bool },
    /// Report silently dropped (e.g. reboot pending).
    Dropped,
    /// No hotkey — route the combined report normally.
    Route,
}

/// Process a keyboard report: check hotkeys, update state, decide routing.
/// Returns the action for the ffi layer to execute.
pub fn process_report(
    state: &mut DeviceState<'_>,
    report_bytes: &[u8; 8],
    itf: u8,
) -> KbdAction {
    if state.fw.reboot_requested {
        return KbdAction::Dropped;
    }

    // Update keyboard state for this device
    let kbd = HidKeyboardReport {
        modifier: report_bytes[0],
        reserved: report_bytes[1],
        keycode: [report_bytes[2], report_bytes[3], report_bytes[4],
                  report_bytes[5], report_bytes[6], report_bytes[7]],
    };
    kbd_state::update_kbd_state(state, &kbd, itf);

    // Check hotkeys
    if let Some(m) = keyboard::check_all_hotkeys(&kbd) {
        if m.pass_to_os {
            return KbdAction::HotkeyPassthrough { action: m.action, acknowledge: m.acknowledge };
        } else {
            return KbdAction::HotkeyConsumed { action: m.action, acknowledge: m.acknowledge };
        }
    }

    KbdAction::Route
}

/// Route the combined keyboard state to the active output.
pub fn route_combined(
    state: &mut DeviceState<'_>,
    hal: &impl ReportRouter,
) {
    let combined = kbd_state::combine_kbd_states(state);
    let bytes = unsafe { core::slice::from_raw_parts(&combined as *const _ as *const u8, core::mem::size_of::<HidKeyboardReport>()) };
    hal.route_kbd(state, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    #[test]
    fn process_report_normal_key() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let report = [0u8, 0, 0x04, 0, 0, 0, 0, 0]; // key A, no modifier
        match process_report(&mut state, &report, 0) {
            KbdAction::Route => {} // expected — no hotkey
            _ => panic!("Expected Route"),
        }
    }

    #[test]
    fn process_report_reboot_requested() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.reboot_requested = true;
        let report = [0u8; 8];
        match process_report(&mut state, &report, 0) {
            KbdAction::Dropped => {}
            _ => panic!("Expected Dropped when rebooting"),
        }
    }

    #[test]
    fn route_combined_active_output() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;

        route_combined(&mut state, &hal);

        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn route_combined_peer_output() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1; // NOT active

        route_combined(&mut state, &hal);

        assert!(hal.kbd_reports.borrow().is_empty());
        assert_eq!(hal.sent_packets.borrow().len(), 1);
    }

    #[test]
    fn test_process_report_hotkey_passthrough() {
        use crate::domain::constants::{KEYBOARD_MODIFIER_RIGHTALT, KEYBOARD_MODIFIER_RIGHTCTRL};
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // MouseZoomToggle: RightAlt + RightCtrl, no keys, pass_to_os=true
        let modifier = KEYBOARD_MODIFIER_RIGHTALT | KEYBOARD_MODIFIER_RIGHTCTRL;
        let report = [modifier, 0, 0, 0, 0, 0, 0, 0];
        match process_report(&mut state, &report, 0) {
            KbdAction::HotkeyPassthrough { action, acknowledge } => {
                assert_eq!(action, HotkeyAction::MouseZoomToggle);
                assert!(acknowledge);
            }
            other => panic!("Expected HotkeyPassthrough, got {:?}", kbd_action_name(&other)),
        }
    }

    #[test]
    fn test_route_combined_updates_timestamp() {
        let hal = MockHal::new();
        hal.set_time(42_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active

        route_combined(&mut state, &hal);

        // touch_activity should have been called by route_kbd
        assert_eq!(state.cfg.last_activity[0], 42_000_000);
    }

    #[test]
    fn test_process_report_state_update() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // Send a report with modifier=0x01, key=0x04 (A)
        let report = [0x01, 0, 0x04, 0x05, 0, 0, 0, 0];
        let result = process_report(&mut state, &report, 0);

        // Should be Route (not a hotkey)
        assert!(matches!(result, KbdAction::Route));

        // kbd_state should have been updated via update_kbd_state
        assert_eq!(state.hid.local_kbd_states[0].modifier, 0x01);
        assert_eq!(state.hid.local_kbd_states[0].keycode[0], 0x04);
        assert_eq!(state.hid.local_kbd_states[0].keycode[1], 0x05);
    }

    #[test]
    fn test_process_report_hotkey_consumed_output_toggle() {
        use crate::domain::constants::{HOTKEY_MODIFIER, HOTKEY_TOGGLE};
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // HOTKEY_MODIFIER = KEYBOARD_MODIFIER_LEFTCTRL (0x01)
        // HOTKEY_TOGGLE = HID_KEY_CAPS_LOCK (0x39)
        let report = [HOTKEY_MODIFIER, 0, HOTKEY_TOGGLE, 0, 0, 0, 0, 0];
        match process_report(&mut state, &report, 0) {
            KbdAction::HotkeyConsumed { action, acknowledge } => {
                assert_eq!(action, HotkeyAction::OutputToggle);
                assert!(!acknowledge);
            }
            other => panic!("Expected HotkeyConsumed(OutputToggle), got {:?}", kbd_action_name(&other)),
        }
    }

    #[test]
    fn test_process_report_six_key_rollover() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // Fill all 6 keycode slots with non-hotkey keys
        let report = [0x00, 0, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09];
        match process_report(&mut state, &report, 0) {
            KbdAction::Route => {} // expected — no hotkey, no crash
            other => panic!("Expected Route with 6-key rollover, got {:?}", kbd_action_name(&other)),
        }
        // Verify all 6 keys stored
        assert_eq!(state.hid.local_kbd_states[0].keycode, [0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
    }
}

/// Debug helper for test assertions
#[cfg(test)]
fn kbd_action_name(action: &KbdAction) -> &'static str {
    match action {
        KbdAction::HotkeyConsumed { .. } => "HotkeyConsumed",
        KbdAction::HotkeyPassthrough { .. } => "HotkeyPassthrough",
        KbdAction::Dropped => "Dropped",
        KbdAction::Route => "Route",
    }
}
