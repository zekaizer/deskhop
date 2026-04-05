// UART message handlers — pure logic portions.
// These process incoming packet data and return state changes
// instead of directly modifying device_t.

use crate::domain::constants::PacketType;
use crate::domain::actions::{should_start_fw_upgrade, FwUpgradeRequest};
use crate::domain::structs::DeviceState;

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
    StartFwUpgrade(FwUpgradeRequest),
    Reboot,
    FlashLed,
    WipeConfig,
    SaveConfig,
    ReleaseAllKeys,
}

/// Handle simple flag-setting messages that just copy data[0] to a state field.
/// Returns the action to take.
pub fn handle_simple_msg(ptype: u8, data: &[u8; 8], state: &DeviceState<'_>) -> HandlerAction {
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
            let other_crc16 = u16::from_le_bytes([data[2], data[3]]);
            let my_crc16 = state.fw._running_fw.checksum as u16;
            match should_start_fw_upgrade(
                other_version, state.fw._running_fw.version,
                other_crc16, my_crc16,
                state.cfg.board_role,
                state.fw.fw.upgrade_in_progress,
            ) {
                Some(fw_state) => HandlerAction::StartFwUpgrade(fw_state),
                None => HandlerAction::None,
            }
        }
        _ => HandlerAction::None,
    }
}

