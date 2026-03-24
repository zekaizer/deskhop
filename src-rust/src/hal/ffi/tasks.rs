use core::ffi::c_void;
use crate::hal::device;

const CORE1_HANG_TIMEOUT_US: u64 = 500_000; // 500ms

/// Rust implementation of kick_watchdog_task
#[no_mangle]
pub unsafe extern "C" fn rust_kick_watchdog_task(dev: *mut c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if state.reboot_requested { return; }
    // Always kick watchdog — core1 timestamp check disabled until core1 is verified
    device::hal_watchdog_update();
}

/// Rust implementation of process_uart_tx_task
#[no_mangle]
pub unsafe extern "C" fn rust_process_uart_tx_task(dev: *mut c_void) {
    if device::hal_dma_channel_is_busy(dev) { return; }
    let mut packet = [0u8; 10]; // uart_packet_t size
    if !device::hal_uart_tx_queue_remove(dev, packet.as_mut_ptr()) { return; }
    // write_raw_packet + DMA send
    let pkt = crate::app::packet::UartPacket {
        ptype: packet[0],
        data: {
            let mut d = [0u8; 8];
            d.copy_from_slice(&packet[1..9]);
            d
        },
        checksum: packet[9],
    };
    let raw = crate::app::packet::write_raw_packet(&pkt);
    device::hal_dma_tx_send(dev, raw.as_ptr(), crate::app::constants::RAW_PACKET_LENGTH as u32);
}

/// Rust implementation of process_kbd_queue_task
#[no_mangle]
pub unsafe extern "C" fn rust_process_kbd_queue_task(dev: *mut c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if !state.tud_connected { return; }
    let mut report = [0u8; 8]; // hid_keyboard_report_t
    if !device::hal_kbd_queue_peek(dev, report.as_mut_ptr()) { return; }
    if device::hal_tud_suspended() { device::hal_tud_remote_wakeup(); }
    if !device::hal_tud_hid_n_ready(crate::app::constants::ITF_NUM_HID) { return; }
    if device::hal_tud_hid_keyboard_report(1, report[0], report[2..].as_ptr()) { // REPORT_ID_KEYBOARD=1
        device::hal_kbd_queue_remove(dev, report.as_mut_ptr());
    }
}

/// Rust implementation of process_mouse_queue_task
#[no_mangle]
pub unsafe extern "C" fn rust_process_mouse_queue_task(dev: *mut c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if !state.tud_connected { return; }
    let mut r = [0u8; 8]; // mouse_report_t
    if !device::hal_mouse_queue_peek(dev, r.as_mut_ptr()) { return; }
    if device::hal_tud_suspended() { device::hal_tud_remote_wakeup(); }
    if !device::hal_tud_hid_n_ready(crate::app::constants::ITF_NUM_HID) { return; }
    // mouse_report_t: buttons(1)+x(i16)+y(i16)+wheel(i8)+pan(i8)+mode(1)
    let mode = r[7];
    let buttons = r[0];
    let x = i16::from_le_bytes([r[1], r[2]]);
    let y = i16::from_le_bytes([r[3], r[4]]);
    let wheel = r[5] as i8;
    let pan = r[6] as i8;
    if device::hal_tud_mouse_report(mode, buttons, x, y, wheel, pan) {
        device::hal_mouse_queue_remove(dev, r.as_mut_ptr());
    }
}

static mut LAST_POINTER_MOVE: u32 = 0;

/// Rust implementation of screensaver_task
#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_task(dev: *mut c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let role = state.board_role as usize;
    if role >= state.config.output.len() { return; }

    let ss = &state.config.output[role].screensaver;
    let inactivity = device::hal_time_us_64() - state.last_activity[role];
    let current_time = device::hal_time_us_32();

    if !crate::app::screensaver::should_activate(
        &crate::app::screensaver::ScreensaverConfig {
            mode: ss.mode,
            only_if_inactive: ss.only_if_inactive != 0,
            idle_time_us: ss.idle_time_us,
            max_time_us: ss.max_time_us,
        },
        inactivity,
        state.is_active_output(),
        device::hal_tud_ready(),
        LAST_POINTER_MOVE,
        current_time,
    ) {
        return;
    }

    // Generate report
    let mut report_bytes = [0u8; 8];
    match ss.mode {
        1 => super::screensaver::rust_screensaver_pong(report_bytes.as_mut_ptr()),  // PONG
        2 => super::screensaver::rust_screensaver_jitter(report_bytes.as_mut_ptr()), // JITTER
        _ => return,
    }

    // Queue mouse report
    device::hal_queue_mouse_report(dev, report_bytes.as_ptr());
    LAST_POINTER_MOVE = device::hal_time_us_32();
}

/// Rust implementation of heartbeat_output_task
#[no_mangle]
pub unsafe extern "C" fn rust_heartbeat_output_task(dev: *mut c_void) {
    let state = crate::app::structs::device_from_ptr(dev);

    if state.fw.upgrade_in_progress { return; }

    if state.config_mode_active {
        if device::hal_time_us_64() > state.config_mode_timer {
            device::hal_reboot();
        }
        device::hal_blink_led(dev);
    }

    // Build heartbeat packet: type=HEARTBEAT, data16[0]=version, data16[2]=active_output
    let version = state.running_fw.version;
    let mut packet = [0u8; 10]; // uart_packet_t: type(1) + data(8) + checksum(1)
    packet[0] = crate::app::constants::PacketType::Heartbeat as u8;
    packet[1] = (version & 0xFF) as u8;
    packet[2] = ((version >> 8) & 0xFF) as u8;
    packet[5] = state.active_output; // data16[2] = bytes 5-6

    device::hal_queue_uart_packet(dev, packet.as_ptr());
}
