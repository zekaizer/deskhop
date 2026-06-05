// Config API service — field map + read/write logic for the configuration protocol.
// Moved from hal/ffi/config.rs to separate business logic from FFI exports.

use crate::domain::structs::DeviceState;
use crate::domain::constants;
use crate::hal::traits::*;

pub struct FieldDef {
    pub idx: u8,
    pub readonly: bool,
    pub len: u8,
}

pub static FIELDS: &[FieldDef] = &[
    FieldDef { idx: 0,  readonly: true,  len: 1 },
    FieldDef { idx: 1,  readonly: true,  len: 2 },
    FieldDef { idx: 2,  readonly: true,  len: 2 },
    FieldDef { idx: 3,  readonly: true,  len: 2 },
    FieldDef { idx: 10, readonly: false, len: 4 }, FieldDef { idx: 11, readonly: false, len: 4 },
    FieldDef { idx: 12, readonly: false, len: 4 }, FieldDef { idx: 13, readonly: false, len: 4 },
    FieldDef { idx: 14, readonly: false, len: 4 }, FieldDef { idx: 15, readonly: false, len: 4 },
    FieldDef { idx: 16, readonly: false, len: 1 }, FieldDef { idx: 17, readonly: false, len: 1 },
    FieldDef { idx: 18, readonly: false, len: 1 }, FieldDef { idx: 19, readonly: false, len: 1 },
    FieldDef { idx: 20, readonly: false, len: 1 }, FieldDef { idx: 21, readonly: false, len: 7 },
    FieldDef { idx: 22, readonly: false, len: 7 },
    FieldDef { idx: 40, readonly: false, len: 4 }, FieldDef { idx: 41, readonly: false, len: 4 },
    FieldDef { idx: 42, readonly: false, len: 4 }, FieldDef { idx: 43, readonly: false, len: 4 },
    FieldDef { idx: 44, readonly: false, len: 4 }, FieldDef { idx: 45, readonly: false, len: 4 },
    FieldDef { idx: 46, readonly: false, len: 1 }, FieldDef { idx: 47, readonly: false, len: 1 },
    FieldDef { idx: 48, readonly: false, len: 1 }, FieldDef { idx: 49, readonly: false, len: 1 },
    FieldDef { idx: 50, readonly: false, len: 1 }, FieldDef { idx: 51, readonly: false, len: 7 },
    FieldDef { idx: 52, readonly: false, len: 7 },
    FieldDef { idx: 70, readonly: false, len: 4 },
    FieldDef { idx: 71, readonly: false, len: 1 }, FieldDef { idx: 72, readonly: false, len: 1 },
    FieldDef { idx: 73, readonly: false, len: 1 }, FieldDef { idx: 74, readonly: false, len: 1 },
    FieldDef { idx: 75, readonly: false, len: 1 }, FieldDef { idx: 76, readonly: false, len: 1 },
    FieldDef { idx: 77, readonly: false, len: 2 },
    FieldDef { idx: 78, readonly: true,  len: 2 }, FieldDef { idx: 79, readonly: true,  len: 4 },
    FieldDef { idx: 80, readonly: true,  len: 1 }, FieldDef { idx: 81, readonly: true,  len: 1 },
    FieldDef { idx: 82, readonly: true,  len: 1 },
    FieldDef { idx: 83, readonly: false, len: 1 }, FieldDef { idx: 84, readonly: false, len: 1 },
    FieldDef { idx: 85, readonly: false, len: 4 },
];

pub fn find_field(api_idx: u8) -> Option<&'static FieldDef> {
    FIELDS.iter().find(|f| f.idx == api_idx)
}

