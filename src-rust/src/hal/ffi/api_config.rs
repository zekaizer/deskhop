// API field map — Rust-native field access, no offset_of needed.
// Replaces C api_config.c field map + hal_shim.c accessors.

use crate::domain::structs::Device;

struct FieldDef {
    idx: u8,
    readonly: bool,
    len: u8,
}

// Read a field from Device into output buffer
unsafe fn read_field(state: &Device, idx: u8, out: *mut u8) {
    macro_rules! w8  { ($v:expr) => { *out = $v }; }
    macro_rules! w16 { ($v:expr) => {{ let b = ($v).to_le_bytes(); *out = b[0]; *out.add(1) = b[1]; }}; }
    macro_rules! w32 { ($v:expr) => {{ let b = ($v).to_le_bytes(); core::ptr::copy_nonoverlapping(b.as_ptr(), out, 4); }}; }
    macro_rules! w64 { ($v:expr, $len:expr) => {{ let b = ($v).to_le_bytes(); core::ptr::copy_nonoverlapping(b.as_ptr(), out, $len); }}; }

    match idx {
        0  => w8!(state.active_output),
        1  => w16!(state.pointer_x),
        2  => w16!(state.pointer_y),
        3  => w16!(state.mouse_buttons),

        10 => w32!(state.config.output[0].number),
        11 => w32!(state.config.output[0].screen_count),
        12 => w32!(state.config.output[0].speed_x as u32),
        13 => w32!(state.config.output[0].speed_y as u32),
        14 => w32!(state.config.output[0].border.top as u32),
        15 => w32!(state.config.output[0].border.bottom as u32),
        16 => w8!(state.config.output[0].os),
        17 => w8!(state.config.output[0].pos),
        18 => w8!(state.config.output[0].mouse_park_pos),
        19 => w8!(state.config.output[0].screensaver.mode),
        20 => w8!(state.config.output[0].screensaver.only_if_inactive),
        21 => w64!(state.config.output[0].screensaver.idle_time_us, 7),
        22 => w64!(state.config.output[0].screensaver.max_time_us, 7),

        40 => w32!(state.config.output[1].number),
        41 => w32!(state.config.output[1].screen_count),
        42 => w32!(state.config.output[1].speed_x as u32),
        43 => w32!(state.config.output[1].speed_y as u32),
        44 => w32!(state.config.output[1].border.top as u32),
        45 => w32!(state.config.output[1].border.bottom as u32),
        46 => w8!(state.config.output[1].os),
        47 => w8!(state.config.output[1].pos),
        48 => w8!(state.config.output[1].mouse_park_pos),
        49 => w8!(state.config.output[1].screensaver.mode),
        50 => w8!(state.config.output[1].screensaver.only_if_inactive),
        51 => w64!(state.config.output[1].screensaver.idle_time_us, 7),
        52 => w64!(state.config.output[1].screensaver.max_time_us, 7),

        70 => w32!(state.config.version),
        71 => w8!(state.config.force_mouse_boot_mode),
        72 => w8!(state.config.force_kbd_boot_protocol),
        73 => w8!(state.config.kbd_led_as_indicator),
        74 => w8!(state.config.hotkey_toggle),
        75 => w8!(state.config.enable_acceleration),
        76 => w8!(state.config.enforce_ports),
        77 => w16!(state.config.jump_threshold),
        78 => w16!(state.running_fw.version),
        79 => w32!(state.running_fw.checksum),
        80 => w8!(state.keyboard_connected as u8),
        81 => w8!(state.switch_lock as u8),
        82 => w8!(state.relative_mouse as u8),
        _ => {}
    }
}

