// Packet dispatch service — routes incoming UART packets to handlers.
// Replaces the C process_packet() switch statement in uart.c.

use crate::domain::constants::PacketType;
use crate::domain::dispatch::{self, DispatchAction};
use crate::domain::msg_handlers;
use crate::domain::packet::UartPacket;
use crate::domain::structs::Device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

/// Dispatch a received UART packet to the appropriate handler.
/// This is the Rust equivalent of C's process_packet() in uart.c.
pub fn dispatch_packet(
    state: &mut Device,
    hal: &(impl ReportRouter + OutputControl + ConfigStore + PeerLink
           + Watchdog + Indicator + PacketQueue + Timer),
    packet: &UartPacket,
) {
    let action = match dispatch::process_packet(packet) {
        Ok(a) => a,
        Err(_) => return,
    };

    match action {
        // Direct handlers — already in service layer
        DispatchAction::KeyboardReport => {
            crate::service::msg_bridge::handle_kbd_from_peer(state, hal, &packet.data);
        }
        DispatchAction::MouseReport => {
            crate::service::msg_bridge::handle_mouse_from_peer(state, hal, &packet.data);
        }
        DispatchAction::SyncBorders => {
            crate::service::msg_bridge::handle_sync_borders(state, hal, Some(&packet.data));
        }
        DispatchAction::GetVal | DispatchAction::SetVal => {
            crate::service::config_api::handle_api_msg(
                state, hal, packet.ptype, packet.data[0], &packet.data,
            );
        }
        DispatchAction::GetAllVals => {
            crate::service::config_api::handle_api_read_all(state, hal);
        }
        DispatchAction::RequestByte => {
            let address = u32::from_le_bytes([
                packet.data[0], packet.data[1], packet.data[2], packet.data[3],
            ]);
            if let Some(response) = crate::service::fw_upgrade::send_fw_byte(state, hal, address) {
                hal.send_packet(&response, PacketType::ResponseByte as u8);
            }
        }
        DispatchAction::ResponseByte => {
            let address = u32::from_le_bytes([
                packet.data[0], packet.data[1], packet.data[2], packet.data[3],
            ]);
            let fw_data = [packet.data[4], packet.data[5], packet.data[6], packet.data[7]];
            crate::service::fw_upgrade::receive_fw_byte(state, hal, address, &fw_data);
        }
        DispatchAction::ConsumerControl => {
            hal.push_consumer_control(&packet.data);
        }
        DispatchAction::ProxyPacket => {
            hal.send_packet(&packet.data[1..], packet.data[0]);
        }
        DispatchAction::FirmwareUpgrade => {
            hal.reboot_to_bootloader();
        }

        // Simple messages: domain state mutation + optional HAL side-effect
        _ => {
            dispatch_simple(state, hal, action, packet);
        }
    }
}

