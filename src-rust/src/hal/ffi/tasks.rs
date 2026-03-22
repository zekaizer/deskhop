use core::ffi::c_void;
use crate::hal::device;

const CORE1_HANG_TIMEOUT_US: u64 = 500_000; // 500ms

/// Rust implementation of kick_watchdog_task
#[no_mangle]
pub unsafe extern "C" fn rust_kick_watchdog_task(_dev: *mut c_void) {
    let state = &*crate::app::state::rust_get_app_state();
    if state.reboot_requested { return; }
    let c1 = state.core1_last_loop_pass;
    let now = device::hal_time_us_64();
    if now - c1 < CORE1_HANG_TIMEOUT_US {
        device::hal_watchdog_update();
    }
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

static mut LAST_POINTER_MOVE: u32 = 0;

/// Rust implementation of screensaver_task
#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_task(dev: *mut c_void) {
    let state = &*crate::app::state::rust_get_app_state();
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
    let state = &*crate::app::state::rust_get_app_state();

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
