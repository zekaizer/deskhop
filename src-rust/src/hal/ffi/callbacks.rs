// USB/UART callback FFI — all functions called from usb.c/uart.c callbacks.
// Thin wrappers: pointer conversion + delegation to domain/service layer.

use core::ffi::c_void;
use crate::domain::hid_routing;
use crate::domain::keyboard::HotkeyAction;
use crate::domain::mouse_logic;
use crate::domain::hid_parser::{self, ReportVal};
use crate::domain::config::ConfigFlash;
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

    // CapsLock remap (hotkey-first): skip the remap when this report is a
    // consumed hotkey, so combos like LCtrl+CapsLock (OutputToggle) still fire on
    // the original report. Otherwise drive the remap state machine, which strips
    // the trigger from the live report; the tap (LANG1 / Shift+Space) and hold
    // (CapsLock) outputs are emitted separately by remap_engine_tick_task. No-op
    // when the engine has no entries (e.g. macOS on both outputs).
    let orig = crate::domain::structs::HidKeyboardReport {
        modifier: new_report[0],
        reserved: new_report[1],
        keycode: [new_report[2], new_report[3], new_report[4],
                  new_report[5], new_report[6], new_report[7]],
    };
    let consumed_hotkey = matches!(
        crate::domain::keyboard::check_all_hotkeys(&orig),
        Some(m) if !m.pass_to_os
    );
    if !consumed_hotkey {
        let engine = super::tasks::get_remap_engine();
        let mut kr = orig;
        crate::domain::key_remap::remap_engine_process(
            engine, &mut kr, state.cfg.active_output, hal.now_us_64());
        new_report = [
            kr.modifier, kr.reserved,
            kr.keycode[0], kr.keycode[1], kr.keycode[2],
            kr.keycode[3], kr.keycode[4], kr.keycode[5],
        ];
    }

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
    if raw_report.is_null() || iface_ptr.is_null() || len <= 0 { crate::traceln!("mouse: null ptr"); return; }

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

/// Parser scratch state (~850B). Static, not stack: the sole runtime caller is
/// the TinyUSB mount path on Core1 (rust_on_hid_mount -> tuh_hid_mount_cb),
/// whose stack is only 2KB. Core1-only access — never touch from Core0.
static mut PARSER_STATE: hid_parser::ParserState = unsafe { core::mem::zeroed() };

/// Replace C's parse_report_descriptor with Rust parser.
/// Parses the HID descriptor, then calls extract_data for each
/// parsed INPUT item to populate hid_interface_t.
/// Streams INPUT items instead of accumulating results: the by-value variant
/// needed a ~14KB stack frame, overrunning Core1's 2KB stack on every mount.
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

    let parser = &mut *core::ptr::addr_of_mut!(PARSER_STATE);
    parser.reset();

    hid_parser::parse_descriptor_with(desc, parser, |input| {
        if input.uses_report_id {
            iface_from_ptr(iface_ptr).uses_report_id = true;
        }

        for i in 0..input.count {
            let val = &input.vals[i];
            rust_extract_data(iface_ptr, val as *const _ as *const u8);
        }
    });
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
// TinyUSB host callbacks — thin FFI wrappers delegating to service::usb
// ============================================================

/// Write a usage page name (or 0xHEX) to a dlog line.
#[cfg(feature = "dh_debug")]
fn dlog_put_page(line: &mut crate::service::dlog::DLog, page: u16) {
    let n = crate::domain::hid_descdump::page_name(page);
    if n.is_empty() {
        line.s(b"0x").hx16(page);
    } else {
        line.s(n);
    }
}

/// Write a usage name within a page (or 0xHEX) to a dlog line.
#[cfg(feature = "dh_debug")]
fn dlog_put_usage(line: &mut crate::service::dlog::DLog, page: u16, usage: u32) {
    let n = crate::domain::hid_descdump::usage_name(page, usage);
    if n.is_empty() {
        line.s(b"0x").hx16(usage as u16);
    } else {
        line.s(n);
    }
}

