use crate::app::constants::*;
use crate::app::crc::calc_checksum;

/// UART packet structure mirroring the C uart_packet_t.
/// Layout: type (1 byte) + data (8 bytes) + checksum (1 byte) = 10 bytes
// WORKAROUND(c-compat): Uses #[repr(C, packed)] and flat u8 arrays to match
// C's uart_packet_t layout with union { data[8]; data16[4]; data32[2]; }.
// Can be replaced with a proper Rust enum-based packet when C interop is removed.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct UartPacket {
    pub ptype: u8,
    pub data: [u8; PACKET_DATA_LENGTH],
    pub checksum: u8,
}

impl UartPacket {
    pub fn new(ptype: u8, data: [u8; PACKET_DATA_LENGTH]) -> Self {
        Self {
            ptype,
            data,
            checksum: 0,
        }
    }

    /// Verify the checksum matches the data
    pub fn verify_checksum(&self) -> bool {
        calc_checksum(&self.data) == self.checksum
    }

    /// Access data as u16 array (little-endian)
    pub fn data16(&self, index: usize) -> u16 {
        let offset = index * 2;
        u16::from_le_bytes([self.data[offset], self.data[offset + 1]])
    }

    /// Access data as u32 array (little-endian)
    pub fn data32(&self, index: usize) -> u32 {
        let offset = index * 4;
        u32::from_le_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }
}

/// Encode a UartPacket into a raw wire-format byte array.
/// Format: [START1, START2, type, data[0..8], checksum]
pub fn write_raw_packet(packet: &UartPacket) -> [u8; RAW_PACKET_LENGTH] {
    let mut raw = [0u8; RAW_PACKET_LENGTH];
    raw[0] = START1;
    raw[1] = START2;
    raw[2] = packet.ptype;
    raw[3..11].copy_from_slice(&packet.data);
    raw[11] = calc_checksum(&packet.data);
    raw
}

/// Parse a raw wire-format byte array into a UartPacket.
/// Expects START1, START2 preamble. Returns None if preamble mismatch.
pub fn parse_raw_packet(raw: &[u8; RAW_PACKET_LENGTH]) -> Option<UartPacket> {
    if raw[0] != START1 || raw[1] != START2 {
        return None;
    }

    let mut data = [0u8; PACKET_DATA_LENGTH];
    data.copy_from_slice(&raw[3..11]);

    Some(UartPacket {
        ptype: raw[2],
        data,
        checksum: raw[11],
    })
}

/// Calculate ring buffer pointer delta
pub fn get_ptr_delta(current: u32, saved: u32, buffer_size: u32) -> u32 {
    let delta = if current >= saved {
        current - saved
    } else {
        buffer_size - saved + current
    };
    // Clamp to 10 bits (buffer_size is 1024)
    delta & 0x3FF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_raw_packet() {
        let pkt = UartPacket::new(PacketType::Heartbeat as u8, [0; PACKET_DATA_LENGTH]);
        let raw = write_raw_packet(&pkt);

        assert_eq!(raw[0], START1);
        assert_eq!(raw[1], START2);
        assert_eq!(raw[2], PacketType::Heartbeat as u8);
        assert_eq!(raw[11], 0); // checksum of all zeros
    }

    #[test]
    fn test_roundtrip() {
        let mut data = [0u8; PACKET_DATA_LENGTH];
        data[0] = 0x42;
        data[7] = 0xFF;

        let pkt = UartPacket::new(PacketType::MouseReport as u8, data);
        let raw = write_raw_packet(&pkt);
        let parsed = parse_raw_packet(&raw).unwrap();

        assert_eq!(parsed.ptype, PacketType::MouseReport as u8);
        assert_eq!(parsed.data[0], 0x42);
        assert_eq!(parsed.data[7], 0xFF);
        assert!(parsed.verify_checksum());
    }

    #[test]
    fn test_parse_bad_preamble() {
        let mut raw = [0u8; RAW_PACKET_LENGTH];
        raw[0] = 0x00; // bad preamble
        assert!(parse_raw_packet(&raw).is_none());
    }

    #[test]
    fn test_verify_checksum() {
        let pkt = UartPacket {
            ptype: 1,
            data: [0xAA, 0x55, 0, 0, 0, 0, 0, 0],
            checksum: 0xFF,
        };
        assert!(pkt.verify_checksum());

        let bad_pkt = UartPacket {
            ptype: 1,
            data: [0xAA, 0x55, 0, 0, 0, 0, 0, 0],
            checksum: 0x00,
        };
        assert!(!bad_pkt.verify_checksum());
    }

    #[test]
    fn test_data16_data32() {
        let pkt = UartPacket::new(0, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(pkt.data16(0), 0x0201);
        assert_eq!(pkt.data32(0), 0x04030201);
    }

    #[test]
    fn test_get_ptr_delta() {
        assert_eq!(get_ptr_delta(100, 50, 1024), 50);
        assert_eq!(get_ptr_delta(50, 100, 1024), 974);
        assert_eq!(get_ptr_delta(0, 0, 1024), 0);
        assert_eq!(get_ptr_delta(1023, 0, 1024), 1023);
    }
}
