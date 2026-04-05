// USB callback business logic — extracted from hal/ffi/callbacks.rs.
// All functions operate on DeviceState + HAL traits, no raw pointers.

use crate::domain::constants;
use crate::domain::hid_routing;
use crate::domain::structs::{DeviceConfig, DeviceState, HidInterface};
use crate::hal::traits::*;

/// Mount parameters resolved from raw FFI values.
pub struct MountParams {
    pub dev_addr: u8,
    pub instance: u8,
    pub itf_protocol: u8,
}

/// HID mount business logic — protocol negotiation, config checks.
/// Returns Some(protocol) if set_protocol should be called, None otherwise.
pub fn on_hid_mount(
    state: &mut DeviceState<'_>,
    hal: &(impl Indicator + PeerLink),
    iface: &mut HidInterface,
    params: &MountParams,
) -> Option<u8> {
    let mut set_proto = None;

    match params.itf_protocol {
        constants::HID_ITF_PROTOCOL_KEYBOARD => {
            if state.cfg.config.enforce_ports != 0
                && state.cfg.board_role == constants::OUTPUT_B
            {
                return None;
            }

            if state.cfg.config.force_kbd_boot_protocol != 0 {
                set_proto = Some(constants::HID_PROTOCOL_BOOT);
            }

            state.hid.kbd_dev_addr = params.dev_addr;
            state.hid.kbd_instance = params.instance;
            state.cfg.keyboard_connected = true;
        }

        constants::HID_ITF_PROTOCOL_MOUSE => {
            if state.cfg.config.enforce_ports != 0
                && state.cfg.board_role == constants::OUTPUT_A
            {
                return None;
            }

            if state.cfg.config.force_mouse_boot_mode != 0 {
                set_proto = Some(constants::HID_PROTOCOL_BOOT);
            } else if iface.protocol == constants::HID_PROTOCOL_BOOT {
                set_proto = Some(constants::HID_PROTOCOL_REPORT);
            }

            state.cfg.mouse_connected = true;
        }

        _ => {}
    }

    // Composite devices (e.g. QMK) may expose mouse via keyboard interface
    if iface.mouse.is_found {
        state.cfg.mouse_connected = true;
    }

    hal.blink();
    hal.send_value(constants::ENABLE, constants::PacketType::FlashLed as u8);

    set_proto
}

/// HID umount business logic — clear connection flags.
pub fn on_hid_umount(cfg: &mut DeviceConfig, itf_protocol: u8) {
    match itf_protocol {
        constants::HID_ITF_PROTOCOL_KEYBOARD => {
            cfg.keyboard_connected = false;
        }
        constants::HID_ITF_PROTOCOL_MOUSE => {
            cfg.mouse_connected = false;
        }
        _ => {}
    }
}

/// Process keyboard LED state from set_report callback.
pub fn process_led_report(
    state: &mut DeviceState<'_>,
    hal: &(impl OutputControl + PeerLink),
    led_byte: u8,
) {
    let leds = hid_routing::process_led_state(
        led_byte,
        state.cfg.config.kbd_led_as_indicator != 0,
        state.cfg.active_output,
    );

    state.cfg.keyboard_leds[state.cfg.board_role as usize] = leds;

    if state.cfg.keyboard_connected && state.is_active_output() {
        hal.sync_leds();
    }

    hal.send_value(leds, constants::PacketType::KbdSetReport as u8);
}

/// Returns true if config mode is active (caller selects descriptor pointer).
pub fn is_config_mode(cfg: &DeviceConfig) -> bool {
    cfg.config_mode_active
}