/// System-level readable dump of a HID report descriptor at mount time. Decodes
/// report-oriented: globals (page, size, count, logical) persist while locals
/// (usages, usage min/max) reset after each Main item, so each Input/Output/
/// Feature collapses to ONE line summarizing its fields. Collection nesting is
/// shown via 2-space indentation. Applies to every HID interface (not just
/// passthrough-captured ones). DH_DEBUG-only: compiled out of release builds
/// (no CDC, no logging) so it costs nothing there.
#[cfg(feature = "dh_debug")]
unsafe fn dump_hid_descriptor(dev_addr: u8, instance: u8, proto: u8, desc: &[u8]) {
    use crate::domain::hid_descdump::{self, Item, Kind};
    use crate::service::dlog;

    let proto_name: &[u8] = match proto {
        1 => b"keyboard",
        2 => b"mouse",
        _ => b"none",
    };
    dlog::i(b"hid")
        .s(b"mount addr=").hx(dev_addr)
        .s(b" inst=").hx(instance)
        .s(b" proto=").s(proto_name)
        .s(b" len=").u(desc.len() as u32)
        .done();

    let mut page: u16 = 0;
    let mut usages = [0u32; 8];
    let mut nusages = 0usize;
    let mut umin: i64 = -1; // -1 == unset
    let mut umax: i64 = -1;
    let mut rsize = 0u32;
    let mut rcount = 0u32;
    let mut lmin = 0u32;
    let mut lmax = 0u32;
    let mut have_log = false;

    hid_descdump::decode(desc, |it: Item| {
        match it.kind {
            // Globals (persist) + locals (reset after each Main item)
            Kind::UsagePage => page = it.value as u16,
            Kind::Usage => {
                if nusages < usages.len() {
                    usages[nusages] = it.value;
                    nusages += 1;
                }
            }
            Kind::UsageMin => umin = it.value as i64,
            Kind::UsageMax => umax = it.value as i64,
            Kind::ReportSize => rsize = it.value,
            Kind::ReportCount => rcount = it.value,
            Kind::LogicalMin => {
                lmin = it.value;
                have_log = true;
            }
            Kind::LogicalMax => {
                lmax = it.value;
                have_log = true;
            }
            Kind::ReportId => {
                let mut line = dlog::i(b"hid");
                for _ in 0..it.depth {
                    line.s(b"  ");
                }
                line.s(b"ReportID ").u(it.value).done();
            }
            Kind::Collection => {
                let mut line = dlog::i(b"hid");
                for _ in 0..it.depth {
                    line.s(b"  ");
                }
                line.s(b"Collection ");
                let cn = hid_descdump::collection_name(it.value);
                if cn.is_empty() {
                    line.s(b"0x").hx(it.value as u8);
                } else {
                    line.s(cn);
                }
                if nusages > 0 {
                    line.s(b" [");
                    dlog_put_page(&mut line, page);
                    line.s(b"/");
                    dlog_put_usage(&mut line, page, usages[0]);
                    line.s(b"]");
                }
                line.done();
                nusages = 0;
                umin = -1;
                umax = -1;
            }
            Kind::EndCollection => {
                let mut line = dlog::i(b"hid");
                for _ in 0..it.depth {
                    line.s(b"  ");
                }
                line.s(b"EndCollection").done();
            }
            Kind::Input | Kind::Output | Kind::Feature => {
                let label: &[u8] = match it.kind {
                    Kind::Input => b"Input ",
                    Kind::Output => b"Output ",
                    _ => b"Feature ",
                };
                let mut line = dlog::i(b"hid");
                for _ in 0..it.depth {
                    line.s(b"  ");
                }
                line.s(label);
                dlog_put_page(&mut line, page);
                line.s(b" ");
                if umin >= 0 && umax >= 0 {
                    line.u(umin as u32).s(b"..").u(umax as u32);
                } else {
                    for (k, &u) in usages[..nusages].iter().enumerate() {
                        if k > 0 {
                            line.s(b",");
                        }
                        dlog_put_usage(&mut line, page, u);
                    }
                }
                line.s(b" cnt=").u(rcount).s(b" sz=").u(rsize);
                if have_log {
                    line.s(b" log=").u(lmin).s(b"..").u(lmax);
                }
                line.s(b" ")
                    .s(if it.value & 0x01 != 0 { b"Const" } else { b"Data" })
                    .s(if it.value & 0x02 != 0 { b",Var" } else { b",Array" })
                    .s(if it.value & 0x04 != 0 { b",Rel" } else { b",Abs" });
                line.done();
                nusages = 0;
                umin = -1;
                umax = -1;
            }
            Kind::Other => {}
        }
    });
}

/// HID device mounted — configure protocol and start receiving reports.
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

    // Parse HID report descriptor
    rust_parse_report_descriptor(iface_ptr, desc_report, desc_len as i32);

    let mut state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();

    // TinyUSB can hand a NULL descriptor (failed descriptor fetch);
    // from_raw_parts requires non-null even for len 0.
    let desc_slice = if desc_report.is_null() {
        &[][..]
    } else {
        core::slice::from_raw_parts(desc_report, desc_len as usize)
    };

    // System-level readable dump of the device's HID report descriptor (all HID
    // interfaces, not just passthrough) so the exact presentation is captured.
    // DH_DEBUG-only — no cost in release.
    #[cfg(feature = "dh_debug")]
    dump_hid_descriptor(dev_addr, instance, itf_protocol, desc_slice);

    // Passthrough: capture descriptor if enabled
    let pt = super::tasks::get_pt_state();
    crate::service::passthrough_service::on_device_mount(
        pt, &state, dev_addr, instance, itf_protocol, desc_slice, &hal,
    );

    let params = crate::service::usb::MountParams { dev_addr, instance, itf_protocol };

    if let Some(proto) = crate::service::usb::on_hid_mount(&mut state, &hal, iface, &params) {
        device::hal_tuh_hid_set_protocol(dev_addr, instance, proto);
    }

    // Mount completion — the open-bracket to the `hid umount` close-bracket, so
    // an enumeration that mounts but never delivers reports is distinguishable
    // from one that never mounted. proto: 0=none 1=keyboard 2=mouse.
    crate::service::dlog::i(b"usb")
        .s(b"hid mount addr=").hx(dev_addr)
        .s(b" inst=").hx(instance)
        .s(b" proto=").hx(itf_protocol)
        .done();

    device::hal_tuh_hid_receive_report(dev_addr, instance);
}

