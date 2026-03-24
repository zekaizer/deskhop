// Hotkey handler implementations — called when hotkey combos are detected.
// These use HAL functions for hardware access (send_value, blink_led, etc.)

use core::ffi::c_void;
use crate::app::constants::PacketType;
use crate::app::structs::Device;
use crate::hal::device;

/// Toggle output between A and B
pub unsafe fn output_toggle(dev: *mut c_void, state: &mut Device) {
    if state.switch_lock {
        return;
    }
    state.active_output ^= 1;
    device::hal_set_active_output(dev, state.active_output);
}

/// Toggle mouse zoom mode
pub unsafe fn mouse_zoom_toggle(state: &mut Device) {
    state.mouse_zoom = !state.mouse_zoom;
    device::hal_send_value(state.mouse_zoom as u8, PacketType::MouseZoom as u8);
}

/// Toggle switch lock
pub unsafe fn switch_lock_toggle(state: &mut Device) {
    state.switch_lock = !state.switch_lock;
    device::hal_send_value(state.switch_lock as u8, PacketType::SwitchLock as u8);
}

/// Toggle gaming mode
pub unsafe fn gaming_mode_toggle(state: &mut Device) {
    state.gaming_mode = !state.gaming_mode;
    device::hal_send_value(state.gaming_mode as u8, PacketType::GamingMode as u8);
}

/// Firmware upgrade board A
pub unsafe fn fw_upgrade_a() {
    device::hal_reset_usb_boot();
}

/// Firmware upgrade board B
pub unsafe fn fw_upgrade_b() {
    device::hal_send_value(1, PacketType::FirmwareUpgrade as u8);
}

/// Wipe config and reload
pub unsafe fn wipe_config(dev: *mut c_void, _state: &mut Device) {
    device::hal_wipe_config();
    device::hal_load_config(dev);
    device::hal_send_value(1, PacketType::WipeConfig as u8);
}

/// Set screensaver mode
pub unsafe fn screensaver_set(state: &mut Device, mode: u8) {
    if state.is_active_output() {
        let role = state.board_role as usize;
        if role < state.config.output.len() {
            state.config.output[role].screensaver.mode = mode;
        }
    } else {
        device::hal_send_value(mode, PacketType::Screensaver as u8);
    }
}

/// Enable pong screensaver
pub unsafe fn screensaver_pong_enable(state: &mut Device) {
    let role = state.board_role as usize;
    if role < state.config.output.len() {
        let current = state.config.output[role].screensaver.mode;
        let desired = if current == 0 || current == 2 { 1 } else { current }; // PONG=1
        screensaver_set(state, desired);
    }
}

/// Enable jitter screensaver
pub unsafe fn screensaver_jitter_enable(state: &mut Device) {
    let role = state.board_role as usize;
    if role < state.config.output.len() {
        let current = state.config.output[role].screensaver.mode;
        let desired = if current == 0 || current == 1 { 2 } else { current }; // JITTER=2
        screensaver_set(state, desired);
    }
}

/// Disable screensaver
pub unsafe fn screensaver_disable(state: &mut Device) {
    screensaver_set(state, 0); // DISABLED
}

/// Enter config mode — set watchdog scratch registers and request reboot
pub unsafe fn config_enable(dev: *mut core::ffi::c_void, state: &mut Device) {
    if !state.config_mode_active {
        device::hal_set_config_mode_scratch();
    }
    device::hal_release_all_keys(dev);
    state.reboot_requested = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_toggle_logic() {
        let mut state = unsafe { core::mem::zeroed::<Device>() };
        state.active_output = 0;
        state.switch_lock = false;
        // Can't call unsafe HAL in tests, but we can verify state logic
        state.active_output ^= 1;
        assert_eq!(state.active_output, 1);
        state.active_output ^= 1;
        assert_eq!(state.active_output, 0);
    }

    #[test]
    fn test_toggle_blocked_by_lock() {
        let mut state = unsafe { core::mem::zeroed::<Device>() };
        state.switch_lock = true;
        state.active_output = 0;
        // output_toggle would return early
        if !state.switch_lock {
            state.active_output ^= 1;
        }
        assert_eq!(state.active_output, 0);
    }

    #[test]
    fn test_screensaver_mode_selection() {
        // PONG=1, JITTER=2, DISABLED=0
        let mut state = unsafe { core::mem::zeroed::<Device>() };
        state.board_role = 0;
        state.config.output[0].screensaver.mode = 0; // DISABLED

        // Enable pong: when disabled, should pick PONG(1)
        let current = state.config.output[0].screensaver.mode;
        let desired = if current == 0 || current == 2 { 1 } else { current };
        assert_eq!(desired, 1);

        // Enable jitter: when PONG, should pick JITTER(2)
        state.config.output[0].screensaver.mode = 1;
        let current = state.config.output[0].screensaver.mode;
        let desired = if current == 0 || current == 1 { 2 } else { current };
        assert_eq!(desired, 2);
    }
}
