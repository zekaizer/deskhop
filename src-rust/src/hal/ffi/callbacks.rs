// USB/UART callback FFI — all functions called from usb.c/uart.c callbacks.

use core::ffi::c_void;
use crate::domain::hid_routing;
use crate::domain::keyboard::HotkeyAction;
use crate::domain::mouse_logic;
use crate::domain::hid_parser::{self, ReportVal};
use crate::domain::constants;
use crate::domain::structs::{self, iface_from_ptr, get_keyboard, KBD_REPORT_LENGTH, HidInterface};
use crate::hal::device;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

// ============================================================
// Keyboard report processing (from kbd_process.rs)
// ============================================================

/// Full keyboard report processing pipeline.
#[export_name = "process_keyboard_report"]
pub unsafe extern "C" fn rust_process_keyboard_report(
    raw_report: *mut u8,
    length: i32,
    itf: u8,
    iface: *mut c_void,
) {
    if raw_report.is_null() || iface.is_null() { crate::traceln!("kbd: null ptr"); return; }
    if length < KBD_REPORT_LENGTH as i32 { crate::traceln!("kbd: short report"); return; }

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();

    // Extract keyboard data (unsafe pointer work stays in ffi)
    let mut new_report = [0u8; 8];
    rust_extract_kbd_data(raw_report, length, itf, iface, new_report.as_mut_ptr());

    // Delegate to service
    use crate::service::frontend::kbd_pipeline::{self, KbdAction};
    match kbd_pipeline::process_report(state, &new_report, itf) {
        KbdAction::HotkeyConsumed { action, acknowledge } => {
            execute_hotkey_action(state, &hal, action);
            if acknowledge { hal.blink(); }
            return;
        }
        KbdAction::HotkeyPassthrough { action, acknowledge } => {
            execute_hotkey_action(state, &hal, action);
            if acknowledge { hal.blink(); }
            // Fall through to route
        }
        KbdAction::Dropped => return,
        KbdAction::Route => {}
    }

    kbd_pipeline::route_combined(state, &hal);
}

/// Rust implementation of process_consumer_report
#[export_name = "process_consumer_report"]
pub unsafe extern "C" fn rust_process_consumer_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    iface: *mut c_void,
) {
    if raw_report.is_null() || iface.is_null() || length < 2 { crate::traceln!("cc: null ptr"); return; }
    let ifc = iface_from_ptr(iface);

    let raw = core::slice::from_raw_parts(raw_report, length as usize);
    let report_id = *raw_report;
    let kbd = get_keyboard(ifc, report_id);
    let new_report = hid_routing::parse_consumer_report(
        raw, ifc.consumer.is_variable, &kbd.cc_array,
    );

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();
    hal.route_consumer(state, &new_report);
}

#[export_name = "process_system_report"]
pub unsafe extern "C" fn rust_process_system_report(
    raw_report: *const u8,
    length: i32,
    _itf: u8,
    _iface: *mut c_void,
) {
    if raw_report.is_null() || length < 2 { crate::traceln!("sys: null ptr"); return; }

    let report = [*raw_report.add(1), 0];

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();
    hal.route_system(state, &report);
}

// ============================================================
// Mouse report processing (from mouse_process.rs)
// ============================================================

/// Full mouse report processing pipeline.
#[export_name = "process_mouse_report"]
pub unsafe extern "C" fn rust_process_mouse_report(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,
) {
    if raw_report.is_null() || iface_ptr.is_null() { crate::traceln!("mouse: null ptr"); return; }

    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;
    let hal = crate::hal::pico::PicoHal::new();
    let iface = iface_from_ptr(iface_ptr);

    // Extract raw HID values (unsafe pointer work stays in ffi)
    let values = extract_mouse_values(raw_report, len, iface, state.hid.mouse_buttons);

    // Delegate to service
    crate::service::frontend::mouse_pipeline::process_report(state, &hal, &values);
}