/// HID device unmounted — clear connection state and zero interface.
#[export_name = "rust_on_hid_umount"]
pub unsafe extern "C" fn rust_on_hid_umount(
    dev_addr: u8,
    instance: u8,
    iface_ptr: *mut c_void,
) {
    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let state = structs::DeviceState::from_globals();

    crate::service::dlog::i(b"usb")
        .s(b"hid umount addr=").hx(dev_addr)
        .s(b" inst=").hx(instance)
        .done();

    crate::service::usb::on_hid_umount(state.cfg, itf_protocol);

    // Passthrough: clean up state for this device
    let pt = super::tasks::get_pt_state();
    crate::service::passthrough_service::on_device_unmount(pt, dev_addr);

    // Zero the interface structure
    let iface = iface_ptr as *mut HidInterface;
    core::ptr::write_bytes(iface, 0, 1);
}

/// HID report received — dispatch to appropriate handler.
/// Raw pointer dispatch stays in FFI layer; only device_idx calc is delegated.
#[export_name = "rust_on_hid_report_received"]
pub unsafe extern "C" fn rust_on_hid_report_received(
    dev_addr: u8,
    instance: u8,
    report: *const u8,
    len: u16,
    iface_ptr: *mut c_void,
) {
    // TinyUSB delivers len==0 for failed/ZLP transfers (hidh_xfer_cb passes the
    // result through with no length filter). Every consumer below indexes
    // report[0] (report-ID dispatch, extract paths), so drop empty reports here
    // — an index panic on Core1 spins the diag_led loop until the watchdog
    // reboots the whole board.
    if report.is_null() || len == 0 { return; }

    let itf_protocol = device::hal_tuh_hid_interface_protocol(dev_addr, instance);
    let iface = iface_from_ptr(iface_ptr);
    let mut state = structs::DeviceState::from_globals();

    // Stamp HID activity (mouse/keyboard/HID++) for the board-LED activity
    // flicker. u32 write is atomic across cores; led_blink_tick (Core1) reads it.
    state.cfg.last_hid_activity_us = device::hal_time_us_32();

    // Passthrough: forward non-keyboard reports first
    if itf_protocol != crate::domain::constants::HID_ITF_PROTOCOL_KEYBOARD {
        let pt = super::tasks::get_pt_state();
        let hal = crate::hal::pico::PicoHal::new();
        let report_slice = core::slice::from_raw_parts(report, len as usize);
        if matches!(
            crate::service::passthrough_service::on_report_received(
                pt, &mut state, report_slice, dev_addr, instance, &hal,
            ),
            crate::service::passthrough_service::ReportAction::Handled
        ) {
            return;
        }
    }

    let device_idx = hid_routing::calculate_device_idx(
        itf_protocol, dev_addr, instance,
        state.hid.kbd_dev_addr, state.hid.kbd_instance,
    );

    if iface.uses_report_id || itf_protocol == crate::domain::constants::HID_ITF_PROTOCOL_NONE {
        let report_id = if iface.uses_report_id { *report } else { 0 };

        if (report_id as usize) < structs::MAX_REPORTS {
            if let Some(handler) = iface.report_handler[report_id as usize] {
                handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
            }
        }
    } else if itf_protocol == crate::domain::constants::HID_ITF_PROTOCOL_KEYBOARD {
        if let Some(handler) = get_process_keyboard_report() {
            handler(report as *mut u8, len as i32, device_idx, iface as *mut HidInterface);
        }
    } else if itf_protocol == crate::domain::constants::HID_ITF_PROTOCOL_MOUSE {
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
#[export_name = "rust_on_hid_set_protocol_complete"]
pub unsafe extern "C" fn rust_on_hid_set_protocol_complete(
    iface_ptr: *mut c_void,
    protocol: u8,
) {
    let iface = iface_from_ptr(iface_ptr);
    iface.protocol = protocol;
}

/// TinyUSB device set_report callback — config packets and keyboard LED handling.
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

    // Passthrough: forward output reports from host to receiver
    {
        let pt = super::tasks::get_pt_state();
        if pt.active && instance >= crate::domain::passthrough::ITF_NUM_PT_BASE {
            let buf = core::slice::from_raw_parts(buffer, bufsize as usize);
            crate::service::passthrough_service::on_set_report(
                pt, instance, report_id, report_type, buf,
            );
            return;
        }
    }

    // Config vendor report (pointer dispatch stays in FFI)
    if instance == ITF_NUM_HID_VENDOR && report_id == REPORT_ID_VENDOR {
        let state = structs::DeviceState::from_globals();
        if !state.cfg.config_mode_active { return; }
        if bufsize as usize != RAW_PACKET_LENGTH {
            crate::service::dlog::w(b"cfg").s(b"rx bad len=").u(bufsize as u32).done();
            return;
        }

        extern "C" {
            fn validate_packet(packet: *const u8) -> bool;
            fn process_packet(packet: *const u8);
        }
        let packet_ptr = buffer.add(START_LENGTH);
        if !validate_packet(packet_ptr) {
            crate::service::dlog::w(b"cfg").s(b"rx bad checksum").done();
            return;
        }
        process_packet(packet_ptr);
    }

    // Keyboard LED state change — delegate to service
    if report_id != REPORT_ID_KEYBOARD || bufsize != 1 || report_type != HID_REPORT_TYPE_OUTPUT {
        return;
    }

    let mut state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();
    crate::service::usb::process_led_report(&mut state, &hal, *buffer);
}

// ============================================================
// Setup init — thin FFI wrapper delegating to domain::config
// ============================================================

#[no_mangle]
pub unsafe extern "C" fn rust_init_config(config_mode_active: bool, board_role: u8, timestamp: u64) {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    crate::domain::config::init_config(cfg, config_mode_active, board_role, timestamp);

    // Dump the loaded config at boot so settings (esp. per-output OS, which gates
    // OS-aware remaps) are visible. Runs after load_config + peer_log_init.
    // os legend: 1=Linux 2=macOS 3=Windows 4=Android.
    #[cfg(feature = "dh_debug")]
    {
        let c = &cfg.config;
        crate::service::dlog::i(b"cfg")
            .s(b"ver=").u(c.version)
            .s(b" role=").u(board_role as u32)
            .s(b" cfgmode=").u(config_mode_active as u32)
            .s(b" active=").u(cfg.active_output as u32)
            .s(b" pt=").u(c.passthrough_enabled as u32)
            .s(b" gm=").u(c.gaming_mode_default as u32)
            .s(b" ss_ms=").u(c.smartshift_double_click_ms)
            .done();
        crate::service::dlog::i(b"cfg")
            .s(b"out0 os=").u(c.output[0].os as u32)
            .s(b" out1 os=").u(c.output[1].os as u32)
            .s(b" (1=Lin 2=Mac 3=Win 4=Andr)")
            .done();
    }
}

// ============================================================
// Flash config — thin FFI wrappers delegating to domain::config
// ============================================================

extern "C" {
    #[link_name = "default_config"]
    static DEFAULT_CONFIG: structs::Config;
}

#[export_name = "load_config"]
pub unsafe extern "C" fn rust_load_config() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hal = crate::hal::pico::PicoHal::new();
    let size = core::mem::size_of::<structs::Config>();
    let config_bytes = core::slice::from_raw_parts_mut(
        &mut cfg.config as *mut structs::Config as *mut u8, size,
    );
    if let Some(default) = crate::domain::config::load_config_from_bytes(
        config_bytes, &hal, &DEFAULT_CONFIG,
    ) {
        cfg.config = default;
    }
}