/// Read a field from DeviceState into output slice.
pub fn read_field(state: &DeviceState<'_>, idx: u8, out: &mut [u8]) {
    macro_rules! w8  { ($v:expr) => { out[0] = $v }; }
    macro_rules! w16 { ($v:expr) => {{ let b = ($v).to_le_bytes(); out[..2].copy_from_slice(&b); }}; }
    macro_rules! w32 { ($v:expr) => {{ let b = ($v).to_le_bytes(); out[..4].copy_from_slice(&b); }}; }
    macro_rules! w64 { ($v:expr, $len:expr) => {{ let b = ($v).to_le_bytes(); out[..$len].copy_from_slice(&b[..$len]); }}; }

    match idx {
        0  => w8!(state.cfg.active_output),
        1  => w16!(state.hid.pointer_x),
        2  => w16!(state.hid.pointer_y),
        3  => w16!(state.hid.mouse_buttons),

        10 => w32!(state.cfg.config.output[0].number),
        11 => w32!(state.cfg.config.output[0].screen_count),
        12 => w32!(state.cfg.config.output[0].speed_x as u32),
        13 => w32!(state.cfg.config.output[0].speed_y as u32),
        14 => w32!(state.cfg.config.output[0].border.top as u32),
        15 => w32!(state.cfg.config.output[0].border.bottom as u32),
        16 => w8!(state.cfg.config.output[0].os),
        17 => w8!(state.cfg.config.output[0].pos),
        18 => w8!(state.cfg.config.output[0].mouse_park_pos),
        19 => w8!(state.cfg.config.output[0].screensaver.mode),
        20 => w8!(state.cfg.config.output[0].screensaver.only_if_inactive),
        21 => w64!(state.cfg.config.output[0].screensaver.idle_time_us, 7),
        22 => w64!(state.cfg.config.output[0].screensaver.max_time_us, 7),

        40 => w32!(state.cfg.config.output[1].number),
        41 => w32!(state.cfg.config.output[1].screen_count),
        42 => w32!(state.cfg.config.output[1].speed_x as u32),
        43 => w32!(state.cfg.config.output[1].speed_y as u32),
        44 => w32!(state.cfg.config.output[1].border.top as u32),
        45 => w32!(state.cfg.config.output[1].border.bottom as u32),
        46 => w8!(state.cfg.config.output[1].os),
        47 => w8!(state.cfg.config.output[1].pos),
        48 => w8!(state.cfg.config.output[1].mouse_park_pos),
        49 => w8!(state.cfg.config.output[1].screensaver.mode),
        50 => w8!(state.cfg.config.output[1].screensaver.only_if_inactive),
        51 => w64!(state.cfg.config.output[1].screensaver.idle_time_us, 7),
        52 => w64!(state.cfg.config.output[1].screensaver.max_time_us, 7),

        70 => w32!(state.cfg.config.version),
        71 => w8!(state.cfg.config.force_mouse_boot_mode),
        72 => w8!(state.cfg.config.force_kbd_boot_protocol),
        73 => w8!(state.cfg.config.kbd_led_as_indicator),
        74 => w8!(state.cfg.config.hotkey_toggle),
        75 => w8!(state.cfg.config.enable_acceleration),
        76 => w8!(state.cfg.config.enforce_ports),
        77 => w16!(state.cfg.config.jump_threshold),
        78 => w16!(state.fw._running_fw.version),
        79 => w32!(state.fw._running_fw.checksum),
        80 => w8!(state.cfg.keyboard_connected as u8),
        81 => w8!(state.cfg.switch_lock as u8),
        82 => w8!(state.cfg.relative_mouse as u8),
        83 => w8!(state.cfg.config.passthrough_enabled),
        84 => w8!(state.cfg.config.gaming_mode_default),
        85 => w32!(state.cfg.config.smartshift_double_click_ms),
        _ => {}
    }
}