/// Apply a HandlerAction to DeviceState (pure state mutation, no HAL calls).
/// Returns true if HAL side-effects are needed (the caller must handle those).
pub fn apply_action(action: &HandlerAction, state: &mut DeviceState<'_>) -> bool {
    match action {
        HandlerAction::None => false,
        HandlerAction::SetMouseZoom(v) => { state.cfg.mouse_zoom = *v; true }
        HandlerAction::SetSwitchLock(v) => { state.cfg.switch_lock = *v; false }
        HandlerAction::SetGamingMode(v) => { state.cfg.gaming_mode = *v; false }
        HandlerAction::SetScreensaverMode(m) => {
            let role = state.cfg.board_role as usize;
            if role < state.cfg.config.output.len() {
                state.cfg.config.output[role].screensaver.mode = *m;
            }
            false
        }
        HandlerAction::SetActiveOutput(o) => { state.cfg.active_output = *o; true }
        HandlerAction::SetKeyboardLeds(leds) => {
            // Store to OTHER_ROLE (1 - board_role)
            let other = 1 - state.cfg.board_role as usize;
            if other < state.cfg.keyboard_leds.len() {
                state.cfg.keyboard_leds[other] = *leds;
            }
            true // needs sync_leds HAL call
        }
        HandlerAction::StartFwUpgrade(fw) => {
            state.fw.fw.upgrade_in_progress = fw.upgrade_in_progress;
            state.fw.fw.byte_done = fw.byte_done;
            state.fw.fw.address = fw.address;
            state.fw.fw.checksum = fw.checksum;
            false
        }
        HandlerAction::ToggleOutput => {
            if !state.cfg.switch_lock {
                state.cfg.active_output ^= 1;
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
pub fn handle_mouse_uart(data: &[u8; 8], state: &mut DeviceState<'_>) {
    // mouse_report_t layout: buttons(1) + x(i16) + y(i16) + wheel(i8) + pan(i8) + mode(1)
    state.hid.pointer_x = i16::from_le_bytes([data[1], data[2]]);
    state.hid.pointer_y = i16::from_le_bytes([data[3], data[4]]);
    state.hid.mouse_buttons = data[0] as i16;
}

/// Handle keyboard report from UART — update remote keyboard state.
pub fn handle_keyboard_uart(data: &[u8; 8], state: &mut DeviceState<'_>) {
    // hid_keyboard_report_t layout: modifier(1) + reserved(1) + keycode(6)
    state.hid.remote_kbd_state.modifier = data[0];
    state.hid.remote_kbd_state.reserved = data[1];
    state.hid.remote_kbd_state.keycode.copy_from_slice(&data[2..8]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::structs::DeviceState;

    #[test]
    fn test_handle_mouse_zoom() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::MouseZoom as u8, &data, &state);
        match action {
            HandlerAction::SetMouseZoom(true) => {}
            _ => panic!("Expected SetMouseZoom(true)"),
        }
    }

    #[test]
    fn test_handle_switch_lock() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::SwitchLock as u8, &data, &state);
        match action {
            HandlerAction::SetSwitchLock(true) => {}
            _ => panic!("Expected SetSwitchLock(true)"),
        }
    }

    #[test]
    fn test_handle_flash_led() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0u8; 8];
        let action = handle_simple_msg(PacketType::FlashLed as u8, &data, &state);
        match action {
            HandlerAction::FlashLed => {}
            _ => panic!("Expected FlashLed"),
        }
    }

    #[test]
    fn test_apply_mouse_zoom() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        assert!(!state.cfg.mouse_zoom);
        apply_action(&HandlerAction::SetMouseZoom(true), &mut state);
        assert!(state.cfg.mouse_zoom);
    }

    #[test]
    fn test_apply_toggle_output() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.active_output = 0;
        apply_action(&HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.cfg.active_output, 1);
        apply_action(&HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.cfg.active_output, 0);
    }

    #[test]
    fn test_apply_toggle_blocked_by_lock() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.switch_lock = true;
        state.cfg.active_output = 0;
        apply_action(&HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.cfg.active_output, 0); // unchanged
    }

    #[test]
    fn test_handle_mouse_uart() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // buttons=1, x=0x1234, y=0x5678
        let data = [1, 0x34, 0x12, 0x78, 0x56, 0, 0, 0];
        handle_mouse_uart(&data, &mut state);
        assert_eq!(state.hid.mouse_buttons, 1);
        assert_eq!(state.hid.pointer_x, 0x1234);
        assert_eq!(state.hid.pointer_y, 0x5678);
    }

    #[test]
    fn test_handle_keyboard_uart() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0x01, 0x00, 0x04, 0x05, 0x00, 0x00, 0x00, 0x00];
        handle_keyboard_uart(&data, &mut state);
        assert_eq!(state.hid.remote_kbd_state.modifier, 0x01);
        assert_eq!(state.hid.remote_kbd_state.keycode[0], 0x04);
        assert_eq!(state.hid.remote_kbd_state.keycode[1], 0x05);
    }

    #[test]
    fn test_heartbeat_newer_version() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw._running_fw.version = 100;
        let data = [0xC8, 0x00, 0, 0, 0, 0, 0, 0]; // version 200
        let action = handle_simple_msg(PacketType::Heartbeat as u8, &data, &state);
        match action {
            HandlerAction::StartFwUpgrade(_) => {}
            _ => panic!("Expected StartFwUpgrade"),
        }
    }

    #[test]
    fn test_heartbeat_same_version() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw._running_fw.version = 100;
        let data = [100, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::Heartbeat as u8, &data, &state);
        match action {
            HandlerAction::None => {}
            _ => panic!("Expected None"),
        }
    }

    #[test]
    fn test_apply_screensaver_mode() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        apply_action(&HandlerAction::SetScreensaverMode(1), &mut state);
        assert_eq!(state.cfg.config.output[0].screensaver.mode, 1);
    }

    #[test]
    fn test_apply_set_active_output() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        apply_action(&HandlerAction::SetActiveOutput(1), &mut state);
        assert_eq!(state.cfg.active_output, 1);
    }

    #[test]
    fn test_apply_set_keyboard_leds() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        apply_action(&HandlerAction::SetKeyboardLeds(0x07), &mut state);
        assert_eq!(state.cfg.keyboard_leds[1], 0x07); // OTHER_ROLE = 1
    }

    #[test]
    fn test_apply_fw_upgrade() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let fw_req = crate::domain::actions::FwUpgradeRequest {
            upgrade_in_progress: true,
            byte_done: true,
            address: 0,
            checksum: 0xFFFFFFFF,
        };
        apply_action(&HandlerAction::StartFwUpgrade(fw_req), &mut state);
        assert!(state.fw.fw.upgrade_in_progress);
        assert!(state.fw.fw.byte_done);
        assert_eq!(state.fw.fw.checksum, 0xFFFFFFFF);
    }

    #[test]
    fn test_handle_reboot_msg() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0u8; 8];
        let action = handle_simple_msg(PacketType::Reboot as u8, &data, &state);
        match action {
            HandlerAction::Reboot => {}
            _ => panic!("Expected Reboot"),
        }
    }

    #[test]
    fn test_handle_save_config() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0u8; 8];
        let action = handle_simple_msg(PacketType::SaveConfig as u8, &data, &state);
        match action {
            HandlerAction::SaveConfig => {}
            _ => panic!("Expected SaveConfig"),
        }
    }

    #[test]
    fn test_handle_unknown_type() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0u8; 8];
        let action = handle_simple_msg(0xFF, &data, &state);
        match action {
            HandlerAction::None => {}
            _ => panic!("Expected None for unknown type"),
        }
    }

    #[test]
    fn test_handle_gaming_mode() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = handle_simple_msg(PacketType::GamingMode as u8, &data, &state);
        match action {
            HandlerAction::SetGamingMode(true) => {}
            _ => panic!("Expected SetGamingMode(true)"),
        }
    }

    #[test]
    fn test_apply_returns_needs_hal() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        assert!(!apply_action(&HandlerAction::None, &mut state));
        assert!(!apply_action(&HandlerAction::SetSwitchLock(true), &mut state));
        assert!(apply_action(&HandlerAction::Reboot, &mut state));
        assert!(apply_action(&HandlerAction::FlashLed, &mut state));
        assert!(apply_action(&HandlerAction::SetActiveOutput(1), &mut state));
    }

    #[test]
    fn test_handle_keyboard_uart_updates_remote_state() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0x03, 0x00, 0x04, 0x05, 0x06, 0x00, 0x00, 0x00];
        handle_keyboard_uart(&data, &mut state);
        assert_eq!(state.hid.remote_kbd_state.modifier, 0x03);
        assert_eq!(state.hid.remote_kbd_state.reserved, 0x00);
        assert_eq!(state.hid.remote_kbd_state.keycode[0], 0x04);
        assert_eq!(state.hid.remote_kbd_state.keycode[1], 0x05);
        assert_eq!(state.hid.remote_kbd_state.keycode[2], 0x06);
    }

    #[test]
    fn test_handle_keyboard_uart_empty_report() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0u8; 8];
        handle_keyboard_uart(&data, &mut state);
        assert_eq!(state.hid.remote_kbd_state.modifier, 0);
        assert_eq!(state.hid.remote_kbd_state.keycode, [0; 6]);
    }

    #[test]
    fn test_handle_keyboard_uart_with_modifiers() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // Left Ctrl + Left Shift = 0x03, keys: A(0x04), B(0x05)
        let data = [0x03, 0x00, 0x04, 0x05, 0x00, 0x00, 0x00, 0x00];
        handle_keyboard_uart(&data, &mut state);
        assert_eq!(state.hid.remote_kbd_state.modifier, 0x03);
        assert_eq!(state.hid.remote_kbd_state.keycode[0], 0x04);
        assert_eq!(state.hid.remote_kbd_state.keycode[1], 0x05);
        // Remaining keycodes should be zero
        assert_eq!(state.hid.remote_kbd_state.keycode[2..], [0; 4]);
    }

    #[test]
    fn test_handle_mouse_uart_updates_activity_flag() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // buttons=2, x=100 (0x0064), y=-50 (0xFFCE)
        let data = [0x02, 0x64, 0x00, 0xCE, 0xFF, 0x00, 0x00, 0x00];
        handle_mouse_uart(&data, &mut state);
        assert_eq!(state.hid.mouse_buttons, 2);
        assert_eq!(state.hid.pointer_x, 100);
        assert_eq!(state.hid.pointer_y, -50);
    }

    #[test]
    fn test_handle_mouse_uart_empty_report() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0u8; 8];
        handle_mouse_uart(&data, &mut state);
        assert_eq!(state.hid.mouse_buttons, 0);
        assert_eq!(state.hid.pointer_x, 0);
        assert_eq!(state.hid.pointer_y, 0);
    }

    #[test]
    fn test_handle_mouse_uart_with_buttons() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // buttons=0x07 (left+right+middle), x=0, y=0
        let data = [0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        handle_mouse_uart(&data, &mut state);
        assert_eq!(state.hid.mouse_buttons, 7);
        assert_eq!(state.hid.pointer_x, 0);
        assert_eq!(state.hid.pointer_y, 0);
    }
}
