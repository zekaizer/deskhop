// UART message handlers — pure logic portions.
// These process incoming packet data and return state changes
// instead of directly modifying device_t.

use crate::app::constants::PacketType;
use crate::app::handlers::{should_start_fw_upgrade, FwUpgradeState};
use crate::app::state::AppState;

/// Result of processing a UART message — tells the caller what to do
#[derive(Debug)]
pub enum HandlerAction {
    None,
    SetMouseZoom(bool),
    SetSwitchLock(bool),
    SetGamingMode(bool),
    SetKeyboardLeds(u8),
    SetScreensaverMode(u8),
    SetActiveOutput(u8),
    ToggleOutput,
    StartFwUpgrade(FwUpgradeState),
    Reboot,
    FlashLed,
    WipeConfig,
    SaveConfig,
    ReleaseAllKeys,
}

/// Handle simple flag-setting messages that just copy data[0] to a state field.
/// Returns the action to take.
pub fn handle_simple_msg(ptype: u8, data: &[u8; 8], state: &AppState) -> HandlerAction {
    let val = data[0];

    match PacketType::from_u8(ptype) {
        Some(PacketType::MouseZoom) => HandlerAction::SetMouseZoom(val != 0),
        Some(PacketType::SwitchLock) => HandlerAction::SetSwitchLock(val != 0),
        Some(PacketType::GamingMode) => HandlerAction::SetGamingMode(val != 0),
        Some(PacketType::FlashLed) => HandlerAction::FlashLed,
        Some(PacketType::WipeConfig) => HandlerAction::WipeConfig,
        Some(PacketType::SaveConfig) => HandlerAction::SaveConfig,
        Some(PacketType::Reboot) => HandlerAction::Reboot,
        Some(PacketType::Screensaver) => HandlerAction::SetScreensaverMode(val),
        Some(PacketType::OutputSelect) => {
            HandlerAction::SetActiveOutput(val)
        }
        Some(PacketType::KbdSetReport) => {
            HandlerAction::SetKeyboardLeds(val)
        }
        Some(PacketType::Heartbeat) => {
            let other_version = u16::from_le_bytes([data[0], data[1]]);
            match should_start_fw_upgrade(other_version, state.running_fw.version, state.fw.upgrade_in_progress) {
                Some(fw_state) => HandlerAction::StartFwUpgrade(fw_state),
                None => HandlerAction::None,
            }
        }
        _ => HandlerAction::None,
    }
}

/// Apply a HandlerAction to AppState (pure state mutation, no HAL calls).
/// Returns true if HAL side-effects are needed (the caller must handle those).
pub fn apply_action(action: &HandlerAction, state: &mut AppState) -> bool {
    match action {
        HandlerAction::None => false,
        HandlerAction::SetMouseZoom(v) => { state.mouse_zoom = *v; true }
        HandlerAction::SetSwitchLock(v) => { state.switch_lock = *v; false }
        HandlerAction::SetGamingMode(v) => { state.gaming_mode = *v; false }
        HandlerAction::SetScreensaverMode(m) => {
            let role = state.board_role as usize;
            if role < state.config.output.len() {
                state.config.output[role].screensaver.mode = *m;
            }
            false
        }
        HandlerAction::SetActiveOutput(o) => { state.active_output = *o; true }
        HandlerAction::SetKeyboardLeds(leds) => {
            // Store to OTHER_ROLE (1 - board_role)
            let other = 1 - state.board_role as usize;
            if other < state.keyboard_leds.len() {
                state.keyboard_leds[other] = *leds;
            }
            true // needs restore_leds HAL call
        }
        HandlerAction::StartFwUpgrade(fw) => {
            state.fw.upgrade_in_progress = fw.upgrade_in_progress;
            state.fw.byte_done = fw.byte_done;
            state.fw.address = fw.address;
            state.fw.checksum = fw.checksum;
            false
        }
        HandlerAction::ToggleOutput => {
            if !state.switch_lock {
                state.active_output ^= 1;
            }
            true
        }
        HandlerAction::Reboot => true,       // HAL must handle
        HandlerAction::FlashLed => true,      // HAL must handle
        HandlerAction::WipeConfig => true,    // HAL must handle
        HandlerAction::SaveConfig => true,    // HAL must handle
        HandlerAction::ReleaseAllKeys => true, // HAL must handle
    }
}