// Write a field from input buffer into Device
unsafe fn write_field(state: &mut Device, idx: u8, data: *const u8) {
    macro_rules! r8  { () => { *data }; }
    macro_rules! r16 { () => {{ u16::from_le_bytes([*data, *data.add(1)]) }}; }
    macro_rules! r32 { () => {{ u32::from_le_bytes([*data, *data.add(1), *data.add(2), *data.add(3)]) }}; }
    macro_rules! r64 { () => {{
        let mut b = [0u8; 8]; core::ptr::copy_nonoverlapping(data, b.as_mut_ptr(), 7); u64::from_le_bytes(b)
    }}; }

    match idx {
        10 => state.config.output[0].number = r32!(),
        11 => state.config.output[0].screen_count = r32!(),
        12 => state.config.output[0].speed_x = r32!() as i32,
        13 => state.config.output[0].speed_y = r32!() as i32,
        14 => state.config.output[0].border.top = r32!() as i32,
        15 => state.config.output[0].border.bottom = r32!() as i32,
        16 => state.config.output[0].os = r8!(),
        17 => state.config.output[0].pos = r8!(),
        18 => state.config.output[0].mouse_park_pos = r8!(),
        19 => state.config.output[0].screensaver.mode = r8!(),
        20 => state.config.output[0].screensaver.only_if_inactive = r8!(),
        21 => state.config.output[0].screensaver.idle_time_us = r64!(),
        22 => state.config.output[0].screensaver.max_time_us = r64!(),

        40 => state.config.output[1].number = r32!(),
        41 => state.config.output[1].screen_count = r32!(),
        42 => state.config.output[1].speed_x = r32!() as i32,
        43 => state.config.output[1].speed_y = r32!() as i32,
        44 => state.config.output[1].border.top = r32!() as i32,
        45 => state.config.output[1].border.bottom = r32!() as i32,
        46 => state.config.output[1].os = r8!(),
        47 => state.config.output[1].pos = r8!(),
        48 => state.config.output[1].mouse_park_pos = r8!(),
        49 => state.config.output[1].screensaver.mode = r8!(),
        50 => state.config.output[1].screensaver.only_if_inactive = r8!(),
        51 => state.config.output[1].screensaver.idle_time_us = r64!(),
        52 => state.config.output[1].screensaver.max_time_us = r64!(),

        70 => state.config.version = r32!(),
        71 => state.config.force_mouse_boot_mode = r8!(),
        72 => state.config.force_kbd_boot_protocol = r8!(),
        73 => state.config.kbd_led_as_indicator = r8!(),
        74 => state.config.hotkey_toggle = r8!(),
        75 => state.config.enable_acceleration = r8!(),
        76 => state.config.enforce_ports = r8!(),
        77 => state.config.jump_threshold = r16!(),
        // 78-82 are readonly
        _ => {}
    }
}

static FIELDS: &[FieldDef] = &[
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
];

fn find_field(api_idx: u8) -> Option<&'static FieldDef> {
    FIELDS.iter().find(|f| f.idx == api_idx)
}

// --- FFI exports ---

#[export_name = "get_field_map_length"]
pub extern "C" fn rust_get_field_map_length() -> u32 {
    FIELDS.len() as u32
}

#[export_name = "hal_get_field_map_length"]
pub extern "C" fn rust_hal_get_field_map_length() -> u32 {
    FIELDS.len() as u32
}

#[export_name = "hal_get_field_map_idx"]
pub extern "C" fn rust_hal_get_field_map_idx(i: u32) -> u8 {
    let idx = if (i as usize) >= FIELDS.len() { FIELDS.len() - 1 } else { i as usize };
    FIELDS[idx].idx
}

#[export_name = "hal_get_field_map"]
pub unsafe extern "C" fn rust_hal_get_field_map(
    api_idx: u8, offset: *mut u32, len: *mut u32, readonly: *mut bool,
) -> i32 {
    match find_field(api_idx) {
        Some(f) => {
            // offset not used by Rust callers — set to 0
            *offset = 0;
            *len = f.len as u32;
            *readonly = f.readonly;
            0
        }
        None => -1,
    }
}

#[export_name = "hal_api_read_field"]
pub unsafe extern "C" fn rust_hal_api_read_field(_offset: u32, _len: u32, _out: *mut u8) {
    // Legacy — not used by Rust api_config path
}

#[export_name = "hal_api_write_field"]
pub unsafe extern "C" fn rust_hal_api_write_field(_offset: u32, _len: u32, _data: *const u8) {
    // Legacy — not used by Rust api_config path
}

// New: Rust-native API message handler (replaces fw_handlers.rs rust_handle_api_msgs)
pub unsafe fn handle_api_msg(ptype: u8, api_idx: u8, data: *const u8, dev: *mut core::ffi::c_void) {
    let field = match find_field(api_idx) {
        Some(f) => f,
        None => return,
    };

    let state = crate::domain::structs::device_from_ptr(dev);

    const SET_VAL: u8 = crate::domain::constants::PacketType::SetVal as u8;
    const GET_VAL: u8 = crate::domain::constants::PacketType::GetVal as u8;

    use crate::hal::traits::*;
    let hal = crate::hal::pico::PicoHal::new(dev);

    if ptype == SET_VAL {
        if field.readonly { return; }
        write_field(state, api_idx, data);
    } else if ptype == GET_VAL {
        let mut response = [0u8; 10];
        response[0] = GET_VAL;
        response[1] = api_idx;
        read_field(state, api_idx, response[2..].as_mut_ptr());
        hal.push_config_packet(response.as_ptr());
    }

    state.config_mode_timer = hal.now_us_64() + 300_000_000;
}

pub unsafe fn handle_api_read_all(dev: *mut core::ffi::c_void) {
    for f in FIELDS.iter() {
        handle_api_msg(
            crate::domain::constants::PacketType::GetVal as u8,
            f.idx, [f.idx, 0, 0, 0, 0, 0, 0, 0].as_ptr(), dev,
        );
    }
}