/// Handle "simple" packet types that follow the pattern:
/// domain state mutation → optional HAL side-effect.
fn dispatch_simple(
    state: &mut Device,
    hal: &(impl OutputControl + ConfigStore + PeerLink + Watchdog + Indicator + ReportQueue),
    action: DispatchAction,
    packet: &UartPacket,
) {
    let handler_action = msg_handlers::handle_simple_msg(packet.ptype, &packet.data, state);
    let needs_hal = msg_handlers::apply_action(&handler_action, state);

    if !needs_hal { return; }

    match action {
        DispatchAction::OutputSelect => {
            crate::service::msg_bridge::handle_output_select(state, hal, packet.data[0]);
        }
        DispatchAction::KbdSetReport => {
            crate::service::msg_bridge::handle_set_report(state, hal, packet.data[0]);
        }
        DispatchAction::FlashLed => hal.blink(),
        DispatchAction::WipeConfig => {
            crate::service::hotkey_dispatch::wipe_and_notify(hal);
        }
        DispatchAction::SaveConfig => { hal.save(); }
        DispatchAction::Reboot => hal.reboot(),
        DispatchAction::Heartbeat => {} // state already updated by apply_action
        _ => {} // MouseZoom, SwitchLock, GamingMode, Screensaver — state-only
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::crc::calc_checksum;
    use crate::hal::mock::MockHal;

    fn make_packet(ptype: PacketType, data: [u8; 8]) -> UartPacket {
        UartPacket {
            ptype: ptype as u8,
            data,
            checksum: calc_checksum(&data),
        }
    }

    #[test]
    fn dispatch_keyboard_report() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 0;

        let pkt = make_packet(PacketType::KeyboardReport, [0x01, 0, 0x04, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(hal.kbd_reports.borrow().len(), 1);
    }

    #[test]
    fn dispatch_mouse_report() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let mut state = Device::zeroed();
        state.board_role = 0;

        let pkt = make_packet(PacketType::MouseReport, [1, 10, 0, 20, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(hal.mouse_reports.borrow().len(), 1);
    }

    #[test]
    fn dispatch_consumer_control() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::ConsumerControl, [0xE9, 0x00, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(hal.cc_packets.borrow().len(), 1);
    }

    #[test]
    fn dispatch_output_select() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.usb_connected = true;

        let pkt = make_packet(PacketType::OutputSelect, [1, 0, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(state.active_output, 1);
        assert_eq!(hal.leds_synced.get(), 1);
    }

    #[test]
    fn dispatch_flash_led() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::FlashLed, [0; 8]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(hal.blink_count.get(), 1);
    }

    #[test]
    fn dispatch_save_config() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::SaveConfig, [0; 8]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(hal.config_saved.get(), 1);
    }

    #[test]
    fn dispatch_wipe_config() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::WipeConfig, [0; 8]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(hal.config_wiped.get(), 1);
        assert_eq!(hal.config_loaded.get(), 1);
    }

    #[test]
    fn dispatch_mouse_zoom() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::MouseZoom, [1, 0, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert!(state.mouse_zoom);
    }

    #[test]
    fn dispatch_switch_lock() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::SwitchLock, [1, 0, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert!(state.switch_lock);
    }

    #[test]
    fn dispatch_gaming_mode() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = make_packet(PacketType::GamingMode, [1, 0, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert!(state.gaming_mode);
    }

    #[test]
    fn dispatch_bad_checksum_ignored() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        let pkt = UartPacket {
            ptype: PacketType::KeyboardReport as u8,
            data: [0x01, 0, 0x04, 0, 0, 0, 0, 0],
            checksum: 0xFF, // bad
        };
        dispatch_packet(&mut state, &hal, &pkt);

        // Nothing should happen
        assert!(hal.kbd_reports.borrow().is_empty());
    }

    #[test]
    fn dispatch_proxy_packet() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();

        // data[0] = target packet type, data[1..] = payload
        let pkt = make_packet(PacketType::ProxyPacket, [PacketType::FlashLed as u8, 1, 2, 3, 4, 5, 6, 7]);
        dispatch_packet(&mut state, &hal, &pkt);

        let pkts = hal.sent_packets.borrow();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].1, PacketType::FlashLed as u8);
    }

    #[test]
    fn dispatch_sync_borders() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.active_output = 1; // not active → remote path

        let data = [100u8, 0, 0, 0, 200, 0, 0, 0];
        let pkt = make_packet(PacketType::SyncBorders, data);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(state.config.output[1].border.top, 100);
        assert_eq!(state.config.output[1].border.bottom, 200);
    }

    #[test]
    fn dispatch_screensaver_mode() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;

        let pkt = make_packet(PacketType::Screensaver, [2, 0, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(state.config.output[0].screensaver.mode, 2);
    }

    #[test]
    fn dispatch_kbd_set_report() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.board_role = 0;
        state.keyboard_connected = true;
        state.active_output = 1; // not active → sync

        let pkt = make_packet(PacketType::KbdSetReport, [0x07, 0, 0, 0, 0, 0, 0, 0]);
        dispatch_packet(&mut state, &hal, &pkt);

        assert_eq!(state.keyboard_leds[1], 0x07);
        assert_eq!(hal.leds_synced.get(), 1);
    }
}
