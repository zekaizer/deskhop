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
        let leds = cfg.keyboard_leds[cfg.active_output as usize];
        send_kbd_leds(hid.kbd_dev_addr, hid.kbd_instance, &leds, 1);
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
