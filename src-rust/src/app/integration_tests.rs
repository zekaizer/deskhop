// Integration tests — verify multi-module interactions.

#[cfg(test)]
mod tests {
    use crate::app::constants::*;
    use crate::app::crc;
    use crate::app::dispatch;
    use crate::app::hid_parser;
    use crate::app::hid_report;
    use crate::app::keyboard;
    use crate::app::mouse;
    use crate::app::mouse_logic;
    use crate::app::msg_handlers;
    use crate::app::packet;
    use crate::app::screensaver;
    use crate::app::state::AppState;

    /// Test full packet roundtrip: create → serialize → parse → validate
    #[test]
    fn test_packet_roundtrip_with_checksum() {
        let mut data = [0u8; PACKET_DATA_LENGTH];
        data[0] = PacketType::OutputSelect as u8;
        data[1] = 1; // output B

        let pkt = packet::UartPacket::new(PacketType::OutputSelect as u8, data);
        let raw = packet::write_raw_packet(&pkt);

        // Parse back
        let parsed = packet::parse_raw_packet(&raw).unwrap();
        assert_eq!(parsed.ptype, PacketType::OutputSelect as u8);
        assert!(parsed.verify_checksum());

        // Dispatch
        let action = dispatch::process_packet(&parsed);
        assert_eq!(action.unwrap(), dispatch::DispatchAction::OutputSelect);
    }

    /// Test CRC32 used in config validation
    #[test]
    fn test_crc32_config_pattern() {
        // Simulate config data
        let config_data = [0xB0, 0x0B, 0x1E, 0x05, 0x08, 0x00, 0x00, 0x00];
        let crc = crc::calc_crc32(&config_data);
        // Verify incremental matches batch
        let mut icrc = 0xFFFF_FFFFu32;
        for &b in &config_data {
            icrc = crc::crc32_iter(icrc, b);
        }
        assert_eq!(!icrc, crc);
    }

    /// Test mouse movement chain: position update → screen switch detection
    #[test]
    fn test_mouse_move_to_screen_edge() {
        let values = mouse_logic::MouseValues {
            move_x: -100, move_y: 0, wheel: 0, pan: 0, buttons: 0,
        };
        // Start near left edge
        let (x, _, dir) = mouse_logic::update_mouse_position(
            100, 16000, &values, 16, 28, false, false, 0,
        );
        assert_eq!(x, 0); // clamped
        assert_eq!(dir, mouse_logic::SwitchDirection::Left);
    }

    /// Test keyboard hotkey → handler action chain
    #[test]
    fn test_hotkey_to_handler_action() {
        let mut state = AppState::new();
        state.active_output = 0;

        // Simulate output select message
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = msg_handlers::handle_simple_msg(
            PacketType::OutputSelect as u8, &data, &state,
        );
        let needs_hal = msg_handlers::apply_action(&action, &mut state);
        assert!(needs_hal);
        assert_eq!(state.active_output, 1);
    }

    /// Test HID report value extraction from parsed descriptor
    #[test]
    fn test_hid_parse_then_extract() {
        // Simple 3-button mouse descriptor
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0x05, 0x01, 0x09, 0x02, 0xA1, 0x01,
            0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
            0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01,
            0x81, 0x02, // buttons
            0x95, 0x01, 0x75, 0x05, 0x81, 0x01, // padding
            0x05, 0x01, 0x09, 0x30, 0x09, 0x31,
            0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x02,
            0x81, 0x06, // X, Y
            0xC0,
        ];

        let (_, results) = hid_parser::parse_descriptor(desc);
        assert!(results.len() >= 3);

        // Simulate a mouse report: buttons=0x05, X=-10, Y=20
        let report = [0x05u8, 0xF6, 0x14]; // 3 buttons set, X=-10, Y=20

        // Extract X value (8 bits at offset 8)
        let x = hid_report::get_report_value(&report, 8, 8);
        assert_eq!(x, -10); // sign-extended

        let y = hid_report::get_report_value(&report, 16, 8);
        assert_eq!(y, 20);
    }

    /// Test screensaver pong stays within bounds during extended run
    #[test]
    fn test_screensaver_pong_extended() {
        let mut pong = screensaver::PongState::new();
        for _ in 0..100_000 {
            let r = pong.step();
            assert!(r.x >= MIN_SCREEN_COORD && r.x <= MAX_SCREEN_COORD);
            assert!(r.y >= MIN_SCREEN_COORD && r.y <= MAX_SCREEN_COORD);
        }
    }

    /// Test keyboard state combine with full report slots
    #[test]
    fn test_kbd_combine_overflow() {
        use crate::app::kbd_state;
        use crate::app::structs::HidKeyboardReport;

        let mut state = AppState::new();
        // Fill all 6 slots in keyboard 0
        state.local_kbd_states[0] = HidKeyboardReport {
            modifier: 0xFF, reserved: 0,
            keycode: [0x04, 0x05, 0x06, 0x07, 0x08, 0x09],
        };
        state.max_kbd_idx = 0;
        // Try adding more from remote
        state.remote_kbd_state = HidKeyboardReport {
            modifier: 0, reserved: 0,
            keycode: [0x0A, 0x0B, 0, 0, 0, 0],
        };

        let combined = kbd_state::combine_kbd_states(&state);
        assert_eq!(combined.modifier, 0xFF);
        // Should have 6 keys from local, no room for remote
        assert_eq!(combined.keycode.iter().filter(|&&k| k != 0).count(), 6);
    }

    /// Test full message handler chain for firmware upgrade
    #[test]
    fn test_fw_upgrade_chain() {
        let mut state = AppState::new();
        state.running_fw.version = 100;

        // Heartbeat with newer version triggers upgrade
        let data = [200u8, 0, 0, 0, 0, 0, 0, 0];
        let action = msg_handlers::handle_simple_msg(
            PacketType::Heartbeat as u8, &data, &state,
        );
        msg_handlers::apply_action(&action, &mut state);

        assert!(state.fw.upgrade_in_progress);
        assert!(state.fw.byte_done);
        assert_eq!(state.fw.address, 0);
        assert_eq!(state.fw.checksum, 0xFFFF_FFFF);
    }

    /// Test packet validation rejects invalid types
    #[test]
    fn test_invalid_packet_rejected() {
        let pkt = packet::UartPacket { ptype: 99, data: [0; 8], checksum: 0 };
        let result = dispatch::validate_received_packet(&pkt);
        assert!(result.is_err());
    }

    /// Test mouse acceleration factor is monotonically increasing
    #[test]
    fn test_acceleration_monotonic() {
        let mut prev = 1.0f32;
        for speed in [5, 15, 30, 45, 60, 70] {
            let factor = mouse::calculate_mouse_acceleration_factor(speed, 0, true);
            assert!(factor >= prev, "Factor should increase: {} < {} at speed {}", factor, prev, speed);
            prev = factor;
        }
    }
}