/// Extract mouse values from raw HID report (boot protocol or descriptor-based).
unsafe fn extract_mouse_values(
    raw_report: *mut u8,
    len: i32,
    iface: &crate::domain::structs::HidInterface,
    fallback_buttons: i16,
) -> mouse_logic::MouseValues {
    const HID_PROTOCOL_BOOT: u8 = 0;
    let mut v = [0i32; 5];

    if iface.protocol == HID_PROTOCOL_BOOT {
        v[0] = *raw_report.add(1) as i8 as i32;
        v[1] = *raw_report.add(2) as i8 as i32;
        v[2] = *raw_report.add(3) as i8 as i32;
        v[4] = *raw_report as i32;
    } else {
        let uses_id = iface.uses_report_id;
        let report_slice = core::slice::from_raw_parts(raw_report, len as usize);

        fn extract_val(report: &[u8], uses_id: bool, rv: &ReportVal) -> Option<i32> {
            let src = if uses_id {
                if report[0] != rv.report_id { return None; }
                &report[1..]
            } else { report };
            Some(crate::domain::hid_report::get_report_value(src, rv.offset, rv.size))
        }

        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.move_x) { v[0] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.move_y) { v[1] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.wheel) { v[2] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.pan) { v[3] = val; }
        if let Some(val) = extract_val(report_slice, uses_id, &iface.mouse.buttons) {
            v[4] = val;
        } else {
            v[4] = fallback_buttons as i32;
        }
    }

    mouse_logic::MouseValues { move_x: v[0], move_y: v[1], wheel: v[2], pan: v[3], buttons: v[4] }
}

// ============================================================
// UART packet dispatch — unified entry point replacing C's process_packet()
// ============================================================

/// Process a UART packet. Called from C's packet_receiver_task after fetch_packet.
/// Replaces the entire process_packet() switch in uart.c.
#[export_name = "process_packet"]
pub unsafe extern "C" fn rust_process_uart_packet(packet_ptr: *const u8) {
    if packet_ptr.is_null() { crate::traceln!("uart: null pkt"); return; }
    let hal = crate::hal::pico::PicoHal::new();
    let mut state = crate::domain::structs::DeviceState::from_globals();
    let state = &mut state;

    let pkt = crate::domain::packet::UartPacket {
        ptype: *packet_ptr,
        data: {
            let mut d = [0u8; 8];
            core::ptr::copy_nonoverlapping(packet_ptr.add(1), d.as_mut_ptr(), 8);
            d
        },
        checksum: *packet_ptr.add(9),
    };

    crate::service::packet_dispatch::dispatch_packet(state, &hal, &pkt);
}

// ============================================================
// Hotkey dispatch (from hotkey_dispatch.rs)
// ============================================================

/// Execute a hotkey action by its enum variant. Called from kbd_process.
pub unsafe fn execute_hotkey_action(
    state: &mut crate::domain::structs::DeviceState<'_>,
    hal: &crate::hal::pico::PicoHal,
    action: HotkeyAction,
) {
    crate::service::hotkey_dispatch::execute_action(state, hal, action);
}

// ============================================================
// HID report + parser FFI (from hid.rs)
// ============================================================

#[export_name = "get_report_value"]
pub unsafe extern "C" fn rust_get_report_value(report: *const u8, len: i32, val: *const u8) -> i32 {
    if report.is_null() || val.is_null() || len <= 0 { crate::traceln!("grv: bad input"); return 0; }
    let slice = core::slice::from_raw_parts(report, len as usize);
    let rv = core::ptr::read_unaligned(val as *const ReportVal);
    crate::domain::hid_report::get_report_value(slice, rv.offset, rv.size)
}

// ============================================================
// HID parser (from hid_parser_ffi.rs)
// ============================================================

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls extract_data for each
/// parsed INPUT item to populate hid_interface_t.
#[export_name = "parse_report_descriptor"]
pub unsafe extern "C" fn rust_parse_report_descriptor(
    iface_ptr: *mut c_void,  // hid_interface_t*
    report: *const u8,
    desc_len: i32,
) {
    if iface_ptr.is_null() || report.is_null() || desc_len <= 0 {
        crate::traceln!("hid: null desc");
        return;
    }

    let desc = core::slice::from_raw_parts(report, desc_len as usize);
    let (_parser, results) = hid_parser::parse_descriptor(desc);

    let iface = iface_from_ptr(iface_ptr);

    for input in results.iter() {
        if input.uses_report_id {
            iface.uses_report_id = true;
        }

        for i in 0..input.count {
            let val = &input.vals[i];
            rust_extract_data(iface_ptr, val as *const _ as *const u8);
        }
    }
}