#[export_name = "save_config"]
pub unsafe extern "C" fn rust_save_config() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hal = crate::hal::pico::PicoHal::new();
    let size = core::mem::size_of::<structs::Config>();
    let config_bytes = core::slice::from_raw_parts_mut(
        &mut cfg.config as *mut structs::Config as *mut u8, size,
    );
    cfg.config.checksum = crate::domain::config::compute_config_checksum(config_bytes);
    // Re-read bytes after checksum update
    let config_bytes = core::slice::from_raw_parts(
        &cfg.config as *const structs::Config as *const u8, size,
    );
    let page = crate::domain::config::prepare_save_page(config_bytes);
    hal.flash_write_config(&page);
}

#[export_name = "reset_config_timer"]
pub unsafe extern "C" fn rust_reset_config_timer() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let now = device::hal_time_us_64();
    crate::domain::config::reset_config_timer(cfg, now);
}

// ============================================================
// Output switching — thin FFI wrapper delegating to service::output
// ============================================================

extern "C" {
    fn release_all_keys();
}

/// Send an upstream keyboard-LED report, but only from Core1 (the host-stack
/// core). Called on Core0, it defers by flagging leds_resync_pending; the Core1
/// led_blink_tick drains the flag and re-sends. This keeps tuh_hid_set_report
/// off Core0, which would otherwise race tuh_task and hang Core1.
unsafe fn send_kbd_leds_xcore(da: u8, inst: u8, leds: *const u8, len: u8) {
    if device::hal_is_core1() {
        device::hal_tuh_hid_set_report(da, inst, leds, len);
    } else {
        (*core::ptr::addr_of_mut!(structs::GLOBAL_CFG)).leds_resync_pending = true;
    }
}

#[export_name = "set_active_output"]
pub unsafe extern "C" fn rust_set_active_output(output: u8) {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hid = &*core::ptr::addr_of!(structs::GLOBAL_HID);
    crate::service::output::switch_output(
        cfg,
        hid,
        output,
        |on| device::hal_gpio_put_led(on),
        |da, inst, leds, len| send_kbd_leds_xcore(da, inst, leds, len),
        |val, ptype| device::send_value(val, ptype),
        || release_all_keys(),
    );
}

// ============================================================
// Debug state dump
// ============================================================

