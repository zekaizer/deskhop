// Hotkey handler implementations — pure state logic, no HAL dependency.
// HAL calls (send_value, set_active_output, etc.) are in hal/ffi/handlers.rs.

use crate::domain::structs::Device;

/// Toggle output between A and B. Returns true if toggled (false if locked).
pub fn output_toggle(state: &mut Device) -> bool {
    if state.switch_lock {
        return false;
    }
    state.active_output ^= 1;
    true
}

/// Toggle mouse zoom mode. Returns new value.
pub fn mouse_zoom_toggle(state: &mut Device) -> bool {
    state.mouse_zoom = !state.mouse_zoom;
    state.mouse_zoom
}

/// Toggle switch lock. Returns new value.
pub fn switch_lock_toggle(state: &mut Device) -> bool {
    state.switch_lock = !state.switch_lock;
    state.switch_lock
}

/// Toggle gaming mode. Returns new value.
pub fn gaming_mode_toggle(state: &mut Device) -> bool {
    state.gaming_mode = !state.gaming_mode;
    state.gaming_mode
}

/// Result of screensaver_set: whether HAL send_value is needed.
pub enum ScreensaverAction {
    UpdatedLocally,
    SendToRemote(u8),
}

/// Set screensaver mode. Returns action for caller.
pub fn screensaver_set(state: &mut Device, mode: u8) -> ScreensaverAction {
    if state.is_active_output() {
        let role = state.board_role as usize;
        if role < state.config.output.len() {
            state.config.output[role].screensaver.mode = mode;
        }
        ScreensaverAction::UpdatedLocally
    } else {
        ScreensaverAction::SendToRemote(mode)
    }
}

/// Compute pong screensaver mode. Returns mode to set.
pub fn screensaver_pong_mode(state: &Device) -> Option<u8> {
    let role = state.board_role as usize;
    if role >= state.config.output.len() { return None; }
    let current = state.config.output[role].screensaver.mode;
    Some(if current == 0 || current == 2 { 1 } else { current })
}

/// Compute jitter screensaver mode. Returns mode to set.
pub fn screensaver_jitter_mode(state: &Device) -> Option<u8> {
    let role = state.board_role as usize;
    if role >= state.config.output.len() { return None; }
    let current = state.config.output[role].screensaver.mode;
    Some(if current == 0 || current == 1 { 2 } else { current })
}

/// Check if config mode scratch registers need to be set.
/// Does NOT set reboot_requested — caller handles that after HAL calls.
pub fn needs_config_scratch(state: &Device) -> bool {
    !state.config_mode_active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_toggle_logic() {
        let mut state = Device::zeroed();
        state.active_output = 0;
        assert!(output_toggle(&mut state));
        assert_eq!(state.active_output, 1);
        assert!(output_toggle(&mut state));
        assert_eq!(state.active_output, 0);
    }

    #[test]
    fn test_toggle_blocked_by_lock() {
        let mut state = Device::zeroed();
        state.switch_lock = true;
        state.active_output = 0;
        assert!(!output_toggle(&mut state));
        assert_eq!(state.active_output, 0);
    }

    #[test]
    fn test_mouse_zoom_toggle() {
        let mut state = Device::zeroed();
        assert!(!state.mouse_zoom);
        assert!(mouse_zoom_toggle(&mut state));
        assert!(state.mouse_zoom);
        assert!(!mouse_zoom_toggle(&mut state));
        assert!(!state.mouse_zoom);
    }

    #[test]
    fn test_screensaver_mode_selection() {
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.config.output[0].screensaver.mode = 0;
        assert_eq!(screensaver_pong_mode(&state), Some(1));

        state.config.output[0].screensaver.mode = 1;
        assert_eq!(screensaver_jitter_mode(&state), Some(2));

        state.config.output[0].screensaver.mode = 2;
        assert_eq!(screensaver_pong_mode(&state), Some(1));
    }

    #[test]
    fn test_screensaver_set_active() {
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0; // active
        match screensaver_set(&mut state, 1) {
            ScreensaverAction::UpdatedLocally => {
                assert_eq!(state.config.output[0].screensaver.mode, 1);
            }
            _ => panic!("Expected UpdatedLocally"),
        }
    }

    #[test]
    fn test_screensaver_set_remote() {
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 1; // NOT active
        match screensaver_set(&mut state, 2) {
            ScreensaverAction::SendToRemote(m) => assert_eq!(m, 2),
            _ => panic!("Expected SendToRemote"),
        }
    }
}
