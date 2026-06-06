// Hotkey execution service layer — orchestrates domain logic with HAL traits.
// Extracted from hal/ffi/callbacks.rs to enable testing via MockHal.

use crate::domain::actions::{get_border_position, border_to_bytes, BorderUpdate};
use crate::domain::constants::PacketType;
use crate::domain::hotkey_handlers::{self, ScreensaverAction};
use crate::domain::keyboard::HotkeyAction;
use crate::domain::structs::DeviceState;
use crate::hal::traits::*;

/// Execute a hotkey action. Central dispatch for all hotkey types.
pub fn execute_action(
    state: &mut DeviceState<'_>,
    hal: &(impl OutputControl + ReportQueue + PeerLink + ConfigStore + Watchdog + Indicator + Timer),
    action: HotkeyAction,
) {
    crate::service::dlog::i(b"hk").s(b"action=").s(action.name()).done();
    match action {
        HotkeyAction::OutputToggle => output_toggle(state, hal),
        HotkeyAction::MouseZoomToggle => mouse_zoom_toggle(state, hal),
        HotkeyAction::SwitchLockToggle => switch_lock_toggle(state, hal),
        HotkeyAction::GamingModeToggle => gaming_mode_toggle(state, hal),
        HotkeyAction::ScreenLock => execute_screenlock(state, hal),
        HotkeyAction::ScreensaverPong => {
            if let Some(mode) = hotkey_handlers::screensaver_pong_mode(state) {
                dispatch_screensaver(state, hal, mode);
            }
        }
        HotkeyAction::ScreensaverJitter => {
            if let Some(mode) = hotkey_handlers::screensaver_jitter_mode(state) {
                dispatch_screensaver(state, hal, mode);
            }
        }
        HotkeyAction::ScreensaverDisable => dispatch_screensaver(state, hal, 0),
        HotkeyAction::WipeConfig => wipe_and_notify(hal),
        HotkeyAction::ScreenBorder => update_screen_border(state, hal),
        HotkeyAction::ConfigEnable => prepare_config_mode(state, hal),
        HotkeyAction::FwUpgradeA => hal.reboot_to_bootloader(),
        HotkeyAction::FwUpgradeB => hal.send_value(1, PacketType::FirmwareUpgrade as u8),
    }
}

/// Toggle output between A and B.
pub fn output_toggle(state: &mut DeviceState<'_>, hal: &impl OutputControl) {
    if hotkey_handlers::output_toggle(state) {
        hal.switch_output(state.cfg.active_output);
    }
}

/// Toggle mouse zoom and notify peer.
pub fn mouse_zoom_toggle(state: &mut DeviceState<'_>, hal: &impl PeerLink) {
    let val = hotkey_handlers::mouse_zoom_toggle(state);
    hal.send_value(val as u8, PacketType::MouseZoom as u8);
}

/// Toggle switch lock and notify peer.
pub fn switch_lock_toggle(state: &mut DeviceState<'_>, hal: &impl PeerLink) {
    let val = hotkey_handlers::switch_lock_toggle(state);
    hal.send_value(val as u8, PacketType::SwitchLock as u8);
}

/// Toggle gaming mode and notify peer.
pub fn gaming_mode_toggle(state: &mut DeviceState<'_>, hal: &impl PeerLink) {
    let val = hotkey_handlers::gaming_mode_toggle(state);
    hal.send_value(val as u8, PacketType::GamingMode as u8);
}

/// Send screen lock key sequence to both outputs.
/// Local output: queue report + release keys.
/// Remote output: send report + empty report via peer link.
pub fn execute_screenlock(
    state: &mut DeviceState<'_>,
    hal: &(impl ReportQueue + PeerLink),
) {
    for out in 0..2u8 {
        if let Some((modifier, key)) = crate::domain::actions::screenlock_keys(
            state.cfg.config.output[out as usize].os,
        ) {
            let mut report = [0u8; 8];
            report[0] = modifier;
            report[2] = key;
            if state.cfg.board_role == out {
                hal.push_kbd_report(&report);
                crate::service::backend::host_link::release_all_keys(state, hal);
            } else {
                hal.send_packet(&report, PacketType::KeyboardReport as u8);
                hal.send_packet(&[0u8; 8], PacketType::KeyboardReport as u8);
            }
        }
    }
}