/// Latches once the heap-low warning has fired (arena is monotonic, so one shot).
static mut HEAP_WARNED: bool = false;

#[no_mangle]
pub unsafe extern "C" fn hal_debug_dump_state() {
    use crate::service::dlog;

    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    let inuse = device::hal_heap_inuse();
    let arena = device::hal_heap_arena();
    let limit = device::hal_heap_limit();
    dlog::i(b"hb")
        .s(b"tud=").u(cfg.tud_connected as u32)
        .s(b" kbd=").u(cfg.keyboard_connected as u32)
        .s(b" mse=").u(cfg.mouse_connected as u32)
        .s(b" role=").u(cfg.board_role as u32)
        .s(b" out=").u(cfg.active_output as u32)
        .s(b" c0=").u64(cfg.core0_last_loop_pass)
        .s(b" c1=").u64(cfg.core1_last_loop_pass)
        .s(b" heap=").u(inuse)
        .s(b"/").u(arena)
        .s(b"/").u(limit)
        .done();

    // Early-warn on heap exhaustion: the log ring caps malloc at `limit` bytes,
    // so a runaway arena would crash. Fire once when arena crosses 75% of limit.
    let warned = *core::ptr::addr_of!(HEAP_WARNED);
    if !warned && limit > 0 && arena.saturating_mul(100) >= limit.saturating_mul(75) {
        *core::ptr::addr_of_mut!(HEAP_WARNED) = true;
        dlog::w(b"heap")
            .s(b"low arena=").u(arena)
            .s(b" limit=").u(limit)
            .s(b" raise __LOG_HEAP_RESERVE")
            .done();
    }
}

// ============================================================
// LED blink + toggle
// ============================================================

#[export_name = "blink_led"]
pub unsafe extern "C" fn rust_blink_led() {
    let led = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_LED);
    led.blinks_left = 5;
    led.last_led_change = device::hal_time_us_32() as i32;
}

#[export_name = "toggle_led"]
pub unsafe extern "C" fn rust_toggle_led() -> u8 {
    let state = !device::hal_gpio_get_led();
    device::hal_gpio_put_led(state);
    state as u8
}

#[no_mangle]
pub unsafe extern "C" fn hal_toggle_led() -> u8 {
    rust_toggle_led()
}

// ============================================================
// LED control — thin FFI wrappers delegating to service::led
// ============================================================

#[export_name = "restore_leds"]
pub unsafe extern "C" fn rust_restore_leds() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    let hid = &*core::ptr::addr_of!(structs::GLOBAL_HID);
    crate::service::led::sync_indicators(
        cfg,
        hid,
        |on| device::hal_gpio_put_led(on),
        |da, inst, leds, len| send_kbd_leds_xcore(da, inst, leds, len),
    );
}

