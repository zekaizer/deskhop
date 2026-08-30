// Integration tests — verify multi-module interactions.

#[cfg(test)]
mod tests {
    use crate::domain::constants::*;
    use crate::domain::crc;
    use crate::domain::dispatch;
    use crate::domain::hid_parser;
    use crate::domain::hid_report;
    use crate::domain::keyboard;
    use crate::domain::mouse;
    use crate::domain::mouse_logic;
    use crate::domain::msg_handlers;
    use crate::domain::packet;
    use crate::domain::screensaver;
    use crate::domain::structs::DeviceState;

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
            100, 16000, &values, 16, 28, false, false, 0, 0, 1,
        );
        assert_eq!(x, 0); // clamped
        assert_eq!(dir, mouse_logic::SwitchDirection::Left);
    }

    /// Test keyboard hotkey → handler action chain
    #[test]
    fn test_hotkey_to_handler_action() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.active_output = 0;

        // Simulate output select message
        let data = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let action = msg_handlers::handle_simple_msg(
            PacketType::OutputSelect as u8, &data, &state,
        );
        let needs_hal = msg_handlers::apply_action(&action, &mut state);
        assert!(needs_hal);
        assert_eq!(state.cfg.active_output, 1);
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
        use crate::domain::kbd_state;
        use crate::domain::structs::HidKeyboardReport;

        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // Fill all 6 slots in keyboard 0
        state.hid.local_kbd_states[0] = HidKeyboardReport {
            modifier: 0xFF, reserved: 0,
            keycode: [0x04, 0x05, 0x06, 0x07, 0x08, 0x09],
        };
        state.hid.max_kbd_idx = 0;
        // Try adding more from remote
        state.hid.remote_kbd_state = HidKeyboardReport {
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
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw._running_fw.version = 100;

        // Heartbeat with newer version triggers upgrade
        let data = [200u8, 0, 0, 0, 0, 0, 0, 0];
        let action = msg_handlers::handle_simple_msg(
            PacketType::Heartbeat as u8, &data, &state,
        );
        msg_handlers::apply_action(&action, &mut state);

        assert!(state.fw.fw.upgrade_in_progress);
        assert!(state.fw.fw.byte_done);
        assert_eq!(state.fw.fw.address, 0);
        assert_eq!(state.fw.fw.checksum, 0xFFFF_FFFF);
    }

    /// Test packet validation rejects invalid types
    #[test]
    fn test_invalid_packet_rejected() {
        let pkt = packet::UartPacket { ptype: 99, data: [0; 8], checksum: 0 };
        let result = dispatch::validate_received_packet(&pkt);
        assert!(result.is_err());
    }

    /// Test mouse acceleration factor is monotonically increasing (fixed-point ×256)
    #[test]
    fn test_acceleration_monotonic() {
        let mut prev = 256i32; // 1.0 in fixed-point
        for speed in [5, 15, 30, 45, 60, 70] {
            let factor = mouse::calculate_mouse_acceleration_factor_fp(speed, 0, true);
            assert!(factor >= prev, "Factor should increase: {} < {} at speed {}", factor, prev, speed);
            prev = factor;
        }
    }

    /// Test DeviceState is_active_output helper
    #[test]
    fn test_app_state_active_output() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.board_role = 0;
        state.cfg.active_output = 0;
        assert!(state.is_active_output());
        state.cfg.active_output = 1;
        assert!(!state.is_active_output());
        state.cfg.board_role = 1;
        assert!(state.is_active_output());
    }

    /// Test screensaver jitter alternation over many cycles
    #[test]
    fn test_jitter_stability() {
        let mut jitter = screensaver::JitterState::new();
        for i in 0..1000 {
            let r = jitter.step();
            let expected = if i % 2 == 0 { -2 } else { 2 };
            assert_eq!(r.y, expected, "Jitter failed at step {}", i);
        }
    }

    /// Test extract classify covers all HID usage types
    #[test]
    fn test_extract_classify_all_types() {
        use crate::domain::hid_classify::*;
        use crate::domain::hid_parser::*;

        let types = [
            (HID_USAGE_PAGE_BUTTON, HID_USAGE_DESKTOP_MOUSE, 0, ExtractedType::MouseButtons),
            (HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_X, ExtractedType::MouseX),
            (HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_Y, ExtractedType::MouseY),
            (HID_USAGE_PAGE_DESKTOP, HID_USAGE_DESKTOP_MOUSE, HID_USAGE_DESKTOP_WHEEL, ExtractedType::MouseWheel),
            (HID_USAGE_PAGE_KEYBOARD, HID_USAGE_DESKTOP_KEYBOARD, 0, ExtractedType::Keyboard),
        ];

        for (up, gu, u, expected) in types {
            let val = ReportVal { usage_page: up, global_usage: gu, usage: u, ..ReportVal::default() };
            assert_eq!(classify_report_val(&val), expected);
        }
    }

    /// Test full keyboard report → handler → state update chain
    #[test]
    fn test_kbd_report_to_state_update() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let data = [0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]; // LEFT_CTRL + 'a'
        msg_handlers::handle_keyboard_uart(&data, &mut state);
        assert_eq!(state.hid.remote_kbd_state.modifier, 0x01);
        assert_eq!(state.hid.remote_kbd_state.keycode[0], 0x04);
    }

    /// Test mouse zoom toggle via handler
    #[test]
    fn test_zoom_toggle_roundtrip() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        assert!(!state.cfg.mouse_zoom);
        msg_handlers::apply_action(&msg_handlers::HandlerAction::SetMouseZoom(true), &mut state);
        assert!(state.cfg.mouse_zoom);
        msg_handlers::apply_action(&msg_handlers::HandlerAction::SetMouseZoom(false), &mut state);
        assert!(!state.cfg.mouse_zoom);
    }

    /// Test HID parser with keyboard + mouse composite descriptor
    #[test]
    fn test_hid_parse_composite() {
        // Minimal keyboard descriptor followed by mouse
        #[rustfmt::skip]
        let desc: &[u8] = &[
            // Keyboard
            0x05, 0x01, 0x09, 0x06, 0xA1, 0x01,
            0x85, 0x01, // Report ID 1
            0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7,
            0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08,
            0x81, 0x02, // modifiers
            0xC0,
            // Mouse
            0x05, 0x01, 0x09, 0x02, 0xA1, 0x01,
            0x85, 0x02, // Report ID 2
            0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
            0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01,
            0x81, 0x02, // buttons
            0xC0,
        ];
        let (_, results) = hid_parser::parse_descriptor(desc);
        assert!(results.len() >= 2);
        // Both should have uses_report_id set
        for input in results.iter() {
            assert!(input.uses_report_id);
        }
    }

    /// Test screen switch decision matrix
    #[test]
    fn test_screen_switch_matrix() {
        use mouse_logic::*;

        let ctx = SwitchContext { switch_lock: false, gaming_mode: false, mouse_buttons: 0,
            screen_pos: 2, screen_index: 1, screen_count: 1 };
        assert_eq!(decide_screen_switch(SwitchDirection::Left, &ctx), ScreenSwitchAction::SwitchToOtherPc);

        let ctx2 = SwitchContext { mouse_buttons: 1, ..ctx };
        assert_eq!(decide_screen_switch(SwitchDirection::Left, &ctx2), ScreenSwitchAction::Nothing);

        let ctx3 = SwitchContext { screen_index: 2, screen_count: 3, ..ctx };
        assert_eq!(decide_screen_switch(SwitchDirection::Left, &ctx3),
            ScreenSwitchAction::SwitchVirtualDesktop { new_index: 1 });
    }

    /// Test CRC32 of known firmware metadata pattern
    #[test]
    fn test_crc32_firmware_pattern() {
        let metadata = [0x0d, 0xf0, 0x00, 0x00, 0x01, 0x00]; // magic + version
        let crc = crc::calc_crc32(&metadata);
        assert_ne!(crc, 0);
        // Same data should always produce same CRC
        assert_eq!(crc, crc::calc_crc32(&metadata));
    }

    /// Test all packet types are dispatchable
    #[test]
    fn test_all_valid_packet_types_dispatch() {
        let valid_types: &[u8] = &[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,18,19,20,21,22,23,24,25];
        for &t in valid_types {
            let data = [0u8; 8];
            let checksum = crc::calc_checksum(&data);
            let pkt = packet::UartPacket { ptype: t, data, checksum };
            assert!(dispatch::process_packet(&pkt).is_ok(), "Type {} failed", t);
        }
    }

    /// Test DeviceState feature flag toggle sequence
    #[test]
    fn test_feature_flag_sequence() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        // Toggle gaming mode multiple times
        msg_handlers::apply_action(&msg_handlers::HandlerAction::SetGamingMode(true), &mut state);
        assert!(state.cfg.gaming_mode);
        msg_handlers::apply_action(&msg_handlers::HandlerAction::SetGamingMode(false), &mut state);
        assert!(!state.cfg.gaming_mode);
        // Switch lock
        msg_handlers::apply_action(&msg_handlers::HandlerAction::SetSwitchLock(true), &mut state);
        assert!(state.cfg.switch_lock);
        // Toggle output should be blocked
        msg_handlers::apply_action(&msg_handlers::HandlerAction::ToggleOutput, &mut state);
        assert_eq!(state.cfg.active_output, 0); // unchanged due to lock
    }

    /// Test HID report extraction with different bit sizes
    #[test]
    fn test_hid_extract_various_sizes() {
        let report = [0xFF, 0x80, 0x7F, 0x00];
        // 1 bit at offset 0
        assert_eq!(hid_report::get_report_value(&report, 0, 1), -1);
        // 4 bits at offset 0
        assert_eq!(hid_report::get_report_value(&report, 0, 4), -1); // 0xF sign-extended
        // 16 bits at offset 0
        assert_eq!(hid_report::get_report_value(&report, 0, 16), -32513); // 0x80FF sign-extended
    }

    /// Test mouse acceleration at exact curve points (fixed-point ×256)
    #[test]
    fn test_acceleration_curve_points() {
        // At 0 movement → 256 (1.0)
        assert_eq!(mouse::calculate_mouse_acceleration_factor_fp(0, 0, true), 256);
        // At very small movement (below first point) → 256 (1.0)
        let f = mouse::calculate_mouse_acceleration_factor_fp(1, 0, true);
        assert_eq!(f, 256);
        // Very large movement (above last point) → 1024 (4.0)
        let f = mouse::calculate_mouse_acceleration_factor_fp(100, 0, true);
        assert_eq!(f, 1024);
    }

    /// Test mouse Y scaling symmetry
    #[test]
    fn test_y_scale_symmetry() {
        // Same borders → identity
        let y = mouse::scale_y_coordinate(16383, (0, 0), (0, 0));
        assert_eq!(y, 16383);

        // Different borders → value changes
        let y2 = mouse::scale_y_coordinate(16383, (0, 0), (8000, 0));
        assert_ne!(y2, 16383);
        assert!(y2 > 8000 && y2 < 24000);
    }

    // ================================================================
    // End-to-end scenario tests
    // ================================================================

    /// Test screen switch + border Y scaling end-to-end.
    /// When pointer hits the screen edge and triggers a switch, the Y
    /// coordinate must be proportionally scaled between the two outputs'
    /// border configurations.
    #[test]
    fn test_screen_switch_with_border_y_scaling() {
        use crate::domain::mouse_logic;

        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };

        // Output 0: border top=-10000, bottom=10000 → usable range = 32767 - (-10000) - 10000 = 32767
        // (borders are subtracted from MAX_SCREEN_COORD to get usable range)
        // Actually: from_range = 32767 - from_top - from_bottom
        // So top=0 bottom=0 gives range=32767, top=5000 bottom=5000 gives range=22767
        state.cfg.config.output[0].border.top = 0;
        state.cfg.config.output[0].border.bottom = 0;
        state.cfg.config.output[1].border.top = 5000;
        state.cfg.config.output[1].border.bottom = 5000;

        // Pointer at Y=16000 on output 0
        state.hid.pointer_y = 16000;
        state.cfg.active_output = 0;

        // Step 1: Detect screen switch — pointer hits left edge
        let values = mouse_logic::MouseValues {
            move_x: -200, move_y: 0, wheel: 0, pan: 0, buttons: 0,
        };
        let (x, _y, dir) = mouse_logic::update_mouse_position(
            50, state.hid.pointer_y, &values, 16, 28, false, false, 0, 0, 1,
        );
        assert_eq!(x, 0); // clamped to edge
        assert_eq!(dir, mouse_logic::SwitchDirection::Left);

        // Step 2: Scale Y for the target output's borders
        let scaled_y = mouse::scale_y_coordinate(
            state.hid.pointer_y,
            (state.cfg.config.output[0].border.top, state.cfg.config.output[0].border.bottom),
            (state.cfg.config.output[1].border.top, state.cfg.config.output[1].border.bottom),
        );

        // Output 0 range = 32767-0-0 = 32767, output 1 range = 32767-5000-5000 = 22767
        // relative = 16000 - 0 = 16000
        // scaled = 16000 * 22767 / 32767 ≈ 11117
        // result = 11117 + 5000 = 16117
        assert!(scaled_y > 5000, "Scaled Y must be above output 1 top border, got {}", scaled_y);
        assert!(scaled_y < 32767 - 5000, "Scaled Y must be below output 1 bottom border, got {}", scaled_y);
        // Proportionality: roughly 16000/32767 ≈ 0.488 of output 1 range + top
        let expected_approx = 5000 + ((16000i64 * 22767 / 32767) as i16);
        assert!((scaled_y - expected_approx).abs() <= 1,
            "Scaled Y {} should be close to expected {}", scaled_y, expected_approx);
    }

    /// Full keyboard pipeline: report → hotkey detection → state combination.
    /// Verifies that hotkey detection and kbd_state combine work together
    /// through a realistic multi-keyboard scenario.
    #[test]
    fn test_full_keyboard_pipeline() {
        use crate::domain::kbd_state;
        use crate::domain::structs::HidKeyboardReport;

        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };

        // Keyboard 0: hotkey modifier + hotkey key (LEFT_CTRL + CAPS_LOCK)
        let hotkey_report = HidKeyboardReport {
            modifier: HOTKEY_MODIFIER, // LEFT_CTRL
            reserved: 0,
            keycode: [HOTKEY_TOGGLE, 0, 0, 0, 0, 0], // CAPS_LOCK
        };
        kbd_state::update_kbd_state(&mut state, &hotkey_report, 0);

        // Check hotkey detection on the report we just stored
        let hotkey_match = keyboard::check_all_hotkeys(&state.hid.local_kbd_states[0]);
        assert!(hotkey_match.is_some(), "Hotkey should be detected");
        assert_eq!(hotkey_match.unwrap().action, keyboard::HotkeyAction::OutputToggle);

        // Keyboard 1: normal typing (no hotkey)
        let normal_report = HidKeyboardReport {
            modifier: 0,
            reserved: 0,
            keycode: [HID_KEY_A, HID_KEY_B, 0, 0, 0, 0],
        };
        kbd_state::update_kbd_state(&mut state, &normal_report, 1);

        // No hotkey in the normal report
        let no_hotkey = keyboard::check_all_hotkeys(&state.hid.local_kbd_states[1]);
        assert!(no_hotkey.is_none(), "Normal report should not trigger hotkey");

        // Remote keyboard state
        state.hid.remote_kbd_state = HidKeyboardReport {
            modifier: KEYBOARD_MODIFIER_RIGHTSHIFT,
            reserved: 0,
            keycode: [HID_KEY_C, 0, 0, 0, 0, 0],
        };

        // Combine all keyboard states
        let combined = kbd_state::combine_kbd_states(&state);

        // Modifiers: LEFT_CTRL | RIGHT_SHIFT = 0x01 | 0x20 = 0x21
        assert_eq!(combined.modifier, HOTKEY_MODIFIER | KEYBOARD_MODIFIER_RIGHTSHIFT);

        // Keys: CAPS_LOCK, A, B, C (from kbd0, kbd1, remote)
        assert!(combined.keycode.contains(&HOTKEY_TOGGLE));
        assert!(combined.keycode.contains(&HID_KEY_A));
        assert!(combined.keycode.contains(&HID_KEY_B));
        assert!(combined.keycode.contains(&HID_KEY_C));
    }

    /// Packet roundtrip: create → encode → parse → validate → dispatch.
    /// Verifies the full lifecycle of a packet with non-trivial data
    /// through every stage of the packet processing pipeline.
    #[test]
    fn test_packet_full_lifecycle() {
        use crate::domain::crc::calc_checksum;

        // Create a SyncBorders packet with border data
        let mut data = [0u8; PACKET_DATA_LENGTH];
        data[0] = 0; // output index
        // Encode border top = 1000 as little-endian i16 in data[1..3]
        data[1] = 0xE8; // 1000 & 0xFF
        data[2] = 0x03; // 1000 >> 8
        // Encode border bottom = 2000 as little-endian i16 in data[3..5]
        data[3] = 0xD0; // 2000 & 0xFF
        data[4] = 0x07; // 2000 >> 8

        let pkt = packet::UartPacket::new(PacketType::SyncBorders as u8, data);

        // Step 1: Encode to wire format
        let raw = packet::write_raw_packet(&pkt);
        assert_eq!(raw[0], START1);
        assert_eq!(raw[1], START2);
        assert_eq!(raw[2], PacketType::SyncBorders as u8);

        // Step 2: Parse back from wire format
        let parsed = packet::parse_raw_packet(&raw).unwrap();
        assert_eq!(parsed.ptype, PacketType::SyncBorders as u8);
        assert_eq!(parsed.data, data);

        // Step 3: Validate checksum and type
        let ptype = dispatch::validate_received_packet(&parsed).unwrap();
        assert_eq!(ptype, PacketType::SyncBorders);
        assert!(parsed.verify_checksum());

        // Step 4: Get dispatch action
        let action = dispatch::get_dispatch_action(ptype);
        assert_eq!(action, dispatch::DispatchAction::SyncBorders);

        // Step 5: Verify data integrity — raw bytes and typed access
        assert_eq!(parsed.data[0], 0); // output index preserved
        assert_eq!(parsed.data[1], 0xE8);
        assert_eq!(parsed.data[2], 0x03);
        // data16(0) reads data[0..2] as LE u16 → 0xE800 (output_idx=0, first border byte)
        assert_eq!(parsed.data16(0), u16::from_le_bytes([data[0], data[1]]));
        // Verify border top=1000 can be reconstructed from data[1..3]
        let border_top = u16::from_le_bytes([parsed.data[1], parsed.data[2]]);
        assert_eq!(border_top, 1000);
    }

    /// Screensaver activation → report generation → position tracking.
    /// Tests the full screensaver lifecycle from idle detection through
    /// cursor movement generation.
    #[test]
    fn test_screensaver_lifecycle() {
        // Configure pong screensaver with 1 second idle time
        let config = screensaver::ScreensaverConfig {
            mode: 1, // Pong
            only_if_inactive: false,
            idle_time_us: 1_000_000, // 1 sec
            max_time_us: 0,          // no max
        };

        // Not idle long enough — should NOT activate
        let should = screensaver::should_activate(
            &config,
            500_000,  // 0.5s inactivity — not enough
            false,    // not active output
            true,     // USB ready
            0,        // last_move
            100_000,  // current time
        );
        assert!(!should, "Should not activate when idle time not reached");

        // Idle long enough — should activate (with enough delay since last move)
        let should = screensaver::should_activate(
            &config,
            2_000_000, // 2s inactivity — exceeds 1s threshold
            false,
            true,
            0,        // last_move at t=0
            100_000,  // current_time at t=100ms (delta=100ms > 5ms pong delay)
        );
        assert!(should, "Should activate when idle time exceeded");

        // Generate pong steps and verify cursor moves
        let mut pong = screensaver::PongState::new();
        let r1 = pong.step();
        assert!(r1.x != 0 || r1.y != 0, "First pong step should produce non-zero position");
        assert_eq!(r1.x, 20);
        assert_eq!(r1.y, 25);

        let r2 = pong.step();
        assert_eq!(r2.x, 40);
        assert_eq!(r2.y, 50);
        assert!(r2.x > r1.x && r2.y > r1.y, "Pong should advance monotonically initially");

        // After movement, re-check activation with updated last_move
        // If last_move is very recent (within pong delay of 5000us), should not fire
        let should = screensaver::should_activate(
            &config,
            2_000_000,
            false,
            true,
            99_000,   // last_move very close to current
            100_000,  // current_time — delta = 1000 < 5000 pong delay
        );
        assert!(!should, "Should not activate if move delay not yet elapsed");
    }

    /// HID descriptor parse → classify → interface population.
    /// Tests the full HID setup pipeline from raw descriptor bytes
    /// through classification to interface field population.
    #[test]
    fn test_hid_descriptor_full_pipeline() {
        use crate::domain::hid_classify;

        // 5-button mouse with X, Y, wheel (common real-world descriptor)
        #[rustfmt::skip]
        let desc: &[u8] = &[
            0x05, 0x01,       // Usage Page (Generic Desktop)
            0x09, 0x02,       // Usage (Mouse)
            0xA1, 0x01,       // Collection (Application)
            0x09, 0x01,       //   Usage (Pointer)
            0xA1, 0x00,       //   Collection (Physical)
            // Buttons
            0x05, 0x09,       //     Usage Page (Button)
            0x19, 0x01,       //     Usage Minimum (1)
            0x29, 0x05,       //     Usage Maximum (5)
            0x15, 0x00,       //     Logical Minimum (0)
            0x25, 0x01,       //     Logical Maximum (1)
            0x95, 0x05,       //     Report Count (5)
            0x75, 0x01,       //     Report Size (1)
            0x81, 0x02,       //     Input (Data, Variable, Absolute)
            // Padding
            0x95, 0x01,       //     Report Count (1)
            0x75, 0x03,       //     Report Size (3)
            0x81, 0x01,       //     Input (Constant)
            // X, Y
            0x05, 0x01,       //     Usage Page (Generic Desktop)
            0x09, 0x30,       //     Usage (X)
            0x09, 0x31,       //     Usage (Y)
            0x15, 0x81,       //     Logical Minimum (-127)
            0x25, 0x7F,       //     Logical Maximum (127)
            0x75, 0x08,       //     Report Size (8)
            0x95, 0x02,       //     Report Count (2)
            0x81, 0x06,       //     Input (Data, Variable, Relative)
            // Wheel
            0x09, 0x38,       //     Usage (Wheel)
            0x15, 0x81,       //     Logical Minimum (-127)
            0x25, 0x7F,       //     Logical Maximum (127)
            0x75, 0x08,       //     Report Size (8)
            0x95, 0x01,       //     Report Count (1)
            0x81, 0x06,       //     Input (Data, Variable, Relative)
            0xC0,             //   End Collection
            0xC0,             // End Collection
        ];

        let (_, results) = hid_parser::parse_descriptor(desc);
        assert_eq!(results.len(), 4, "Should parse 4 INPUT items");

        // Classify each parsed value and collect results
        let mut found_buttons = false;
        let mut found_x = false;
        let mut found_y = false;
        let mut found_wheel = false;

        // Also populate an interface to test the full pipeline
        let mut iface: crate::domain::structs::HidInterface = unsafe { core::mem::zeroed() };

        for input in results.iter() {
            for i in 0..input.count {
                let val = &input.vals[i];
                let classified = hid_classify::classify_report_val(val);

                match classified {
                    hid_classify::ExtractedType::MouseButtons => found_buttons = true,
                    hid_classify::ExtractedType::MouseX => found_x = true,
                    hid_classify::ExtractedType::MouseY => found_y = true,
                    hid_classify::ExtractedType::MouseWheel => found_wheel = true,
                    _ => {}
                }

                // Populate interface field
                hid_classify::populate_interface_field(&mut iface, val);
            }
        }

        assert!(found_buttons, "Should classify mouse buttons");
        assert!(found_x, "Should classify mouse X");
        assert!(found_y, "Should classify mouse Y");
        assert!(found_wheel, "Should classify mouse wheel");

        // Verify interface was populated correctly
        assert!(iface.mouse.is_found, "Mouse descriptor should be marked as found");
        // Buttons = 5 data bits + 3 padding bits = 8 total (padding is accumulated)
        assert_eq!({ iface.mouse.buttons.size }, 8, "Buttons should be 8 bits (5 data + 3 padding)");
        assert_eq!({ iface.mouse.move_x.usage }, hid_parser::HID_USAGE_DESKTOP_X);
        assert_eq!({ iface.mouse.move_y.usage }, hid_parser::HID_USAGE_DESKTOP_Y);
        assert_eq!({ iface.mouse.wheel.usage }, hid_parser::HID_USAGE_DESKTOP_WHEEL);
        assert_eq!({ iface.mouse.move_x.size }, 8, "X should be 8 bits");
        assert_eq!({ iface.mouse.move_y.size }, 8, "Y should be 8 bits");

        // Verify report value extraction works with the populated interface
        // Simulate a report: buttons=0x05 (btn1+btn3), X=-10, Y=20, Wheel=5
        let report = [0x05u8, 0xF6, 0x14, 0x05];
        let x = hid_report::get_report_value(&report, { iface.mouse.move_x.offset }, { iface.mouse.move_x.size });
        let y = hid_report::get_report_value(&report, { iface.mouse.move_y.offset }, { iface.mouse.move_y.size });
        assert_eq!(x, -10, "Extracted X should be -10");
        assert_eq!(y, 20, "Extracted Y should be 20");
    }
}