/// Enter config mode: set boot flag, release keys, request reboot.
/// Order matters: scratch FIRST, release keys, THEN reboot flag.
pub fn prepare_config_mode(
    state: &mut DeviceState<'_>,
    hal: &(impl Watchdog + ReportQueue),
) {
    if !state.cfg.config_mode_active {
        hal.set_boot_flag();
    }
    crate::service::backend::host_link::release_all_keys(state, hal);
    state.fw.reboot_requested = true;
}

/// Update screen border from current pointer position and sync with peer.
pub fn update_screen_border(
    state: &mut DeviceState<'_>,
    hal: &(impl ConfigStore + PeerLink),
) {
    let idx = state.cfg.active_output as usize;
    if idx >= state.cfg.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.hid.pointer_y) {
            BorderUpdate::Top(v) => state.cfg.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.cfg.config.output[idx].border.bottom = v,
        }
        hal.save();
    }
    let b = &state.cfg.config.output[idx].border;
    let bytes = border_to_bytes(b.top, b.bottom);
    hal.send_packet(&bytes, PacketType::SyncBorders as u8);
}

/// Dispatch screensaver mode change (local update or remote send).
pub fn dispatch_screensaver(
    state: &mut DeviceState<'_>,
    hal: &impl PeerLink,
    mode: u8,
) {
    match hotkey_handlers::screensaver_set(state, mode) {
        ScreensaverAction::UpdatedLocally => {}
        ScreensaverAction::SendToRemote(m) => {
            hal.send_value(m, PacketType::Screensaver as u8);
        }
    }
}

