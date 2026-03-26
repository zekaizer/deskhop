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

/// Consumer control report length (output buffer size).
pub const CONSUMER_CONTROL_LENGTH: usize = 4;

/// Parse a consumer control HID report into a normalized 4-byte report.
///
/// For variable-mode reports: scans bits in the raw data and looks up the
/// corresponding usage code from the keyboard descriptor's cc_array.
/// For array-mode reports: copies raw bytes directly (skipping report ID).
pub fn parse_consumer_report(
    raw: &[u8],
    is_variable: bool,
    cc_array: &[u16],
) -> [u8; CONSUMER_CONTROL_LENGTH] {
    let mut out = [0u8; CONSUMER_CONTROL_LENGTH];

    if is_variable {
        let max_buttons = 16i32; // MAX_CC_BUTTONS
        let max_bits = 8 * (raw.len() as i32 - 1);
        let limit = core::cmp::min(max_buttons, max_bits);

        for i in 0..limit {
            let bit_idx = i % 8;
            let byte_idx = (i >> 3) + 1; // +1 to skip report_id
            if byte_idx < raw.len() as i32
                && (raw[byte_idx as usize] >> bit_idx) & 1 != 0
            {
                let idx = i as usize;
                if idx < cc_array.len() {
                    let cc_val = cc_array[idx];
                    out[0] = (cc_val & 0xFF) as u8;
                    out[1] = ((cc_val >> 8) & 0xFF) as u8;
                }
            }
        }
    } else {
        let data_len = core::cmp::min(raw.len().saturating_sub(1), CONSUMER_CONTROL_LENGTH);
        for i in 0..data_len {
            out[i] = raw[i + 1];
        }
    }

    out
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

    #[test]
    fn test_parse_consumer_report_array_mode() {
        // Array mode: report_id + 2 bytes of usage data
        let raw = [0x01, 0xE9, 0x00]; // report_id=1, volume_up=0x00E9
        let cc_array = [0u16; 16];
        let result = parse_consumer_report(&raw, false, &cc_array);
        assert_eq!(result, [0xE9, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_parse_consumer_report_variable_mode() {
        // Variable mode: report_id + bitmap, bit 0 set -> cc_array[0]
        let raw = [0x01, 0x01]; // report_id=1, bit 0 set
        let mut cc_array = [0u16; 16];
        cc_array[0] = 0x00E9; // Volume Up
        let result = parse_consumer_report(&raw, true, &cc_array);
        assert_eq!(result[0], 0xE9);
        assert_eq!(result[1], 0x00);
    }

    #[test]
    fn test_parse_consumer_report_variable_mode_bit3() {
        // Variable mode: bit 3 set -> cc_array[3]
        let raw = [0x01, 0x08]; // report_id=1, bit 3 set
        let mut cc_array = [0u16; 16];
        cc_array[3] = 0x0183; // App Launch
        let result = parse_consumer_report(&raw, true, &cc_array);
        assert_eq!(result[0], 0x83);
        assert_eq!(result[1], 0x01);
    }

    #[test]
    fn test_parse_consumer_report_empty() {
        let raw = [0x01, 0x00]; // No bits set
        let cc_array = [0u16; 16];
        let result = parse_consumer_report(&raw, true, &cc_array);
        assert_eq!(result, [0, 0, 0, 0]);
    }
}
