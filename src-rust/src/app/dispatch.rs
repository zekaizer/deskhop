use crate::app::constants::PacketType;
use crate::app::crc::calc_checksum;
use crate::app::packet::UartPacket;
use crate::app::constants::PACKET_DATA_LENGTH;

/// Result of packet validation
#[derive(Debug, PartialEq, Eq)]
pub enum PacketError {
    BadChecksum,
    UnknownType,
}

/// Validate a received packet's checksum and type
pub fn validate_received_packet(packet: &UartPacket) -> Result<PacketType, PacketError> {
    let expected = calc_checksum(&packet.data);
    if expected != packet.checksum {
        return Err(PacketError::BadChecksum);
    }

    PacketType::from_u8(packet.ptype).ok_or(PacketError::UnknownType)
}

/// Dispatch action for each packet type.
/// Maps PacketType to a semantic action the caller should execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchAction {
    // Core functions
    KeyboardReport,
    MouseReport,
    OutputSelect,

    // Box control
    MouseZoom,
    KbdSetReport,
    SwitchLock,
    SyncBorders,
    FlashLed,
    GamingMode,
    ConsumerControl,
    SystemControl,
    Screensaver,

    // Config
    WipeConfig,
    SaveConfig,
    Reboot,
    GetVal,
    SetVal,
    GetAllVals,
    ProxyPacket,

    // Firmware
    FirmwareUpgrade,
    RequestByte,
    ResponseByte,
    Heartbeat,
}

/// Map a PacketType to the corresponding dispatch action
pub fn get_dispatch_action(ptype: PacketType) -> DispatchAction {
    match ptype {
        PacketType::KeyboardReport => DispatchAction::KeyboardReport,
        PacketType::MouseReport => DispatchAction::MouseReport,
        PacketType::OutputSelect => DispatchAction::OutputSelect,
        PacketType::FirmwareUpgrade => DispatchAction::FirmwareUpgrade,
        PacketType::MouseZoom => DispatchAction::MouseZoom,
        PacketType::KbdSetReport => DispatchAction::KbdSetReport,
        PacketType::SwitchLock => DispatchAction::SwitchLock,
        PacketType::SyncBorders => DispatchAction::SyncBorders,
        PacketType::FlashLed => DispatchAction::FlashLed,
        PacketType::WipeConfig => DispatchAction::WipeConfig,
        PacketType::Screensaver => DispatchAction::Screensaver,
        PacketType::Heartbeat => DispatchAction::Heartbeat,
        PacketType::GamingMode => DispatchAction::GamingMode,
        PacketType::ConsumerControl => DispatchAction::ConsumerControl,
        PacketType::SystemControl => DispatchAction::SystemControl,
        PacketType::SaveConfig => DispatchAction::SaveConfig,
        PacketType::Reboot => DispatchAction::Reboot,
        PacketType::GetVal => DispatchAction::GetVal,
        PacketType::SetVal => DispatchAction::SetVal,
        PacketType::GetAllVals => DispatchAction::GetAllVals,
        PacketType::ProxyPacket => DispatchAction::ProxyPacket,
        PacketType::RequestByte => DispatchAction::RequestByte,
        PacketType::ResponseByte => DispatchAction::ResponseByte,
    }
}

/// Process a packet: validate checksum, determine type, return action.
pub fn process_packet(packet: &UartPacket) -> Result<DispatchAction, PacketError> {
    let ptype = validate_received_packet(packet)?;
    Ok(get_dispatch_action(ptype))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_packet(ptype: u8) -> UartPacket {
        let data = [0u8; PACKET_DATA_LENGTH];
        UartPacket {
            ptype,
            data,
            checksum: calc_checksum(&data), // valid checksum
        }
    }

    #[test]
    fn test_validate_good_packet() {
        let pkt = make_valid_packet(PacketType::Heartbeat as u8);
        let result = validate_received_packet(&pkt);
        assert_eq!(result, Ok(PacketType::Heartbeat));
    }

    #[test]
    fn test_validate_bad_checksum() {
        let mut pkt = make_valid_packet(PacketType::Heartbeat as u8);
        pkt.checksum = 0xFF; // corrupt
        assert_eq!(validate_received_packet(&pkt), Err(PacketError::BadChecksum));
    }

    #[test]
    fn test_validate_unknown_type() {
        let pkt = UartPacket {
            ptype: 255,
            data: [0; PACKET_DATA_LENGTH],
            checksum: 0,
        };
        assert_eq!(validate_received_packet(&pkt), Err(PacketError::UnknownType));
    }

    #[test]
    fn test_process_packet_keyboard() {
        let pkt = make_valid_packet(PacketType::KeyboardReport as u8);
        assert_eq!(process_packet(&pkt), Ok(DispatchAction::KeyboardReport));
    }

    #[test]
    fn test_process_packet_all_types() {
        // Verify all packet types map to some action
        let types = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 18, 19, 20, 21, 22, 23, 24, 25];
        for &t in &types {
            let pkt = make_valid_packet(t);
            assert!(process_packet(&pkt).is_ok(), "Failed for type {}", t);
        }
    }

    #[test]
    fn test_dispatch_action_mapping() {
        assert_eq!(get_dispatch_action(PacketType::MouseReport), DispatchAction::MouseReport);
        assert_eq!(get_dispatch_action(PacketType::Reboot), DispatchAction::Reboot);
        assert_eq!(get_dispatch_action(PacketType::ProxyPacket), DispatchAction::ProxyPacket);
    }

    #[test]
    fn test_validate_gap_types() {
        // Types 16, 17 don't exist — should be UnknownType
        for t in [0u8, 16, 17, 26, 100] {
            let pkt = UartPacket { ptype: t, data: [0; PACKET_DATA_LENGTH], checksum: 0 };
            assert_eq!(validate_received_packet(&pkt), Err(PacketError::UnknownType));
        }
    }

    #[test]
    fn test_process_packet_with_data() {
        let mut pkt = make_valid_packet(PacketType::OutputSelect as u8);
        pkt.data[0] = 1; // output B
        pkt.checksum = crate::app::crc::calc_checksum(&pkt.data);
        assert_eq!(process_packet(&pkt), Ok(DispatchAction::OutputSelect));
    }

    #[test]
    fn test_dispatch_firmware_types() {
        assert_eq!(get_dispatch_action(PacketType::FirmwareUpgrade), DispatchAction::FirmwareUpgrade);
        assert_eq!(get_dispatch_action(PacketType::RequestByte), DispatchAction::RequestByte);
        assert_eq!(get_dispatch_action(PacketType::ResponseByte), DispatchAction::ResponseByte);
    }
}