/// Write a field from input slice into DeviceState.
pub fn write_field(state: &mut DeviceState<'_>, idx: u8, data: &[u8]) {
    macro_rules! r8  { () => { data[0] }; }
    macro_rules! r16 { () => {{ u16::from_le_bytes([data[0], data[1]]) }}; }
    macro_rules! r32 { () => {{ u32::from_le_bytes([data[0], data[1], data[2], data[3]]) }}; }
    macro_rules! r64 { () => {{
        let mut b = [0u8; 8]; b[..7].copy_from_slice(&data[..7]); u64::from_le_bytes(b)
    }}; }

    match idx {
        10 => state.cfg.config.output[0].number = r32!(),
        11 => state.cfg.config.output[0].screen_count = r32!(),
        12 => state.cfg.config.output[0].speed_x = r32!() as i32,
        13 => state.cfg.config.output[0].speed_y = r32!() as i32,
        14 => state.cfg.config.output[0].border.top = r32!() as i32,
        15 => state.cfg.config.output[0].border.bottom = r32!() as i32,
        16 => state.cfg.config.output[0].os = r8!(),
        17 => state.cfg.config.output[0].pos = r8!(),
        18 => state.cfg.config.output[0].mouse_park_pos = r8!(),
        19 => state.cfg.config.output[0].screensaver.mode = r8!(),
        20 => state.cfg.config.output[0].screensaver.only_if_inactive = r8!(),
        21 => state.cfg.config.output[0].screensaver.idle_time_us = r64!(),
        22 => state.cfg.config.output[0].screensaver.max_time_us = r64!(),

        40 => state.cfg.config.output[1].number = r32!(),
        41 => state.cfg.config.output[1].screen_count = r32!(),
        42 => state.cfg.config.output[1].speed_x = r32!() as i32,
        43 => state.cfg.config.output[1].speed_y = r32!() as i32,
        44 => state.cfg.config.output[1].border.top = r32!() as i32,
        45 => state.cfg.config.output[1].border.bottom = r32!() as i32,
        46 => state.cfg.config.output[1].os = r8!(),
        47 => state.cfg.config.output[1].pos = r8!(),
        48 => state.cfg.config.output[1].mouse_park_pos = r8!(),
        49 => state.cfg.config.output[1].screensaver.mode = r8!(),
        50 => state.cfg.config.output[1].screensaver.only_if_inactive = r8!(),
        51 => state.cfg.config.output[1].screensaver.idle_time_us = r64!(),
        52 => state.cfg.config.output[1].screensaver.max_time_us = r64!(),

        70 => state.cfg.config.version = r32!(),
        71 => state.cfg.config.force_mouse_boot_mode = r8!(),
        72 => state.cfg.config.force_kbd_boot_protocol = r8!(),
        73 => state.cfg.config.kbd_led_as_indicator = r8!(),
        74 => state.cfg.config.hotkey_toggle = r8!(),
        75 => state.cfg.config.enable_acceleration = r8!(),
        76 => state.cfg.config.enforce_ports = r8!(),
        77 => state.cfg.config.jump_threshold = r16!(),
        // 78-82 are readonly
        83 => state.cfg.config.passthrough_enabled = r8!(),
        84 => state.cfg.config.gaming_mode_default = r8!(),
        85 => state.cfg.config.smartshift_double_click_ms = r32!(),
        _ => {}
    }
}

/// Handle a single API config message (GET or SET).
pub fn handle_api_msg<H: Timer + PacketQueue>(
    state: &mut DeviceState<'_>,
    hal: &H,
    ptype: u8,
    api_idx: u8,
    data: &[u8],
) {
    let field = match find_field(api_idx) {
        Some(f) => f,
        None => return,
    };

    const SET_VAL: u8 = constants::PacketType::SetVal as u8;
    const GET_VAL: u8 = constants::PacketType::GetVal as u8;

    if ptype == SET_VAL {
        if field.readonly { return; }
        write_field(state, api_idx, data);
    } else if ptype == GET_VAL {
        let mut response = [0u8; 10];
        response[0] = GET_VAL;
        response[1] = api_idx;
        read_field(state, api_idx, &mut response[2..]);
        hal.push_config_packet(&response);
    }

    state.cfg.config_mode_timer = hal.now_us_64() + 300_000_000;
}