/// Handle mouse report from UART — update pointer state.
pub fn handle_mouse_uart(data: &[u8; 8], state: &mut AppState) {
    // mouse_report_t layout: buttons(1) + x(i16) + y(i16) + wheel(i8) + pan(i8) + mode(1)
    state.pointer_x = i16::from_le_bytes([data[1], data[2]]);
    state.pointer_y = i16::from_le_bytes([data[3], data[4]]);
    state.mouse_buttons = data[0] as i16;
}

/// Handle keyboard report from UART — update remote keyboard state.
pub fn handle_keyboard_uart(data: &[u8; 8], state: &mut AppState) {
    // hid_keyboard_report_t layout: modifier(1) + reserved(1) + keycode(6)
    state.remote_kbd_state.modifier = data[0];
    state.remote_kbd_state.reserved = data[1];
    state.remote_kbd_state.keycode.copy_from_slice(&data[2..8]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_mouse_zoom() {
        let state = AppState::new();
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::MouseZoom as u8, &data, &state);
        match action {
            HandlerAction::SetMouseZoom(true) => {}
            _ => panic!("Expected SetMouseZoom(true)"),
        }
    }

    #[test]
    fn test_handle_switch_lock() {
        let state = AppState::new();
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::SwitchLock as u8, &data, &state);
        match action {
            HandlerAction::SetSwitchLock(true) => {}
            _ => panic!("Expected SetSwitchLock(true)"),
        }
    }

    #[test]
    fn test_handle_flash_led() {
        let state = AppState::new();
        let data = [0u8; 8];
        let action = handle_simple_msg(PacketType::FlashLed as u8, &data, &state);
        match action {
            HandlerAction::FlashLed => {}
            _ => panic!("Expected FlashLed"),
        }
    }

    #[test]
    fn test_apply_mouse_zoom() {
        let mut state = AppState::new();
        assert!(!state.mouse_zoom);
        apply_action(&HandlerAction::SetMouseZoom(true), &mut state);
        assert!(state.mouse_zoom);
    }

    #[test]
    fn test_apply_toggle_output() {
        let mut state = AppState::new();
        state.active_output = 0;
        apply_action(&HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.active_output, 1);
        apply_action(&HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.active_output, 0);
    }

    #[test]
    fn test_apply_toggle_blocked_by_lock() {
        let mut state = AppState::new();
        state.switch_lock = true;
        state.active_output = 0;
        apply_action(&HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.active_output, 0); // unchanged
    }

    #[test]
    fn test_handle_mouse_uart() {
        let mut state = AppState::new();
        // buttons=1, x=0x1234, y=0x5678
        let data = [1, 0x34, 0x12, 0x78, 0x56, 0, 0, 0];
        handle_mouse_uart(&data, &mut state);
        assert_eq!(state.mouse_buttons, 1);
        assert_eq!(state.pointer_x, 0x1234);
        assert_eq!(state.pointer_y, 0x5678);
    }

    #[test]
    fn test_handle_keyboard_uart() {
        let mut state = AppState::new();
        let data = [0x01, 0x00, 0x04, 0x05, 0x00, 0x00, 0x00, 0x00];
        handle_keyboard_uart(&data, &mut state);
        assert_eq!(state.remote_kbd_state.modifier, 0x01);
        assert_eq!(state.remote_kbd_state.keycode[0], 0x04);
        assert_eq!(state.remote_kbd_state.keycode[1], 0x05);
    }

    #[test]
    fn test_heartbeat_newer_version() {
        let mut state = AppState::new();
        state.running_fw.version = 100;
        let data = [0xC8, 0x00, 0, 0, 0, 0, 0, 0]; // version 200
        let action = handle_simple_msg(PacketType::Heartbeat as u8, &data, &state);
        match action {
            HandlerAction::StartFwUpgrade(_) => {}
            _ => panic!("Expected StartFwUpgrade"),
        }
    }

    #[test]
    fn test_heartbeat_same_version() {
        let mut state = AppState::new();
        state.running_fw.version = 100;
        let data = [100, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::Heartbeat as u8, &data, &state);
        match action {
            HandlerAction::None => {}
            _ => panic!("Expected None"),
        }
    }
}