// ============================================================
// Keyboard data extraction — thin FFI wrapper over domain::kbd_extract
// ============================================================

/// Full extract_kbd_data -- FFI entry point.
/// Converts raw pointers to slices and delegates to domain::kbd_extract.
#[export_name = "extract_kbd_data"]
pub unsafe extern "C" fn rust_extract_kbd_data(
    raw_report: *mut u8,
    len: i32,
    _itf: u8,
    iface_ptr: *mut c_void,
    out_report: *mut u8,
) -> i32 {
    if raw_report.is_null() || iface_ptr.is_null() || out_report.is_null() || len < 8 {
        crate::traceln!("ekd: bad input");
        return 0;
    }

    let report_slice = core::slice::from_raw_parts(raw_report, len as usize);
    let iface = iface_from_ptr(iface_ptr);
    let report_id = *raw_report;
    let kbd = get_keyboard(iface, report_id);

    let (result, rc) = crate::domain::kbd_extract::extract_kbd_data(report_slice, iface, kbd);
    core::ptr::copy_nonoverlapping(result.as_ptr(), out_report, result.len());
    rc
}

// ============================================================
// extract_data (from extract_data.rs)
// ============================================================

/// Rust implementation of extract_data -- thin FFI wrapper.
/// Delegates classification + population to domain::hid_classify::populate_interface_field,
/// then calls HAL to register the report handler if needed.
#[export_name = "extract_data"]
pub unsafe extern "C" fn rust_extract_data(iface_ptr: *mut c_void, val_ptr: *const u8) {
    if iface_ptr.is_null() || val_ptr.is_null() { crate::traceln!("ed: null ptr"); return; }

    let val = core::ptr::read_unaligned(val_ptr as *const ReportVal);
    let rid = val.report_id;
    let iface = iface_from_ptr(iface_ptr);

    use crate::domain::hid_classify::populate_interface_field;
    if let Some(handler_type) = populate_interface_field(iface, &val) {
        device::hal_set_report_handler(iface_ptr, rid, handler_type);
    }
}

// ============================================================
// TinyUSB host callbacks — business logic moved from usb.c
// C side keeps only thin stubs that resolve opaque pointers.
// ============================================================

/// HID device mounted — configure protocol and start receiving reports.
/// Called from C tuh_hid_mount_cb after bounds check and iface resolution.
#[export_name = "rust_on_hid_mount"]
pub unsafe extern "C" fn rust_on_hid_mount(
    dev_addr: u8,
    instance: u8,
    desc_report: *const u8,
    desc_len: u16,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);

    let iface = iface_from_ptr(iface_ptr);
    iface.protocol = device::hal_tuh_hid_get_protocol(dev_addr, instance);

    // Parse HID report descriptor (already Rust — call internal fn directly)
    rust_parse_report_descriptor(iface_ptr, desc_report, desc_len as i32);

    let state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();

    match itf_protocol {
        constants::HID_ITF_PROTOCOL_KEYBOARD => {
            if state.cfg.config.enforce_ports != 0
                && state.cfg.board_role == constants::OUTPUT_B
            {
                return;
            }

            if state.cfg.config.force_kbd_boot_protocol != 0 {
                device::hal_tuh_hid_set_protocol(
                    dev_addr, instance, constants::HID_PROTOCOL_BOOT,
                );
            }

            state.hid.kbd_dev_addr = dev_addr;
            state.hid.kbd_instance = instance;
            state.cfg.keyboard_connected = true;
        }

        constants::HID_ITF_PROTOCOL_MOUSE => {
            if state.cfg.config.enforce_ports != 0
                && state.cfg.board_role == constants::OUTPUT_A
            {
                return;
            }

            if state.cfg.config.force_mouse_boot_mode != 0 {
                device::hal_tuh_hid_set_protocol(
                    dev_addr, instance, constants::HID_PROTOCOL_BOOT,
                );
            } else if iface.protocol == constants::HID_PROTOCOL_BOOT {
                device::hal_tuh_hid_set_protocol(
                    dev_addr, instance, constants::HID_PROTOCOL_REPORT,
                );
            }

            state.cfg.mouse_connected = true;
        }

        _ => {} // HID_ITF_PROTOCOL_NONE
    }

    // Composite devices (e.g. QMK) may expose mouse via keyboard interface
    if iface.mouse.is_found {
        state.cfg.mouse_connected = true;
    }

    hal.blink();
    hal.send_value(constants::ENABLE, constants::PacketType::FlashLed as u8);

    device::hal_tuh_hid_receive_report(dev_addr, instance);
}

