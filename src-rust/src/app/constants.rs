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

// Operating system types (from C enum os_type_e)
pub const OS_LINUX: u8 = 1;
pub const OS_MACOS: u8 = 2;
pub const OS_WINDOWS: u8 = 3;
pub const OS_ANDROID: u8 = 4;

// HID keyboard modifiers (from TinyUSB hid.h)
pub const KEYBOARD_MODIFIER_LEFTCTRL: u8 = 0x01;
pub const KEYBOARD_MODIFIER_LEFTSHIFT: u8 = 0x02;
pub const KEYBOARD_MODIFIER_RIGHTCTRL: u8 = 0x10;
pub const KEYBOARD_MODIFIER_RIGHTSHIFT: u8 = 0x20;
pub const KEYBOARD_MODIFIER_RIGHTALT: u8 = 0x40;

// HID key codes (from TinyUSB hid.h — only hotkey-relevant subset)
pub const HID_KEY_A: u8 = 0x04;
pub const HID_KEY_B: u8 = 0x05;
pub const HID_KEY_C: u8 = 0x06;
pub const HID_KEY_D: u8 = 0x07;
pub const HID_KEY_G: u8 = 0x0A;
pub const HID_KEY_J: u8 = 0x0D;
pub const HID_KEY_K: u8 = 0x0E;
pub const HID_KEY_L: u8 = 0x0F;
pub const HID_KEY_O: u8 = 0x12;
pub const HID_KEY_S: u8 = 0x16;
pub const HID_KEY_X: u8 = 0x1B;
pub const HID_KEY_Y: u8 = 0x1C;
pub const HID_KEY_F12: u8 = 0x45;
pub const HID_KEY_CAPS_LOCK: u8 = 0x39;

// Hotkey configuration (from user_config.h)
pub const HOTKEY_MODIFIER: u8 = KEYBOARD_MODIFIER_LEFTCTRL;
pub const HOTKEY_TOGGLE: u8 = HID_KEY_CAPS_LOCK;

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
        assert!(validate_packet_type(PacketType::ProxyPacket as u8, PacketType::GetVal as u8));
        assert!(!validate_packet_type(PacketType::ProxyPacket as u8, PacketType::KeyboardReport as u8));
    }

    #[test]
    fn test_all_allowed_config_packets() {
        // ProxyPacket (23) is special — checks inner type, not itself
        let allowed = [9, 20, 22, 21, 10, 18, 19]; // FlashLed..Reboot
        for &t in &allowed {
            assert!(validate_packet_type(t, 0), "Type {} should be allowed", t);
        }
        // ProxyPacket with allowed inner
        assert!(validate_packet_type(23, 20)); // Proxy(GetVal)
    }

    #[test]
    fn test_all_disallowed_config_packets() {
        let disallowed = [1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13, 14, 15, 24, 25];
        for &t in &disallowed {
            assert!(!validate_packet_type(t, 0), "Type {} should be disallowed", t);
        }
    }

    #[test]
    fn test_packet_type_values() {
        // Verify C enum values match
        assert_eq!(PacketType::KeyboardReport as u8, 1);
        assert_eq!(PacketType::SaveConfig as u8, 18);
        assert_eq!(PacketType::ResponseByte as u8, 25);
    }

    #[test]
    fn test_constants_values() {
        assert_eq!(NUM_SCREENS, 2);
        assert_eq!(RAW_PACKET_LENGTH, 12);
        assert_eq!(PACKET_DATA_LENGTH, 8);
        assert_eq!(MAX_SCREEN_COORD, 32767);
        assert_eq!(MIN_SCREEN_COORD, 0);
    }
}