#[export_name = "set_keyboard_leds"]
pub unsafe extern "C" fn rust_set_keyboard_leds(leds: u8) {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    let hid = &*core::ptr::addr_of!(structs::GLOBAL_HID);
    crate::service::led::send_kbd_led_report(
        cfg,
        hid,
        leds,
        |da, inst, led_val, len| send_kbd_leds_xcore(da, inst, led_val, len),
    );
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
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    if crate::service::usb::is_config_mode(cfg) {
        return core::ptr::addr_of!(desc_device_config);
    }

    // Passthrough: present upstream device identity
    let pt = super::tasks::get_pt_state();
    if pt.active && pt.upstream_vid != 0 {
        // Reuse desc_device as template, patch VID/PID into static buffer
        static mut PT_DEVICE_DESC: [u8; 18] = [0; 18];
        let buf = core::ptr::addr_of_mut!(PT_DEVICE_DESC).cast::<u8>();
        let src = core::ptr::addr_of!(desc_device).cast::<u8>();
        core::ptr::copy_nonoverlapping(src, buf, 18);
        *buf.add(8) = (pt.upstream_vid & 0xFF) as u8;
        *buf.add(9) = (pt.upstream_vid >> 8) as u8;
        *buf.add(10) = (pt.upstream_pid & 0xFF) as u8;
        *buf.add(11) = (pt.upstream_pid >> 8) as u8;
        return buf;
    }

    core::ptr::addr_of!(desc_device)
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_hid_report_descriptor(instance: u8) -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    if crate::service::usb::is_config_mode(cfg) && instance == ITF_NUM_HID_VENDOR {
        return core::ptr::addr_of!(desc_hid_report_vendor);
    }

    // Passthrough: return captured descriptor for passthrough instances
    let pt = super::tasks::get_pt_state();
    if pt.active && instance >= crate::domain::passthrough::ITF_NUM_PT_BASE {
        if let Some((desc, _len)) = crate::domain::passthrough::get_report_desc(pt, instance) {
            return desc.as_ptr();
        }
    }

    match instance {
        ITF_NUM_HID_C => core::ptr::addr_of!(desc_hid_report),
        ITF_NUM_HID_REL_M => core::ptr::addr_of!(desc_hid_report_relmouse),
        _ => core::ptr::addr_of!(desc_hid_report),
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_get_configuration_descriptor() -> *const u8 {
    let cfg = &*core::ptr::addr_of!(structs::GLOBAL_CFG);
    if crate::service::usb::is_config_mode(cfg) {
        return core::ptr::addr_of!(desc_configuration_config);
    }

    // Passthrough: return dynamically built descriptor
    let pt = super::tasks::get_pt_state();
    if pt.active {
        let (ptr, len) = super::pt_config_desc_ptr();
        if len > 0 {
            return ptr;
        }
    }

    core::ptr::addr_of!(desc_configuration)
}

// ============================================================
// USB string descriptors — ASCII→UTF-16 conversion (was the C
// tud_descriptor_string_cb in usb_descriptors.c). Device/config/
// HID-report descriptors already live here; this finishes the set.
// ============================================================

const TUSB_DESC_STRING_TYPE: u16 = 0x03;
const STRID_SERIAL: u8 = 3;
/// PICO_UNIQUE_BOARD_ID_SIZE_BYTES (8) * 2 hex chars + NUL.
const BOARD_ID_STR_LEN: usize = 8 * 2 + 1;

/// Static string table (index 0 is the language id, handled inline; index 3 is
/// the serial, resolved from the chip unique id). Mirrors the former C
/// `string_desc_arr[]`.
fn string_desc_for(index: u8) -> Option<&'static [u8]> {
    Some(match index {
        1 => b"Hrvoje Cavrak",  // Manufacturer
        2 => b"DeskHop Switch", // Product
        4 => b"DeskHop Helper", // Mouse Helper Interface
        5 => b"DeskHop Config", // Vendor Interface
        6 => b"DeskHop Disk",   // Disk Interface
        #[cfg(feature = "dh_debug")]
        7 => b"DeskHop Debug", // Debug Interface
        _ => return None,
    })
}

/// GET STRING DESCRIPTOR. Builds the UTF-16 string descriptor into a static
/// buffer (which must outlive the call — TinyUSB reads it after we return) and
/// returns it, or null to STALL an unknown index.
#[no_mangle]
pub unsafe extern "C" fn rust_get_string_descriptor(index: u8, _langid: u16) -> *const u16 {
    static mut DESC_STR: [u16; 32] = [0; 32];
    let out = core::ptr::addr_of_mut!(DESC_STR).cast::<u16>();

    let chr_count: usize = if index == 0 {
        // Supported language: English (0x0409), little-endian in the word.
        *out.add(1) = 0x0409;
        1
    } else {
        let mut serial = [0u8; BOARD_ID_STR_LEN];
        let s: &[u8] = if index == STRID_SERIAL {
            device::hal_get_board_id_str(serial.as_mut_ptr(), serial.len() as u32);
            let n = serial.iter().position(|&b| b == 0).unwrap_or(serial.len());
            &serial[..n]
        } else {
            match string_desc_for(index) {
                Some(s) => s,
                None => return core::ptr::null(),
            }
        };
        // Cap at the 31-char descriptor limit, then widen ASCII to UTF-16.
        let count = s.len().min(31);
        for (i, &b) in s[..count].iter().enumerate() {
            *out.add(1 + i) = b as u16;
        }
        count
    };

    // First word: low byte = total length (incl. 2-byte header), high byte = type.
    *out = (TUSB_DESC_STRING_TYPE << 8) | (2 * chr_count as u16 + 2);
    out
}

// ============================================================
// TinyUSB device mount/unmount — set tud_connected flag
// ============================================================

/// Dump the composite configuration descriptor WE present to the PC (the tud
/// side: default DeskHop or the passthrough-rebuilt composite) — interfaces and
/// endpoints. We own these bytes (no control transfer), so it's synchronous and
/// safe. DH_DEBUG-only. Lets us verify the re-presented composite at each mount.
#[cfg(feature = "dh_debug")]
unsafe fn dump_tud_composite() {
    use crate::service::dlog;
    let p = rust_get_configuration_descriptor();
    if p.is_null() {
        return;
    }
    let total = u16::from_le_bytes([*p.add(2), *p.add(3)]) as usize;
    if total < 9 {
        return;
    }
    let desc = core::slice::from_raw_parts(p, total);
    dlog::i(b"usb")
        .s(b"tud cfg ifaces=").u(desc[4] as u32)
        .s(b" mA=").u((desc[8] as u32) * 2)
        .done();
    let mut i = 0usize;
    while i + 2 <= total {
        let blen = desc[i] as usize;
        if blen < 2 {
            break;
        }
        match desc[i + 1] {
            0x04 if i + 9 <= total => {
                dlog::i(b"usb")
                    .s(b"tud  if #").u(desc[i + 2] as u32)
                    .s(b" class=").hx(desc[i + 5])
                    .s(b" sub=").hx(desc[i + 6])
                    .s(b" proto=").hx(desc[i + 7])
                    .s(b" eps=").u(desc[i + 4] as u32)
                    .done();
            }
            0x05 if i + 7 <= total => {
                let addr = desc[i + 2];
                let dir: &[u8] = if addr & 0x80 != 0 { b"IN" } else { b"OUT" };
                let ty: &[u8] = match desc[i + 3] & 0x03 {
                    0 => b"ctrl",
                    1 => b"iso",
                    2 => b"bulk",
                    _ => b"intr",
                };
                let mps = u16::from_le_bytes([desc[i + 4], desc[i + 5]]);
                dlog::i(b"usb")
                    .s(b"tud   ep ").hx(addr).s(b" ").s(dir).s(b" ").s(ty)
                    .s(b" mps=").u(mps as u32)
                    .s(b" iv=").u(desc[i + 6] as u32)
                    .done();
            }
            _ => {}
        }
        i += blen;
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_on_tud_mount() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    cfg.tud_connected = true;
    crate::service::dlog::i(b"usb").s(b"tud mount").done();
    #[cfg(feature = "dh_debug")]
    dump_tud_composite();
}

#[no_mangle]
pub unsafe extern "C" fn rust_on_tud_umount() {
    let cfg = &mut *core::ptr::addr_of_mut!(structs::GLOBAL_CFG);
    cfg.tud_connected = false;
    crate::service::dlog::i(b"usb").s(b"tud umount").done();
}

/// CDC control-line state change (SET_CONTROL_LINE_STATE). Logs DTR/RTS so we
/// can see empirically whether a host terminal toggles DTR on open — the signal
/// the scrollback replay latches onto. RTS is logged too (it does NOT gate our
/// connection check, which is DTR-only). Diagnostic; dh_debug only.
#[no_mangle]
pub unsafe extern "C" fn rust_on_cdc_line_state(dtr: bool, rts: bool) {
    crate::service::dlog::i(b"cdc")
        .s(b"line dtr=").u(dtr as u32)
        .s(b" rts=").u(rts as u32)
        .done();
}

/// Parse a single hex token from `s` (skips leading spaces). Returns the value,
/// or None if no hex digit is present.
fn parse_hex(s: &[u8]) -> Option<u32> {
    let mut v = 0u32;
    let mut any = false;
    for &b in s {
        let d = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            b' ' | b'\t' if !any => continue,
            _ => break,
        };
        v = (v << 4) | d as u32;
        any = true;
    }
    if any { Some(v) } else { None }
}

/// Debug CDC command dispatcher (line-based, DH_DEBUG). Commands:
///   logdump            — replay the log scrollback
///   ptr                — toggle pointer-stream logging
///   pushfw             — force the peer to pull THIS board's running image
///                        (faked heartbeat ver=0xFFFF), bypassing the version/crc
///                        direction check — for dev when the peer is at the same
///                        or a higher version than a freshly-flashed build
///   cc<hex>            — send a Consumer Control tap (e.g. cc1a3) to the active output
///   kb<modkey-hex>     — send a keyboard tap; high byte = modifier, low = keycode
///                        (e.g. kb042b = LeftAlt+Tab) to the active output
/// The cc/kb commands let us hunt the right Android key without reflashing.
///
/// Feed raw CDC bytes here; this owns the line buffer and overflow policy (was
/// the static accumulator in tud_cdc_rx_cb) and dispatches each completed
/// (CR/LF-terminated) command to dispatch_dbg_line.
///
/// # Safety
/// `buf` must point to `len` readable bytes. Call from the USB task (Core0)
/// only; commands route via the active-output queues.
#[no_mangle]
pub unsafe extern "C" fn rust_cdc_feed(buf: *const u8, len: u32) {
    if buf.is_null() {
        return;
    }
    let data = core::slice::from_raw_parts(buf, len as usize);

    // Command line buffer (mirrors the former C `static char line[24]`).
    const LINE_MAX: usize = 24;
    static mut LINE: [u8; LINE_MAX] = [0; LINE_MAX];
    static mut LLEN: usize = 0;
    let line = core::ptr::addr_of_mut!(LINE).cast::<u8>();
    let llen = core::ptr::addr_of_mut!(LLEN);

    for &c in data {
        if c == b'\r' || c == b'\n' {
            if *llen > 0 {
                dispatch_dbg_line(core::slice::from_raw_parts(line, *llen));
                *llen = 0;
            }
        } else if *llen < LINE_MAX {
            *line.add(*llen) = c;
            *llen += 1;
        } else {
            *llen = 0; // overflow — drop the line
        }
    }
}

/// Dispatch one complete debug command line.
unsafe fn dispatch_dbg_line(line: &[u8]) {
    if line.is_empty() {
        return;
    }

    if line.starts_with(b"logdump") {
        crate::service::peer_log::rewind_read();
        return;
    }
    if line.starts_with(b"ptr") {
        crate::service::passthrough_service::rust_dbg_toggle_ptr_log();
        return;
    }
    if line.starts_with(b"pushfw") {
        dbg_force_push();
        return;
    }
    if let Some(rest) = line.strip_prefix(b"cc") {
        match parse_hex(rest) {
            Some(usage) => dbg_send_consumer(usage as u16),
            None => crate::service::dlog::w(b"dbg").s(b"cc: bad hex").done(),
        }
        return;
    }
    if let Some(rest) = line.strip_prefix(b"kb") {
        match parse_hex(rest) {
            Some(v) => dbg_send_kbd((v >> 8) as u8, (v & 0xFF) as u8),
            None => crate::service::dlog::w(b"dbg").s(b"kb: bad hex").done(),
        }
        return;
    }
    // A full line that matched no command — echo it so a typo gets feedback
    // instead of silence. (Whole-line dispatch; not per keystroke.)
    crate::service::dlog::w(b"dbg").s(b"unknown cmd").done();
}

/// Force the peer to pull THIS board's running image, regardless of version/crc
/// direction. Sends one heartbeat reporting a sentinel version of 0xFFFF (u16 max,
/// always > any real version): the peer's should_start_fw_upgrade sees "peer is
/// newer" and begins an auto-sync, pulling our REAL image (send_fw_byte serves the
/// real flash, not the fake version) into STAGING, then verifies + promotes. After
/// reboot the peer runs our real image and reports our real version. The fake
/// version exists only in this one heartbeat packet — it is never written anywhere.
unsafe fn dbg_force_push() {
    let state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();
    let crc16 = state.fw._running_fw.checksum as u16;
    // Sentinel version 0xFFFF (always > any real version), real crc16/output.
    // Same payload builder as heartbeat_tick so the layout can't drift.
    let data = crate::domain::packet::build_heartbeat_payload(0xFFFF, crc16, state.cfg.active_output);
    hal.send_packet(&data, crate::domain::constants::PacketType::Heartbeat as u8);
    crate::service::dlog::i(b"dbg").s(b"pushfw: faked hb ver=0xFFFF -> peer pulls our image").done();
}

unsafe fn dbg_send_consumer(usage: u16) {
    use crate::service::router::ReportRouter;
    let mut state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();
    hal.route_consumer(&mut state, &crate::domain::hidpp_keymap::consumer_report(usage));
    hal.route_consumer(&mut state, &crate::domain::hidpp_keymap::consumer_report(0));
    crate::service::dlog::i(b"dbg").s(b"cc=0x").hx16(usage).done();
}

unsafe fn dbg_send_kbd(modifier: u8, key: u8) {
    use crate::service::router::ReportRouter;
    let mut state = structs::DeviceState::from_globals();
    let hal = crate::hal::pico::PicoHal::new();
    let down = [modifier, 0, key, 0, 0, 0, 0, 0];
    let up = [0u8; 8];
    hal.route_kbd(&mut state, &down);
    hal.route_kbd(&mut state, &up);
    crate::service::dlog::i(b"dbg").s(b"kb mod=0x").hx(modifier).s(b" key=0x").hx(key).done();
}

// ============================================================
// LED diagnostics — blocking pattern playback (boot/halt)
// ============================================================

unsafe fn busy_wait_ms(ms: u16) {
    let start = device::hal_time_us_32();
    let duration = (ms as u32) * 1000;
    while device::hal_time_us_32().wrapping_sub(start) < duration {
        core::hint::spin_loop();
    }
}

/// Diagnostic LED entry point (called from C boot stages + the panic/fault
/// handlers). Boot/runtime stages drive the non-blocking timer-backed boot LED
/// (domain/boot_led.rs) so setup isn't stalled; halt patterns keep their
/// blocking playback since the system is stopped anyway.
#[no_mangle]
pub unsafe extern "C" fn diag_led(event: u8) {
    use crate::domain::led_pattern::DiagEvent;

    if let Some(ev) = DiagEvent::from_u8(event) {
        // Log each boot stage with its name so the timeline is readable and the
        // last logged stage shows how far boot got. (ev>=5 lands after
        // peer_log_init, so earlier stages aren't captured.)
        #[cfg(feature = "dh_debug")]
        crate::service::dlog::i(b"boot")
            .s(b"stage=").u(event as u32).s(b" ").s(ev.name())
            .done();

        // Halt patterns block forever — the system is dead, so blocking the
        // (already-stopped) main thread is correct.
        if matches!(ev, DiagEvent::Panic | DiagEvent::HardFault) {
            play_pattern(&ev.pattern());
            return;
        }

        // Boot stages: hand the blink count to the non-blocking boot-LED timer
        // and ensure it's running. Count == stage number; on a boot hang the
        // timer keeps blinking the last stage. Works in release too.
        crate::domain::boot_led::set_stage(event);
        device::hal_boot_led_start();
    }
}

/// Boot-LED timer tick — advances the boot blink state machine and drives the
/// LED. Called from the Core0 hardware timer set up in hal_shim.c.
#[no_mangle]
pub unsafe extern "C" fn rust_boot_led_tick() {
    device::hal_gpio_put_led(crate::domain::boot_led::tick());
}

unsafe fn play_pattern(pat: &crate::domain::led_pattern::LedPattern) {
    loop {
        for _ in 0..pat.blinks {
            device::hal_gpio_put_led(true);
            busy_wait_ms(pat.on_ms);
            device::hal_gpio_put_led(false);
            busy_wait_ms(pat.off_ms);
        }
        busy_wait_ms(pat.pause_ms);
        if !pat.repeat { break; }
    }
}