/// HID device unmounted — clear connection state and zero interface.
/// Called from C tuh_hid_umount_cb after bounds check and iface resolution.
#[export_name = "rust_on_hid_umount"]
pub unsafe extern "C" fn rust_on_hid_umount(
    dev_addr: u8,
    instance: u8,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let state = structs::DeviceState::from_globals();

    match itf_protocol {
        constants::HID_ITF_PROTOCOL_KEYBOARD => {
            state.cfg.keyboard_connected = false;
        }
        constants::HID_ITF_PROTOCOL_MOUSE => {
            state.cfg.mouse_connected = false;
        }
        _ => {}
    }

    // Zero the interface structure
    let iface = iface_ptr as *mut HidInterface;
    core::ptr::write_bytes(iface, 0, 1);
}

/// HID report received — dispatch to appropriate handler.
/// Called from C tuh_hid_report_received_cb after bounds check and iface resolution.
#[export_name = "rust_on_hid_report_received"]
pub unsafe extern "C" fn rust_on_hid_report_received(
    dev_addr: u8,
    instance: u8,
    report: *const u8,
    len: u16,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let iface = iface_from_ptr(iface_ptr);

    let state = structs::DeviceState::from_globals();
    let device_idx = hid_routing::calculate_device_idx(
        itf_protocol,
        dev_addr,
        instance,
        state.hid.kbd_dev_addr,
        state.hid.kbd_instance,
    );

    if iface.uses_report_id || itf_protocol == constants::HID_ITF_PROTOCOL_NONE {
        let report_id = if iface.uses_report_id { *report } else { 0 };

        if (report_id as usize) < structs::MAX_REPORTS {
            if let Some(handler) = iface.report_handler[report_id as usize] {
                handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
            }
        }
    } else if itf_protocol == constants::HID_ITF_PROTOCOL_KEYBOARD {
        if let Some(handler) = get_process_keyboard_report() {
            handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
        }
    } else if itf_protocol == constants::HID_ITF_PROTOCOL_MOUSE {
        if let Some(handler) = get_process_mouse_report() {
            handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
        }
    }

    device::hal_tuh_hid_receive_report(dev_addr, instance);
}

// Direct references to the Rust-exported report processors for fallback dispatch.
extern "C" {
    #[link_name = "process_keyboard_report"]
    fn process_keyboard_report_c(report: *mut u8, len: i32, itf: u8, iface: *mut HidInterface);
    #[link_name = "process_mouse_report"]
    fn process_mouse_report_c(report: *mut u8, len: i32, itf: u8, iface: *mut HidInterface);
}

fn get_process_keyboard_report() -> Option<unsafe extern "C" fn(*mut u8, i32, u8, *mut HidInterface)> {
    Some(process_keyboard_report_c)
}

fn get_process_mouse_report() -> Option<unsafe extern "C" fn(*mut u8, i32, u8, *mut HidInterface)> {
    Some(process_mouse_report_c)
}

/// HID set_protocol completed — update interface protocol field.
/// Called from C tuh_hid_set_protocol_complete_cb after bounds check and iface resolution.
#[export_name = "rust_on_hid_set_protocol_complete"]
pub unsafe extern "C" fn rust_on_hid_set_protocol_complete(
    iface_ptr: *mut c_void,
    protocol: u8,
) {
    let iface = iface_from_ptr(iface_ptr);
    iface.protocol = protocol;
}

