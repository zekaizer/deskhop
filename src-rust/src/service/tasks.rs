// Task logic — extracted from hal/ffi/tasks.rs for testability.
// All functions are generic over HAL traits, enabling MockHal in tests.

use crate::domain::constants::{PacketType, PACKET_DATA_LENGTH, RAW_PACKET_LENGTH};
use crate::domain::packet;
use crate::domain::screensaver::{self, ScreensaverConfig};
use crate::domain::structs::DeviceState;
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

const CORE1_HANG_TIMEOUT_US: u64 = 500_000;

/// Latch so a detected core1 stall logs once, not on every health tick while it
/// stays down. Reset when core1 recovers (so a later stall logs again).
static mut CORE1_STALL_LOGGED: bool = false;

/// Check core1 liveness and refresh system health. Returns true if healthy.
///
/// SAFETY(dual-core): `core1_last_loop_pass` is a u64 written by Core1 and read
/// here on Core0. On Cortex-M0+ u64 reads are not atomic — a torn read could
/// yield a garbage timestamp. Worst case: one false hang detection (no kick) or
/// one spurious kick. Both are tolerable and self-correct on the next iteration.
pub fn check_system_health(
    state: &DeviceState<'_>,
    hal: &(impl Timer + Watchdog),
) -> bool {
    if state.fw.reboot_requested {
        return false;
    }
    let now = hal.now_us_64();
    let last = state.cfg.core1_last_loop_pass;
    let gap = now.wrapping_sub(last);
    if gap < CORE1_HANG_TIMEOUT_US {
        hal.kick();
        // Core1 alive — clear the latch so a future stall logs again.
        // SAFETY: single-writer (Core0 health task); plain store.
        unsafe { core::ptr::write(core::ptr::addr_of_mut!(CORE1_STALL_LOGGED), false); }
        return true;
    }
    // gap >= timeout: a real stall OR a torn 64-bit read of `last` (see the
    // dual-core SAFETY note — a torn read can yield last > now, wrapping `gap`
    // to a huge value). Only a real stall has `last <= now`; gating the log on
    // that keeps the torn-read artifact (which fires transiently at boot) off
    // the error channel. The kick decision above keeps the documented wrapping
    // behaviour unchanged. In release the unkicked watchdog resets the board; in
    // debug the watchdog is off, so without this a dead Core1 leaves no trace.
    if last <= now && !unsafe { core::ptr::read(core::ptr::addr_of!(CORE1_STALL_LOGGED)) } {
        unsafe { core::ptr::write(core::ptr::addr_of_mut!(CORE1_STALL_LOGGED), true); }
        crate::service::dlog::e(b"hb")
            .s(b"core1 STALL ms=").u((gap / 1000) as u32)
            .done();
    }
    false
}

/// Send one pending outbound packet if the transfer channel is idle.
pub fn flush_outbox(hal: &(impl Transfer + PeerLink)) {
    if hal.is_busy() { return; }
    let mut pkt_bytes = [0u8; 10];
    if !hal.dequeue(&mut pkt_bytes) { return; }
    let pkt = packet::UartPacket {
        ptype: pkt_bytes[0],
        data: {
            let mut d = [0u8; 8];
            d.copy_from_slice(&pkt_bytes[1..9]);
            d
        },
        checksum: pkt_bytes[9],
    };
    let raw = packet::write_raw_packet(&pkt);
    hal.transmit(&raw);
}

/// Max report data bytes per queued HID packet (mirrors C HID_REPORT_DATA_MAX in packet.h).
const HID_REPORT_DATA_MAX: usize = 32;
/// Size of hid_generic_pkt_t: instance(1) + report_id(1) + type(1) + len(1) + data(HID_REPORT_DATA_MAX) = 36
const HID_GENERIC_PKT_SIZE: usize = 4 + HID_REPORT_DATA_MAX;

/// Send one pending HID report from the output queue via TinyUSB.
/// Peek → check if TinyUSB endpoint is ready → send → remove on success.
/// Returns true if a report was actually sent (marks active-output activity).
pub fn process_hid_queue(
    hal: &(impl HidQueue + UsbDevice),
) -> bool {
    let mut buf = [0u8; HID_GENERIC_PKT_SIZE];
    if !hal.peek_hid_report(&mut buf) { return false; }

    let instance = buf[0];
    let report_id = buf[1];
    // buf[2] = type (unused in send path)
    let len = buf[3] as usize;
    let data = &buf[4..4 + len.min(HID_REPORT_DATA_MAX)];

    if !hal.hid_ready(instance) { return false; }

    if hal.send_hid_report(instance, report_id, data) {
        hal.pop_hid_report(&mut buf);
        return true;
    }
    false
}


