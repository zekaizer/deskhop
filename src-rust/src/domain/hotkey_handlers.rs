// Hotkey handler implementations — pure state logic, no HAL dependency.
// HAL calls (send_value, set_active_output, etc.) are in hal/ffi/handlers.rs.

use crate::domain::structs::DeviceState;

/// Toggle output between A and B. Returns true if toggled (false if locked).
pub fn output_toggle(state: &mut DeviceState<'_>) -> bool {
    if state.cfg.switch_lock {
        return false;
    }
    state.cfg.active_output ^= 1;
    true
}

/// Toggle mouse zoom mode. Returns new value.
pub fn mouse_zoom_toggle(state: &mut DeviceState<'_>) -> bool {
    state.cfg.mouse_zoom = !state.cfg.mouse_zoom;
    state.cfg.mouse_zoom
}

/// Toggle switch lock. Returns new value.
pub fn switch_lock_toggle(state: &mut DeviceState<'_>) -> bool {
    state.cfg.switch_lock = !state.cfg.switch_lock;
    state.cfg.switch_lock
}

/// Toggle gaming mode. Returns new value.
pub fn gaming_mode_toggle(state: &mut DeviceState<'_>) -> bool {
    state.cfg.gaming_mode = !state.cfg.gaming_mode;
    state.cfg.gaming_mode
}

/// Result of screensaver_set: whether HAL send_value is needed.
pub enum ScreensaverAction {
    UpdatedLocally,
    SendToRemote(u8),
}

/// Set screensaver mode. Returns action for caller.
pub fn screensaver_set(state: &mut DeviceState<'_>, mode: u8) -> ScreensaverAction {
    if state.is_active_output() {
        let role = state.cfg.board_role as usize;
        if role < state.cfg.config.output.len() {
            state.cfg.config.output[role].screensaver.mode = mode;
        }
        ScreensaverAction::UpdatedLocally
    } else {
        ScreensaverAction::SendToRemote(mode)
    }
}

/// Compute pong screensaver mode. Returns mode to set.
pub fn screensaver_pong_mode(state: &DeviceState<'_>) -> Option<u8> {
    let role = state.cfg.board_role as usize;
    if role >= state.cfg.config.output.len() { return None; }
    let current = state.cfg.config.output[role].screensaver.mode;
    Some(if current == 0 || current == 2 { 1 } else { current })
}

/// Compute jitter screensaver mode. Returns mode to set.
pub fn screensaver_jitter_mode(state: &DeviceState<'_>) -> Option<u8> {
    let role = state.cfg.board_role as usize;
    if role >= state.cfg.config.output.len() { return None; }
    let current = state.cfg.config.output[role].screensaver.mode;
    Some(if current == 0 || current == 1 { 2 } else { current })
}

/// Check if config mode scratch registers need to be set.
/// Does NOT set reboot_requested — caller handles that after HAL calls.
pub fn needs_config_scratch(state: &DeviceState<'_>) -> bool {
    !state.cfg.config_mode_active
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::structs::DeviceState;

    #[test]
    fn test_output_toggle_logic() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.active_output = 0;
        assert!(output_toggle(&mut state));
        assert_eq!(state.cfg.active_output, 1);
        assert!(output_toggle(&mut state));
        assert_eq!(state.cfg.active_output, 0);
    }

    #[test]
    fn test_toggle_blocked_by_lock() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.switch_lock = true;
        state.cfg.active_output = 0;
        assert!(!output_toggle(&mut state));
        assert_eq!(state.cfg.active_output, 0);
    }

    #[test]
    fn test_mouse_zoom_toggle() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        assert!(!state.cfg.mouse_zoom);
        assert!(mouse_zoom_toggle(&mut state));
        assert!(state.cfg.mouse_zoom);
        assert!(!mouse_zoom_toggle(&mut state));
        assert!(!state.cfg.mouse_zoom);
    }

    #[test]
    fn test_screensaver_mode_selection() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.config.output[0].screensaver.mode = 0;
        assert_eq!(screensaver_pong_mode(&state), Some(1));

        state.cfg.config.output[0].screensaver.mode = 1;
        assert_eq!(screensaver_jitter_mode(&state), Some(2));

        state.cfg.config.output[0].screensaver.mode = 2;
        assert_eq!(screensaver_pong_mode(&state), Some(1));
    }

    #[test]
    fn test_screensaver_set_active() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0; // active
        match screensaver_set(&mut state, 1) {
            ScreensaverAction::UpdatedLocally => {
                assert_eq!(state.cfg.config.output[0].screensaver.mode, 1);
            }
            _ => panic!("Expected UpdatedLocally"),
        }
    }

    #[test]
    fn test_screensaver_set_remote() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 1; // NOT active
        match screensaver_set(&mut state, 2) {
            ScreensaverAction::SendToRemote(m) => assert_eq!(m, 2),
            _ => panic!("Expected SendToRemote"),
        }
    }

    #[test]
    fn test_screensaver_pong_mode_from_disabled() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.config.output[0].screensaver.mode = 0;
        assert_eq!(screensaver_pong_mode(&state), Some(1));
    }

    #[test]
    fn test_screensaver_jitter_mode_from_disabled() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.config.output[0].screensaver.mode = 0;
        assert_eq!(screensaver_jitter_mode(&state), Some(2));
    }

    #[test]
    fn test_screensaver_pong_mode_already_pong() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.config.output[0].screensaver.mode = 1;
        assert_eq!(screensaver_pong_mode(&state), Some(1));
    }

    #[test]
    fn test_needs_config_scratch_not_active() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.config_mode_active = false;
        assert!(needs_config_scratch(&state));
    }

    #[test]
    fn test_needs_config_scratch_already_active() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.config_mode_active = true;
        assert!(!needs_config_scratch(&state));
    }
}