/// TinyUSB device set_report callback — config packets and keyboard LED handling.
/// Called from C tud_hid_set_report_cb (thin stub).
#[export_name = "rust_on_tud_set_report"]
pub unsafe extern "C" fn rust_on_tud_set_report(
    instance: u8,
    report_id: u8,
    report_type: u8,
    buffer: *const u8,
    bufsize: u16,
) {
    use crate::domain::constants::{RAW_PACKET_LENGTH, START_LENGTH};

    const ITF_NUM_HID_VENDOR: u8 = 2;
    const REPORT_ID_VENDOR: u8 = 6;
    const REPORT_ID_KEYBOARD: u8 = 1;
    const HID_REPORT_TYPE_OUTPUT: u8 = 2;

    if buffer.is_null() { return; }

    // Config vendor report
    if instance == ITF_NUM_HID_VENDOR && report_id == REPORT_ID_VENDOR {
        let state = structs::DeviceState::from_globals();
        if !state.cfg.config_mode_active { return; }
        if bufsize as usize != RAW_PACKET_LENGTH { return; }

        let packet_ptr = buffer.add(START_LENGTH);
        // validate_packet and process_packet are Rust #[export_name] functions
        extern "C" {
            fn validate_packet(packet: *const u8) -> bool;
            fn process_packet(packet: *const u8);
        }
        if !validate_packet(packet_ptr) { return; }
        process_packet(packet_ptr);
    }

    // Keyboard LED state change
    if report_id != REPORT_ID_KEYBOARD || bufsize != 1 || report_type != HID_REPORT_TYPE_OUTPUT {
        return;
    }

    let state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();

    let leds = hid_routing::process_led_state(
        *buffer,
        state.cfg.config.kbd_led_as_indicator != 0,
        state.cfg.active_output,
    );

    state.cfg.keyboard_leds[state.cfg.board_role as usize] = leds;

    if state.cfg.keyboard_connected && state.is_active_output() {
        hal.sync_leds();
    }

    hal.send_value(leds, constants::PacketType::KbdSetReport as u8);
}

// ============================================================
// Flash config — replaces C load_config/save_config/reset_config_timer
// ============================================================

const MAGIC_HEADER: u32 = 0xB00B1E5;
const CURRENT_CONFIG_VERSION: u32 = 8;
const CONFIG_MODE_TIMEOUT: u64 = 300_000_000;

extern "C" {
    #[link_name = "default_config"]
    static DEFAULT_CONFIG: structs::Config;
}

#[export_name = "load_config"]
pub unsafe extern "C" fn rust_load_config() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    let size = core::mem::size_of::<structs::Config>();
    device::hal_flash_read_config(&mut cfg.config as *mut structs::Config as *mut u8, size as u32);
    let raw = core::slice::from_raw_parts(&cfg.config as *const structs::Config as *const u8, size - 4);
    let cs = crate::domain::crc::calc_crc32(raw);
    if cfg.config.magic_header != MAGIC_HEADER
        || cfg.config.checksum != cs
        || cfg.config.version != CURRENT_CONFIG_VERSION
    {
        cfg.config = DEFAULT_CONFIG;
    }
}

#[export_name = "save_config"]
pub unsafe extern "C" fn rust_save_config() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    let size = core::mem::size_of::<structs::Config>();
    let raw = core::slice::from_raw_parts(&cfg.config as *const structs::Config as *const u8, size - 4);
    cfg.config.checksum = crate::domain::crc::calc_crc32(raw);
    let mut page = [0u8; structs::FLASH_PAGE_SIZE];
    let config_bytes = core::slice::from_raw_parts(&cfg.config as *const structs::Config as *const u8, size);
    page[..size].copy_from_slice(config_bytes);
    device::hal_flash_write_config(page.as_ptr());
}

#[export_name = "reset_config_timer"]
pub unsafe extern "C" fn rust_reset_config_timer() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    cfg.config_mode_timer = device::hal_time_us_64() + CONFIG_MODE_TIMEOUT;
}

// ============================================================
// Output switching — replaces C set_active_output in hal_shim.c
// ============================================================