/// Read all config fields and send GET responses for each.
pub fn handle_api_read_all<H: Timer + PacketQueue>(state: &mut DeviceState<'_>, hal: &H) {
    for f in FIELDS.iter() {
        handle_api_msg(
            state,
            hal,
            constants::PacketType::GetVal as u8,
            f.idx,
            &[f.idx, 0, 0, 0, 0, 0, 0, 0],
        );
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use super::*;
    use crate::domain::structs::DeviceState;
    use crate::hal::mock::MockHal;

    // Field idx 70 = config.version (u32, writable, len=4)
    // Field idx 16 = config.output[0].os (u8, writable, len=1)
    // Field idx 0  = active_output (u8, readonly, len=1)

    #[test]
    fn test_read_write_roundtrip() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut dev = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let value: u32 = 0xDEAD_BEEF;
        let data = value.to_le_bytes();

        write_field(&mut dev, 70, &data);
        assert_eq!(dev.cfg.config.version, 0xDEAD_BEEF);

        let mut out = [0u8; 8];
        read_field(&dev, 70, &mut out);
        assert_eq!(&out[..4], &data);
    }

    #[test]
    fn test_handle_api_msg_set_val() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut dev = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let hal = MockHal::new();
        let data = 42u32.to_le_bytes();
        let mut buf = [0u8; 8];
        buf[..4].copy_from_slice(&data);

        handle_api_msg(
            &mut dev, &hal,
            constants::PacketType::SetVal as u8,
            70, // config.version
            &buf,
        );

        assert_eq!(dev.cfg.config.version, 42);
    }

    #[test]
    fn test_handle_api_msg_get_val() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut dev = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        dev.cfg.config.version = 99;
        let hal = MockHal::new();

        handle_api_msg(
            &mut dev, &hal,
            constants::PacketType::GetVal as u8,
            70,
            &[0u8; 8],
        );

        let packets = hal.config_packets.borrow();
        assert_eq!(packets.len(), 1);
        // response[0] = GET_VAL, response[1] = 70, response[2..6] = 99 le
        assert_eq!(packets[0][0], constants::PacketType::GetVal as u8);
        assert_eq!(packets[0][1], 70);
        let ver = u32::from_le_bytes([packets[0][2], packets[0][3], packets[0][4], packets[0][5]]);
        assert_eq!(ver, 99);
    }

    #[test]
    fn test_handle_api_msg_readonly_field() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut dev = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        dev.cfg.active_output = 0;
        let hal = MockHal::new();

        // Field idx 0 (active_output) is readonly
        handle_api_msg(
            &mut dev, &hal,
            constants::PacketType::SetVal as u8,
            0,
            &[1, 0, 0, 0, 0, 0, 0, 0],
        );

        // Value must remain unchanged
        assert_eq!(dev.cfg.active_output, 0);
    }

    #[test]
    fn test_handle_api_msg_updates_timer() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut dev = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let hal = MockHal::new();
        hal.set_time(1_000_000);

        handle_api_msg(
            &mut dev, &hal,
            constants::PacketType::GetVal as u8,
            70,
            &[0u8; 8],
        );

        assert_eq!(dev.cfg.config_mode_timer, 1_000_000 + 300_000_000);
    }

    #[test]
    fn test_handle_api_read_all() {
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut dev = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        let hal = MockHal::new();

        handle_api_read_all(&mut dev, &hal);

        let packets = hal.config_packets.borrow();
        assert_eq!(packets.len(), FIELDS.len());

        // Each packet should have GET_VAL type and the corresponding field idx
        for (i, field) in FIELDS.iter().enumerate() {
            assert_eq!(packets[i][0], constants::PacketType::GetVal as u8);
            assert_eq!(packets[i][1], field.idx);
        }
    }
}
