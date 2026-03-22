// Packet constants
pub const START1: u8 = 0xAA;
pub const START2: u8 = 0x55;
pub const START_LENGTH: usize = 2;

pub const TYPE_LENGTH: usize = 1;
pub const PACKET_DATA_LENGTH: usize = 8;
pub const CHECKSUM_LENGTH: usize = 1;
pub const PACKET_LENGTH: usize = TYPE_LENGTH + PACKET_DATA_LENGTH + CHECKSUM_LENGTH;
pub const RAW_PACKET_LENGTH: usize = START_LENGTH + PACKET_LENGTH;

// Output identifiers
pub const OUTPUT_A: u8 = 0;
pub const OUTPUT_B: u8 = 1;

// Mouse modes
pub const ABSOLUTE: u8 = 0;
pub const RELATIVE: u8 = 1;

// Screen coordinates
pub const MAX_SCREEN_COORD: i16 = 32767;
pub const MIN_SCREEN_COORD: i16 = 0;

// Number of screens
pub const NUM_SCREENS: usize = 2;

// HID interface numbers
pub const ITF_NUM_HID: u8 = 0;
pub const ITF_NUM_HID_REL_M: u8 = 1;
pub const ITF_NUM_HID_VENDOR: u8 = 2;

// DMA ring buffer
pub const DMA_RX_BUFFER_SIZE: u32 = 1024;

/// UART packet types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    KeyboardReport = 1,
    MouseReport = 2,
    OutputSelect = 3,
    FirmwareUpgrade = 4,
    MouseZoom = 5,
    KbdSetReport = 6,
    SwitchLock = 7,
    SyncBorders = 8,
    FlashLed = 9,
    WipeConfig = 10,
    Screensaver = 11,
    Heartbeat = 12,
    GamingMode = 13,
    ConsumerControl = 14,
    SystemControl = 15,
    SaveConfig = 18,
    Reboot = 19,
    GetVal = 20,
    SetVal = 21,
    GetAllVals = 22,
    ProxyPacket = 23,
    RequestByte = 24,
    ResponseByte = 25,
}

impl PacketType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::KeyboardReport),
            2 => Some(Self::MouseReport),
            3 => Some(Self::OutputSelect),
            4 => Some(Self::FirmwareUpgrade),
            5 => Some(Self::MouseZoom),
            6 => Some(Self::KbdSetReport),
            7 => Some(Self::SwitchLock),
            8 => Some(Self::SyncBorders),
            9 => Some(Self::FlashLed),
            10 => Some(Self::WipeConfig),
            11 => Some(Self::Screensaver),
            12 => Some(Self::Heartbeat),
            13 => Some(Self::GamingMode),
            14 => Some(Self::ConsumerControl),
            15 => Some(Self::SystemControl),
            18 => Some(Self::SaveConfig),
            19 => Some(Self::Reboot),
            20 => Some(Self::GetVal),
            21 => Some(Self::SetVal),
            22 => Some(Self::GetAllVals),
            23 => Some(Self::ProxyPacket),
            24 => Some(Self::RequestByte),
            25 => Some(Self::ResponseByte),
            _ => None,
        }
    }
}

/// Packet types allowed via configuration endpoint
const ALLOWED_CONFIG_PACKETS: &[PacketType] = &[
    PacketType::FlashLed,
    PacketType::GetVal,
    PacketType::GetAllVals,
    PacketType::SetVal,
    PacketType::WipeConfig,
    PacketType::SaveConfig,
    PacketType::Reboot,
    PacketType::ProxyPacket,
];

/// Validate whether a packet type is allowed via config endpoint.
/// For proxy packets, validates the encapsulated type instead.
pub fn validate_packet_type(packet_type: u8, proxy_inner_type: u8) -> bool {
    let effective_type = if packet_type == PacketType::ProxyPacket as u8 {
        proxy_inner_type
    } else {
        packet_type
    };

    ALLOWED_CONFIG_PACKETS
        .iter()
        .any(|&allowed| allowed as u8 == effective_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_type_roundtrip() {
        assert_eq!(PacketType::from_u8(1), Some(PacketType::KeyboardReport));
        assert_eq!(PacketType::from_u8(25), Some(PacketType::ResponseByte));
        assert_eq!(PacketType::from_u8(0), None);
        assert_eq!(PacketType::from_u8(16), None);
        assert_eq!(PacketType::from_u8(255), None);
    }

    #[test]
    fn test_validate_packet_type() {
        assert!(validate_packet_type(PacketType::FlashLed as u8, 0));
        assert!(validate_packet_type(PacketType::GetVal as u8, 0));
        assert!(validate_packet_type(PacketType::SaveConfig as u8, 0));
        assert!(!validate_packet_type(PacketType::KeyboardReport as u8, 0));
        assert!(!validate_packet_type(PacketType::MouseReport as u8, 0));
    }

    #[test]
    fn test_validate_proxy_packet() {
        // Proxy with allowed inner type
        assert!(validate_packet_type(
            PacketType::ProxyPacket as u8,
            PacketType::GetVal as u8
        ));
        // Proxy with disallowed inner type
        assert!(!validate_packet_type(
            PacketType::ProxyPacket as u8,
            PacketType::KeyboardReport as u8
        ));
    }
}