extern "C" {
    fn restore_leds();
    fn release_all_keys();
}

#[export_name = "set_active_output"]
pub unsafe extern "C" fn rust_set_active_output(output: u8) {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    cfg.active_output = output;
    restore_leds();
    device::send_value(output, constants::PacketType::OutputSelect as u8);
    release_all_keys();
}

// ============================================================
// Debug state dump — replaces C hal_debug_dump_state in hal_shim.c
// ============================================================

extern "C" {
    fn dh_debug_printf(fmt: *const u8, ...);
}

#[no_mangle]
pub unsafe extern "C" fn hal_debug_dump_state() {
    let cfg = &*core::ptr::addr_of!(structs::global_cfg);
    dh_debug_printf(
        c"tud=%d kbd=%d mse=%d role=%d out=%d c1=%llu\n".as_ptr(),
        cfg.tud_connected as u32,
        cfg.keyboard_connected as u32,
        cfg.mouse_connected as u32,
        cfg.board_role as u32,
        cfg.active_output as u32,
        cfg.core1_last_loop_pass,
    );
}

// ============================================================
// LED control — replaces C led.c functions
// ============================================================

#[export_name = "restore_leds"]
pub unsafe extern "C" fn rust_restore_leds() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    let hid = &*core::ptr::addr_of!(structs::global_hid);
    let is_active = cfg.active_output == cfg.board_role;
    cfg.onboard_led_state = is_active;
    device::hal_gpio_put_led(is_active);

    if cfg.keyboard_connected {
        let leds = cfg.keyboard_leds[cfg.active_output as usize];
        device::hal_tuh_hid_set_report(hid.kbd_dev_addr, hid.kbd_instance, &leds, 1);
    }
}

#[export_name = "set_keyboard_leds"]
pub unsafe extern "C" fn rust_set_keyboard_leds(leds: u8) {
    let cfg = &*core::ptr::addr_of!(structs::global_cfg);
    let hid = &*core::ptr::addr_of!(structs::global_hid);
    if cfg.keyboard_connected {
        device::hal_tuh_hid_set_report(hid.kbd_dev_addr, hid.kbd_instance, &leds, 1);
    }
}

// ============================================================
// TinyUSB descriptor selection — C callbacks delegate here
// ============================================================

// C descriptor arrays (defined in usb_descriptors.c, static const)
extern "C" {
    static desc_device_config: u8;
    static desc_device: u8;
    static desc_hid_report: u8;
    static desc_hid_report_relmouse: u8;
    static desc_hid_report_vendor: u8;
    static desc_configuration_config: u8;
    static desc_configuration: u8;
}

const ITF_NUM_HID_VENDOR: u8 = 2;
const ITF_NUM_HID_C: u8 = 0;
const ITF_NUM_HID_REL_M: u8 = 1;

#[no_mangle]
pub unsafe extern "C" fn rust_get_device_descriptor() -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::global_cfg);
    if cfg.config_mode_active {
        core::ptr::addr_of!(desc_device_config)
    } else {
        core::ptr::addr_of!(desc_device)
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_hid_report_descriptor(instance: u8) -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::global_cfg);
    if cfg.config_mode_active && instance == ITF_NUM_HID_VENDOR {
        return core::ptr::addr_of!(desc_hid_report_vendor);
    }
    match instance {
        ITF_NUM_HID_C => core::ptr::addr_of!(desc_hid_report),
        ITF_NUM_HID_REL_M => core::ptr::addr_of!(desc_hid_report_relmouse),
        _ => core::ptr::addr_of!(desc_hid_report),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_configuration_descriptor() -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::global_cfg);
    if cfg.config_mode_active {
        core::ptr::addr_of!(desc_configuration_config)
    } else {
        core::ptr::addr_of!(desc_configuration)
    }
}

// ============================================================
// TinyUSB device mount/unmount — set tud_connected flag
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_on_tud_mount() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    cfg.tud_connected = true;
}

#[no_mangle]
pub unsafe extern "C" fn rust_on_tud_umount() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::global_cfg);
    cfg.tud_connected = false;
}