const DMA_RX_BUFFER_SIZE: u32 = 1024;

/// Poll the DMA ring buffer for incoming UART packets.
/// Scans for START1+START2 preamble, fetches one packet per tick, dispatches.
///
/// dma_ptr and in_packet live in C's global_hw; Rust accesses them through
/// the DmaRx trait so the loop logic stays here. Single-packet-per-tick
/// matches the original C behaviour and keeps Core1 responsive.
pub fn packet_receive_tick(
    state: &mut DeviceState<'_>,
    hal: &(impl DmaRx + ReportRouter + OutputControl + ConfigStore
           + Watchdog + Indicator),
) {
    let cp = hal.dma_rx_current_pos();
    let mut d = packet::get_ptr_delta(cp, hal.dma_rx_read_pos(), DMA_RX_BUFFER_SIZE);

    while d >= RAW_PACKET_LENGTH as u32 {
        if hal.is_start_of_packet() {
            hal.fetch_packet();
            // Build a Rust UartPacket from the freshly-fetched 10 bytes.
            let p = hal.in_packet_ptr();
            if p.is_null() {
                return;
            }
            let pkt = unsafe {
                packet::UartPacket {
                    ptype: *p,
                    data: {
                        let mut b = [0u8; 8];
                        core::ptr::copy_nonoverlapping(p.add(1), b.as_mut_ptr(), 8);
                        b
                    },
                    checksum: *p.add(9),
                }
            };
            crate::service::packet_dispatch::dispatch_packet(state, hal, &pkt);
            return;
        }
        hal.dma_rx_advance_one();
        d -= 1;
    }
}

/// Check screensaver activation and generate mouse report if needed.
/// Returns updated last_pointer_move timestamp, or None if no report generated.
pub fn screensaver_tick(
    state: &DeviceState<'_>,
    hal: &(impl Timer + UsbDevice + ReportQueue),
    last_pointer_move: u32,
    report_bytes: &[u8; 8],
) -> Option<u32> {
    let role = state.cfg.board_role as usize;
    if role >= state.cfg.config.output.len() { return None; }

    let ss = &state.cfg.config.output[role].screensaver;
    let inactivity = hal.now_us_64() - state.cfg.last_activity[role];
    let current_time = hal.now_us_32();

    if !screensaver::should_activate(
        &ScreensaverConfig {
            mode: ss.mode,
            only_if_inactive: ss.only_if_inactive != 0,
            idle_time_us: ss.idle_time_us,
            max_time_us: ss.max_time_us,
        },
        inactivity,
        state.is_active_output(),
        hal.is_ready(),
        last_pointer_move,
        current_time,
    ) {
        return None;
    }

    if ss.mode != 1 && ss.mode != 2 { return None; }

    hal.push_mouse_report(report_bytes);
    Some(hal.now_us_32())
}

/// Execute one LED blink step. Called at 30 Hz from Core1 scheduler.
///
/// 5 transitions at 80ms intervals: OFF→ON→OFF→ON→OFF.
/// On the last transition, restore_leds() resets to normal active-output state.
pub fn led_blink_tick(
    state: &mut DeviceState<'_>,
    hal: &(impl Indicator + OutputControl + Timer),
) {
    use crate::domain::structs::LED_BLINK_PT_WAIT;

    // Defer while the boot-stage LED still owns the indicator (during
    // initial_setup, before the main loop hands off) so this Core1 task doesn't
    // fight the Core0 boot-LED timer over the GPIO.
    if crate::domain::boot_led::owns_led() {
        return;
    }

    // Drain a deferred upstream keyboard-LED resync requested by a Core0 context.
    // sync_leds() touches the host stack (tuh_hid_set_report), which is only safe
    // on this core (Core1) — see send_kbd_leds_xcore.
    if state.cfg.leds_resync_pending {
        state.cfg.leds_resync_pending = false;
        hal.sync_leds();
    }

    // PT_WAIT mode: slow pulse (50ms on / 450ms off) — independent of blinks_left
    if state.led.led_blink_mode == LED_BLINK_PT_WAIT {
        let now = hal.now_us_32();
        let elapsed = now.wrapping_sub(state.led.last_led_change as u32);
        let is_on = state.cfg.onboard_led_state;
        let threshold = if is_on { 50_000u32 } else { 450_000u32 };
        if elapsed >= threshold {
            hal.toggle();
            state.cfg.onboard_led_state = !is_on;
            state.led.last_led_change = now as i32;
        }
        return;
    }

    use crate::domain::blink::{blink_step, BlinkAction};

    // HID activity window: while a HID report was processed within this, the
    // active board's LED flickers instead of staying solid on.
    const HID_ACTIVITY_US: u32 = 60_000;
    // Flicker shape: a short OFF blip within each cycle (mostly-on, fast) so the
    // LED never sits dark during continuous use. Phase is derived from the timer
    // so it renders crisply at the task rate (hz120).
    const FLICKER_PERIOD_US: u32 = 60_000;
    const FLICKER_OFF_US: u32 = 15_000;

    let now = hal.now_us_32();
    match blink_step(state.led.blinks_left, state.led.last_led_change, now) {
        BlinkAction::Idle => {
            // Running steady state (sole board-LED authority): active board's LED
            // is on, inactive board off. While HID input is being processed, blink
            // the active LED with a short off pulse so the user sees activity — it
            // never stays solidly off during continuous use.
            if state.is_active_output() {
                if now.wrapping_sub(state.cfg.last_hid_activity_us) < HID_ACTIVITY_US {
                    hal.set_board_led((now % FLICKER_PERIOD_US) >= FLICKER_OFF_US);
                } else {
                    hal.set_board_led(true);
                }
            } else {
                hal.set_board_led(false);
            }
        }
        BlinkAction::Wait => {}
        BlinkAction::Toggle => {
            let led_on = hal.toggle();
            if state.cfg.keyboard_connected {
                hal.set_keyboard_leds(if led_on { 0x07 } else { 0x00 });
            }
            state.led.blinks_left -= 1;
            state.led.last_led_change = now as i32;
        }
        BlinkAction::ToggleAndRestore => {
            hal.toggle();
            state.led.blinks_left -= 1;
            state.led.last_led_change = now as i32;
            hal.sync_leds();
        }
    }
}