/// Wipe config, reload defaults, and notify peer.
pub fn wipe_and_notify(hal: &(impl ConfigStore + PeerLink)) {
    hal.wipe();
    hal.load();
    hal.send_value(1, PacketType::WipeConfig as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    // ---- execute_screenlock ----

    #[test]
    fn screenlock_local_output_queues_report() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.config.output[0].os = crate::domain::constants::OS_LINUX;
        state.cfg.config.output[1].os = crate::domain::constants::OS_LINUX;

        execute_screenlock(&mut state, &hal);

        // board_role=0 matches out=0 → local queue
        // out=1 → peer link
        assert!(!hal.kbd_reports.borrow().is_empty());
        assert!(!hal.sent_packets.borrow().is_empty());
    }

    #[test]
    fn screenlock_sends_key_then_release() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.config.output[0].os = crate::domain::constants::OS_LINUX;
        state.cfg.config.output[1].os = 0xFF; // unknown OS → no keys

        execute_screenlock(&mut state, &hal);

        // Local: key report + empty (release) report
        let reports = hal.kbd_reports.borrow();
        assert_eq!(reports.len(), 2); // key press + release (from release_all_keys)
    }

    #[test]
    fn screenlock_peer_sends_two_packets() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 1; // role=1 means out=0 is remote
        state.cfg.config.output[0].os = crate::domain::constants::OS_LINUX;
        state.cfg.config.output[1].os = 0xFF; // unknown OS

        execute_screenlock(&mut state, &hal);

        // out=0 is remote → 2 packets (key + empty)
        assert_eq!(hal.sent_packets.borrow().len(), 2);
    }

    // ---- prepare_config_mode ----

    #[test]
    fn config_mode_sets_boot_flag_and_reboot() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.config_mode_active = false;

        prepare_config_mode(&mut state, &hal);

        assert!(hal.boot_flag_set.get());
        assert!(state.fw.reboot_requested);
        // release_all_keys called
        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn config_mode_skips_boot_flag_when_already_active() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.config_mode_active = true;

        prepare_config_mode(&mut state, &hal);

        assert!(!hal.boot_flag_set.get());
        assert!(state.fw.reboot_requested);
    }

    // ---- update_screen_border ----

    #[test]
    fn border_active_saves_and_syncs() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;
        state.hid.pointer_y = 100; // low → top border

        update_screen_border(&mut state, &hal);

        assert_eq!(state.cfg.config.output[0].border.top, 100);
        assert_eq!(hal.config_saved.get(), 1);
        assert_eq!(hal.sent_packets.borrow().len(), 1);
    }

    #[test]
    fn border_inactive_syncs_without_saving() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1; // not active

        update_screen_border(&mut state, &hal);

        assert_eq!(hal.config_saved.get(), 0);
        assert_eq!(hal.sent_packets.borrow().len(), 1);
    }

    #[test]
    fn border_out_of_range_output_noop() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.active_output = 99;

        update_screen_border(&mut state, &hal);

        assert_eq!(hal.config_saved.get(), 0);
        assert!(hal.sent_packets.borrow().is_empty());
    }

    // ---- dispatch_screensaver ----

    #[test]
    fn screensaver_active_updates_locally() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;

        dispatch_screensaver(&mut state, &hal, 1);

        assert_eq!(state.cfg.config.output[0].screensaver.mode, 1);
        assert!(hal.sent_values.borrow().is_empty());
    }

    #[test]
    fn screensaver_inactive_sends_to_peer() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1;

        dispatch_screensaver(&mut state, &hal, 2);

        assert_eq!(hal.sent_values.borrow().len(), 1);
        assert_eq!(hal.sent_values.borrow()[0], (2, PacketType::Screensaver as u8));
    }

    // ---- wipe_and_notify ----

    #[test]
    fn wipe_calls_in_order() {
        let hal = MockHal::new();

        wipe_and_notify(&hal);

        assert_eq!(hal.config_wiped.get(), 1);
        assert_eq!(hal.config_loaded.get(), 1);
        assert_eq!(hal.sent_values.borrow().len(), 1);
        assert_eq!(hal.sent_values.borrow()[0], (1, PacketType::WipeConfig as u8));
    }

    // ---- execute_action dispatch tests ----

    #[test]
    fn execute_output_toggle() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;
        state.cfg.switch_lock = false;

        execute_action(&mut state, &hal, HotkeyAction::OutputToggle);

        assert_eq!(state.cfg.active_output, 1);
        assert_eq!(hal.output_switched.get(), Some(1));
    }

    #[test]
    fn execute_output_toggle_blocked_by_switch_lock() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;
        state.cfg.switch_lock = true;

        execute_action(&mut state, &hal, HotkeyAction::OutputToggle);

        // Should not switch — switch_lock blocks it
        assert_eq!(state.cfg.active_output, 0);
        assert_eq!(hal.output_switched.get(), None);
    }

    #[test]
    fn execute_mouse_zoom_toggle_sends_to_peer() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.mouse_zoom = false;

        execute_action(&mut state, &hal, HotkeyAction::MouseZoomToggle);

        assert!(state.cfg.mouse_zoom);
        let vals = hal.sent_values.borrow();
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0], (1, PacketType::MouseZoom as u8));
    }

    #[test]
    fn execute_switch_lock_toggle_sends_to_peer() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.switch_lock = false;

        execute_action(&mut state, &hal, HotkeyAction::SwitchLockToggle);

        assert!(state.cfg.switch_lock);
        let vals = hal.sent_values.borrow();
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0], (1, PacketType::SwitchLock as u8));
    }

    #[test]
    fn execute_gaming_mode_toggle_sends_to_peer() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.gaming_mode = false;

        execute_action(&mut state, &hal, HotkeyAction::GamingModeToggle);

        assert!(state.cfg.gaming_mode);
        let vals = hal.sent_values.borrow();
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0], (1, PacketType::GamingMode as u8));
    }
}
