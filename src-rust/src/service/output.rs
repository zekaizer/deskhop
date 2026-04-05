// Output switching — updates active output, syncs indicators, notifies peer.

use crate::domain::constants;
use crate::domain::structs::{DeviceConfig, DeviceHid};

/// Switch active output and synchronize all state.
pub fn switch_output(
    cfg: &mut DeviceConfig,
    hid: &DeviceHid,
    output: u8,
    set_board_led: impl Fn(bool),
    send_kbd_leds: impl Fn(u8, u8, &u8, u8),
    notify_peer: impl Fn(u8, u8),
    release_keys: impl Fn(),
) {
    cfg.active_output = output;
    super::led::sync_indicators(cfg, hid, set_board_led, send_kbd_leds);
    notify_peer(output, constants::PacketType::OutputSelect as u8);
    release_keys();
}