/// Build and send heartbeat packet. Handle config mode timeout.
/// Also checks BOOTSEL button for debug flash recovery (DH_DEBUG only).
pub fn heartbeat_tick(
    state: &DeviceState<'_>,
    hal: &(impl Timer + Watchdog + Indicator + PeerLink),
) {
    if state.fw.fw.upgrade_in_progress { return; }

    // Debug: BOOTSEL button triggers USB boot for flash recovery
    if hal.is_bootsel_pressed() {
        hal.reboot_to_bootloader();
    }

    if state.cfg.config_mode_active {
        if hal.now_us_64() > state.cfg.config_mode_timer {
            // Config mode self-exits via reboot after the idle window; otherwise
            // a board that "won't leave config mode" looks like a hang.
            crate::service::dlog::w(b"cfg").s(b"mode idle timeout -> reboot").done();
            hal.reboot();
        }
        hal.blink();
    }

    let version = state.fw._running_fw.version;
    let crc16 = state.fw._running_fw.checksum as u16;
    let mut pkt = [0u8; 10];
    pkt[0] = PacketType::Heartbeat as u8;
    pkt[1..1 + PACKET_DATA_LENGTH].copy_from_slice(
        &crate::domain::packet::build_heartbeat_payload(version, crc16, state.cfg.active_output),
    );

    hal.enqueue(&pkt);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::constants::RAW_PACKET_LENGTH;
    use crate::hal::mock::MockHal;

    // ---- check_system_health ----

    #[test]
    fn check_system_health_core1_alive() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.core1_last_loop_pass = 900_000; // 100ms ago
        assert!(check_system_health(&state, &hal));
        assert!(hal.watchdog_kicked.get());
    }

    #[test]
    fn led_blink_drains_pending_resync() {
        // A Core0-deferred LED resync is flushed here (on Core1) via sync_leds.
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        cfg.leds_resync_pending = true;
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        led_blink_tick(&mut state, &hal);
        assert!(!state.cfg.leds_resync_pending, "flag must be cleared");
        assert_eq!(hal.leds_synced.get(), 1, "sync_leds must run on the drain");
    }

    #[test]
    fn led_blink_no_resync_when_not_pending() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        led_blink_tick(&mut state, &hal);
        assert_eq!(hal.leds_synced.get(), 0);
    }

    #[test]
    fn led_steady_active_idle_is_on() {
        // Active board (active_output == board_role == 0), no recent HID activity
        // → solid on. Also covers restoring the LED after PT_WAIT/blink left it off.
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        hal.board_led.set(false); // pretend left off by a prior pulse
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        led_blink_tick(&mut state, &hal);
        assert!(hal.board_led.get(), "active idle board LED must be on");
    }

    #[test]
    fn led_steady_inactive_is_off() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        hal.board_led.set(true);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        cfg.board_role = 1; // active_output stays 0 → this board is inactive
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        led_blink_tick(&mut state, &hal);
        assert!(!hal.board_led.get(), "inactive board LED must be off");
    }

    #[test]
    fn led_active_flickers_on_hid_activity() {
        // During HID activity the active LED blinks with a short OFF window and is
        // mostly ON (never solid-off). Phase = now % 60ms; off for the first 15ms.
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();

        // OFF phase: now % 60ms < 15ms
        hal.set_time(5_000);
        cfg.last_hid_activity_us = 5_000; // recent → busy
        led_blink_tick(
            &mut DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led }, &hal);
        assert!(!hal.board_led.get(), "short off blip during activity");

        // ON phase: now % 60ms >= 15ms
        hal.set_time(40_000);
        cfg.last_hid_activity_us = 40_000; // recent → busy
        led_blink_tick(
            &mut DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led }, &hal);
        assert!(hal.board_led.get(), "mostly on during activity");
    }

    #[test]
    fn check_system_health_core1_hung() {
        let hal = MockHal::new();
        hal.set_time(2_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.core1_last_loop_pass = 1_000_000; // 1s ago
        assert!(!check_system_health(&state, &hal));
        assert!(!hal.watchdog_kicked.get());
    }

    #[test]
    fn check_system_health_reboot_requested() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.reboot_requested = true;
        assert!(!check_system_health(&state, &hal));
        assert!(!hal.watchdog_kicked.get());
    }

    #[test]
    fn check_system_health_boundary_exactly_500ms() {
        let hal = MockHal::new();
        hal.set_time(500_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.core1_last_loop_pass = 0; // exactly 500ms ago
        // 500_000 - 0 = 500_000, NOT < 500_000, so should NOT kick
        assert!(!check_system_health(&state, &hal));
    }

    #[test]
    fn check_system_health_boundary_499ms() {
        let hal = MockHal::new();
        hal.set_time(499_999);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.core1_last_loop_pass = 0; // 499.999ms ago
        assert!(check_system_health(&state, &hal));
        assert!(hal.watchdog_kicked.get());
    }

    // ---- flush_outbox ----

    #[test]
    fn flush_outbox_busy() {
        let hal = MockHal::new();
        hal.tx_busy.set(true);
        flush_outbox(&hal);
        assert!(hal.transmitted.borrow().is_empty());
    }

    #[test]
    fn flush_outbox_empty_queue() {
        let hal = MockHal::new();
        flush_outbox(&hal);
        assert!(hal.transmitted.borrow().is_empty());
    }

    #[test]
    fn flush_outbox_sends_packet() {
        let hal = MockHal::new();
        // Enqueue a packet: type=1, data=[2,3,4,5,6,7,8,9], checksum=10
        let pkt = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        hal.outbound_queue_in.borrow_mut().push(pkt);
        flush_outbox(&hal);
        assert_eq!(hal.transmitted.borrow().len(), 1);
        assert_eq!(hal.transmitted.borrow()[0].1, RAW_PACKET_LENGTH as u32);
    }

    // ---- heartbeat_tick ----

    #[test]
    fn heartbeat_sends_packet() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw._running_fw.version = 0x1234;
        state.cfg.active_output = 1;
        heartbeat_tick(&state, &hal);
        let pkts = hal.outbound_packets.borrow();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0][0], PacketType::Heartbeat as u8);
        assert_eq!(pkts[0][1], 0x34); // version low
        assert_eq!(pkts[0][2], 0x12); // version high
        assert_eq!(pkts[0][5], 1);    // active_output
    }

    #[test]
    fn heartbeat_skips_during_upgrade() {
        let hal = MockHal::new();
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.fw.fw.upgrade_in_progress = true;
        heartbeat_tick(&state, &hal);
        assert!(hal.outbound_packets.borrow().is_empty());
    }

    #[test]
    fn heartbeat_config_mode_blinks() {
        let hal = MockHal::new();
        hal.set_time(1_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.config_mode_active = true;
        state.cfg.config_mode_timer = 999_999_999; // far future
        heartbeat_tick(&state, &hal);
        assert_eq!(hal.blink_count.get(), 1);
        // Should still send heartbeat
        assert_eq!(hal.outbound_packets.borrow().len(), 1);
    }

    #[test]
    #[should_panic(expected = "MockHal::reboot")]
    fn heartbeat_config_mode_timeout_reboots() {
        let hal = MockHal::new();
        hal.set_time(1_000_000);
        let (mut hid, mut cfg, mut fw, mut led) = DeviceState::zeroed_for_test();
        let mut state = DeviceState { hid: &mut hid, cfg: &mut cfg, fw: &mut fw, led: &mut led };
        state.cfg.config_mode_active = true;
        state.cfg.config_mode_timer = 500_000; // past
        heartbeat_tick(&state, &hal);
    }
}
