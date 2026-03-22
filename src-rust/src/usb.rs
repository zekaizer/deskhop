/// HID interface protocol types (from TinyUSB)
pub const HID_ITF_PROTOCOL_NONE: u8 = 0;
pub const HID_ITF_PROTOCOL_KEYBOARD: u8 = 1;
pub const HID_ITF_PROTOCOL_MOUSE: u8 = 2;

/// Maximum devices and interfaces (must match C defines)
pub const MAX_DEVICES: u8 = 4;
pub const MAX_INTERFACES: u8 = 12;
pub const MAX_REPORTS: u8 = 24;

/// Calculate device index for HID report routing.
///
/// Device index assignment:
/// - 0: Primary keyboard (matching kbd_dev_addr/kbd_instance)
/// - 1: Mouse devices
/// - MAX_DEVICES-2: Secondary keyboards (e.g. unified dongle)
/// - (dev_addr-1) % (MAX_DEVICES-1): Other devices
///
/// Slot MAX_DEVICES-1 is reserved for remote device.
pub fn calculate_device_idx(
    itf_protocol: u8,
    dev_addr: u8,
    instance: u8,
    kbd_dev_addr: u8,
    kbd_instance: u8,
) -> u8 {
    if itf_protocol == HID_ITF_PROTOCOL_KEYBOARD {
        if dev_addr == kbd_dev_addr && instance == kbd_instance {
            0 // Primary keyboard
        } else {
            MAX_DEVICES - 2 // Secondary keyboard
        }
    } else if itf_protocol == HID_ITF_PROTOCOL_MOUSE {
        1 // Mouse
    } else {
        (dev_addr - 1) % (MAX_DEVICES - 1) // Other
    }
}

/// Validate device address and instance bounds
pub fn validate_device_bounds(dev_addr: u8, instance: u8) -> bool {
    dev_addr > 0 && dev_addr <= MAX_DEVICES && instance < MAX_INTERFACES
}

/// LED state processing for tud_hid_set_report_cb.
/// Applies caps lock indicator override if configured.
pub fn process_led_state(leds: u8, kbd_led_as_indicator: bool, active_output: u8) -> u8 {
    if !kbd_led_as_indicator {
        return leds;
    }

    const KEYBOARD_LED_CAPSLOCK: u8 = 0x02;
    let mut result = leds & 0xFD; // Clear caps lock bit

    if active_output != 0 {
        result |= KEYBOARD_LED_CAPSLOCK;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_idx_primary_keyboard() {
        assert_eq!(calculate_device_idx(HID_ITF_PROTOCOL_KEYBOARD, 1, 0, 1, 0), 0);
    }

    #[test]
    fn test_device_idx_secondary_keyboard() {
        assert_eq!(
            calculate_device_idx(HID_ITF_PROTOCOL_KEYBOARD, 2, 0, 1, 0),
            MAX_DEVICES - 2
        );
    }

    #[test]
    fn test_device_idx_mouse() {
        assert_eq!(calculate_device_idx(HID_ITF_PROTOCOL_MOUSE, 1, 0, 1, 0), 1);
    }

    #[test]
    fn test_device_idx_other() {
        assert_eq!(
            calculate_device_idx(HID_ITF_PROTOCOL_NONE, 3, 0, 1, 0),
            (3 - 1) % (MAX_DEVICES - 1)
        );
    }

    #[test]
    fn test_validate_device_bounds() {
        assert!(validate_device_bounds(1, 0));
        assert!(validate_device_bounds(4, 11));
        assert!(!validate_device_bounds(0, 0)); // dev_addr 0 invalid
        assert!(!validate_device_bounds(5, 0)); // exceeds MAX_DEVICES
        assert!(!validate_device_bounds(1, 12)); // exceeds MAX_INTERFACES
    }

    #[test]
    fn test_process_led_state_no_indicator() {
        assert_eq!(process_led_state(0xFF, false, 0), 0xFF);
    }

    #[test]
    fn test_process_led_state_indicator_output_a() {
        // active_output=0 (A), caps lock should be cleared
        let result = process_led_state(0xFF, true, 0);
        assert_eq!(result & 0x02, 0); // caps lock bit cleared
    }

    #[test]
    fn test_process_led_state_indicator_output_b() {
        // active_output=1 (B), caps lock should be set
        let result = process_led_state(0x00, true, 1);
        assert_eq!(result & 0x02, 0x02); // caps lock bit set
    }
}
