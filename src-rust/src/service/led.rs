// Indicator synchronization — board LED + keyboard LED state management.

use crate::domain::structs::{DeviceConfig, DeviceHid};

/// Synchronize all indicators after output switch.
/// Updates onboard LED to reflect active output and sends keyboard
/// LED report (caps lock, etc.) for the newly active output.
pub fn sync_indicators(
    cfg: &mut DeviceConfig,
    hid: &DeviceHid,
    set_board_led: impl Fn(bool),
    send_kbd_leds: impl Fn(u8, u8, &u8, u8),
) {
    let is_active = cfg.active_output == cfg.board_role;
    cfg.onboard_led_state = is_active;
    set_board_led(is_active);

    if cfg.keyboard_connected {
        let leds = cfg.keyboard_leds_desired[cfg.active_output as usize];
        send_kbd_leds(hid.kbd_dev_addr, hid.kbd_instance, &leds, 1);
    }
}

/// Return the LED state to (re-)send to the local keyboard, or None if it
/// already matches. tuh_hid_set_report can fail (endpoint busy, not mounted);
/// the periodic led_sync_task re-sends until the keyboard accepted the
/// desired state for the active output (upstream v0.78 led_sync_task).
pub fn led_sync_needed(cfg: &crate::domain::structs::DeviceConfig) -> Option<u8> {
    if !cfg.keyboard_connected {
        return None;
    }
    let desired = cfg.keyboard_leds_desired[cfg.active_output as usize];
    if cfg.keyboard_leds_actual != desired {
        Some(desired)
    } else {
        None
    }
}

/// Send keyboard LED state if keyboard is connected.
pub fn send_kbd_led_report(
    cfg: &DeviceConfig,
    hid: &DeviceHid,
    leds: u8,
    send_kbd_leds: impl Fn(u8, u8, &u8, u8),
) {
    if cfg.keyboard_connected {
        send_kbd_leds(hid.kbd_dev_addr, hid.kbd_instance, &leds, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::structs::DeviceConfig;

    fn cfg_with_kbd() -> DeviceConfig {
        let mut cfg = DeviceConfig::default();
        cfg.keyboard_connected = true;
        cfg.active_output = 1;
        cfg.keyboard_leds_desired[1] = 0x07;
        cfg.keyboard_leds_actual = 0;
        cfg
    }

    #[test]
    fn sync_needed_when_actual_differs_from_desired() {
        let cfg = cfg_with_kbd();
        assert_eq!(led_sync_needed(&cfg), Some(0x07));
    }

    #[test]
    fn sync_not_needed_when_actual_matches() {
        let mut cfg = cfg_with_kbd();
        cfg.keyboard_leds_actual = 0x07;
        assert_eq!(led_sync_needed(&cfg), None);
    }

    #[test]
    fn sync_not_needed_without_keyboard() {
        let mut cfg = cfg_with_kbd();
        cfg.keyboard_connected = false;
        assert_eq!(led_sync_needed(&cfg), None);
    }

    #[test]
    fn sync_tracks_active_output_not_board_role() {
        // Desired state follows the ACTIVE output's host, not this board's own.
        let mut cfg = cfg_with_kbd();
        cfg.keyboard_leds_desired[0] = 0x01;
        cfg.active_output = 0;
        assert_eq!(led_sync_needed(&cfg), Some(0x01));
    }
}
