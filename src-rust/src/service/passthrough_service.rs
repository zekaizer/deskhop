// Passthrough service — orchestrates domain passthrough logic with HAL calls.
// Ports C's passthrough_task(), USB callback integration, and report routing.

use crate::domain::passthrough::{self, PassthroughState, HIDPP_REPORT_ID_SHORT,
    HIDPP_SWID_DESKHOP, ITF_NUM_PT_BASE, HidppScanState};
use crate::domain::passthrough_scan;
use crate::domain::structs::{DeviceState, MouseReportC, LED_BLINK_NONE, LED_BLINK_PT_WAIT};
use crate::hal::traits::*;

/// Milliseconds to microseconds
const fn ms(ms: u64) -> u64 { ms * 1000 }

/// Delay after last descriptor capture before activation (500ms)
const CAPTURE_STABILIZE_US: u64 = ms(500);

/// Absolute safety timeout. If no vendor interface has appeared upstream
/// by this point, fall back to default DeskHop descriptors so the user
/// can still recover via the config-mode hotkey. Once this fires, late
/// captures are ignored until the next reboot.
const ABSOLUTE_TIMEOUT_US: u64 = ms(10_000);

/// Reconnect delay after disconnect (500ms — host needs time to detect removal)
const RECONNECT_DELAY_US: u64 = ms(500);

// ================================================================
// passthrough_task — periodic main-loop task
// ================================================================

/// Passthrough periodic task. Call from Core0 main loop.
///
/// Handles: SmartShift output switching, LED pulse mode, descriptor
/// activation with 500ms stabilization, reconnect after device removal,
/// output report forwarding, and HID++ scan stepping.
pub fn passthrough_task(
    pt: &mut PassthroughState,
    dev: &mut DeviceState<'_>,
    hal: &(impl PassthroughHal + UsbDevice + UsbHost + OutputControl + HidQueue + Timer),
) {
    // SmartShift double-click → toggle A/B output
    if dev.cfg.switch_requested {
        dev.cfg.switch_requested = false;
        let new_output = if dev.cfg.active_output == 0 { 1 } else { 0 };
        hal.switch_output(new_output);
    }

    // Config mode uses vendor HID at ITF_NUM_PT_BASE — skip passthrough
    if dev.cfg.config_mode_active {
        return;
    }

    let now = hal.now_us_64();

    // LED: slow pulse while device side is disconnected (waiting for host captures)
    if !dev.cfg.tud_connected && dev.led.led_blink_mode != LED_BLINK_PT_WAIT {
        dev.led.led_blink_mode = LED_BLINK_PT_WAIT;
    }

    // Absolute safety net (production policy). After ABSOLUTE_TIMEOUT_US
    // without a vendor interface upstream, fall back to default DeskHop
    // descriptors and lock out late captures so the user can recover via
    // config-mode hotkey instead of reflashing.
    if !pt.gave_up
        && !dev.cfg.tud_connected
        && now > ABSOLUTE_TIMEOUT_US
        && !passthrough::has_vendor_interface(pt)
    {
        hal.device_connect();
        pt.gave_up = true;
    }

    // Activate after host captures stabilize.
    // DEBUG: vendor/HID++ interface check removed — any captured interface
    // triggers activation so non-Logitech mice can also be exercised on the
    // bring-up rig. Production policy should require has_vendor_interface(pt).
    if dev.cfg.config.passthrough_enabled != 0
        && !pt.active
        && !pt.gave_up
        && pt.iface_count > 0
        && pt.last_capture_us > 0
        && now.wrapping_sub(pt.last_capture_us) > CAPTURE_STABILIZE_US
    {
        let desc_ready = hal.build_config_desc();
        if passthrough::activate(pt, desc_ready) {
            dev.cfg.gaming_mode = true;
            // Force re-enumeration: disconnect default DeskHop, then let the
            // reconnect_at_us branch below bring the device side back up with
            // the passthrough composite descriptor. Boot-time disconnect was
            // tried but most PC USB stacks ignore the brief SE0 and keep the
            // cached enumeration; an explicit cycle from a connected state is
            // what triggers the host to re-fetch descriptors.
            hal.device_disconnect();
            pt.reconnect_at_us = now + RECONNECT_DELAY_US;
        }
    }

    // LED: stop blinking once host is connected
    if dev.cfg.tud_connected && dev.led.led_blink_mode == LED_BLINK_PT_WAIT {
        dev.led.led_blink_mode = LED_BLINK_NONE;
    }

    // Phase 2 fallback: device removal → reconnect with new descriptors
    if pt.reconnect_at_us > 0 && now >= pt.reconnect_at_us {
        hal.device_connect();
        pt.reconnect_at_us = 0;
    }

    // Flush deferred HID++ output report
    if pt.out_queue.pending {
        hal.send_set_report(
            pt.out_queue.dev_addr,
            pt.out_queue.instance,
            pt.out_queue.report_id,
            pt.out_queue.report_type,
            &pt.out_queue.data[..pt.out_queue.len as usize],
        );
        pt.out_queue.pending = false;
    }

    // HID++ scan step
    if pt.hidpp_scan.state == HidppScanState::QueryIRoot {
        if let Some(q) = passthrough_scan::scan_step(pt, now) {
            pt.out_queue = q;
        }
    }

    // SmartShift double-click window expiry: flush buffered events so the
    // host sees a delayed-but-correct click sequence.
    if pt.smartshift_window_us > 0 {
        let window_us = (dev.cfg.config.smartshift_double_click_ms as u64) * 1000;
        if now.wrapping_sub(pt.smartshift_window_us) > window_us {
            flush_smartshift_buffer(pt, hal);
            pt.smartshift_window_us = 0;
        }
    }
}

// ================================================================
// USB callback integration
// ================================================================

/// Called from tuh_hid_mount_cb when a host device is mounted.
/// Captures descriptor for passthrough if enabled.
pub fn on_device_mount(
    pt: &mut PassthroughState,
    dev: &DeviceState<'_>,
    dev_addr: u8,
    instance: u8,
    itf_protocol: u8,
    desc: &[u8],
    hal: &(impl UsbHost + Timer),
) {
    if dev.cfg.config.passthrough_enabled == 0 {
        return;
    }

    passthrough::capture_descriptor(pt, dev_addr, instance, itf_protocol, desc);
    pt.last_capture_us = hal.now_us_64();

    // Capture upstream VID/PID once per device
    if pt.upstream_vid == 0 {
        let (vid, pid) = hal.get_upstream_vid_pid(dev_addr);
        pt.upstream_vid = vid;
        pt.upstream_pid = pid;
    }
}

/// Called from tuh_hid_umount_cb when a host device interface is removed.
/// Cleans up passthrough state and triggers reconnection when all interfaces are gone.
pub fn on_device_unmount(
    pt: &mut PassthroughState,
    dev_addr: u8,
    hal: &(impl PassthroughHal + UsbDevice + Timer),
) {
    let was_active = pt.active;
    passthrough::remove_device(pt, dev_addr);

    // Re-enumerate as DeskHop when all passthrough interfaces are gone.
    // Note: remove_device() resets active=false when iface_count reaches 0,
    // so we check `was_active` to know if passthrough was engaged before removal.
    if was_active && pt.iface_count == 0 {
        hal.clear_config_desc();
        hal.device_disconnect();
        pt.reconnect_at_us = hal.now_us_64() + RECONNECT_DELAY_US;
    }
}

/// Result of on_report_received: tells caller whether to continue
/// with normal DeskHop processing or skip it.
pub enum ReportAction {
    /// Report was handled by passthrough — skip DeskHop processing
    Handled,
    /// Fall through to normal DeskHop processing
    Fallthrough,
}

/// Called from tuh_hid_report_received_cb for non-keyboard reports.
/// Routes HID++ and passthrough reports, converts inactive-output
/// HID++ to standard mouse reports.
///
/// Returns Handled if report was consumed, Fallthrough if DeskHop should process it.
pub fn on_report_received(
    pt: &mut PassthroughState,
    dev: &mut DeviceState<'_>,
    report: &[u8],
    dev_addr: u8,
    instance: u8,
    hal: &(impl UsbHost + HidQueue + ReportQueue + PeerLink + Timer),
) -> ReportAction {
    if !pt.active || dev.cfg.config_mode_active {
        return ReportAction::Fallthrough;
    }

    let dev_inst = match passthrough::host_to_device_instance(pt, dev_addr, instance) {
        Some(di) => di,
        None => return ReportAction::Fallthrough,
    };

    let idx = (dev_inst - ITF_NUM_PT_BASE) as usize;

    // HID++ vendor interfaces: route by message type
    if pt.ifaces[idx].always_passthrough {
        // Intercept DeskHop sw_id responses (scan only)
        if report.len() >= 7 && (report[3] & 0x0F) == HIDPP_SWID_DESKHOP {
            if pt.hidpp_scan.state == HidppScanState::QueryIRoot {
                passthrough_scan::handle_scan_response(pt, report);
            }
            hal.receive_report(dev_addr, instance);
            return ReportAction::Handled;
        }

        let is_input = passthrough::is_hidpp_input_event(report);

        // SmartShift double-click state machine. On first SmartShift press,
        // buffer the event and start a window. On a second press within the
        // window, consume both presses + the upcoming release and trigger an
        // output switch — the host never sees the click. On window expiry,
        // buffered events are flushed by passthrough_task so the host sees a
        // delayed-but-correct sequence.
        if is_input {
            let now_us = hal.now_us_64();
            let window_us = (dev.cfg.config.smartshift_double_click_ms as u64) * 1000;

            // (a) consume the release that follows a switch-trigger press
            if pt.smartshift_consume > 0 && passthrough::is_smartshift_event(report) && report[6] == 0 {
                pt.smartshift_consume -= 1;
                hal.receive_report(dev_addr, instance);
                return ReportAction::Handled;
            }

            // (b) inside an active double-click window
            if pt.smartshift_window_us > 0 {
                let elapsed = now_us.wrapping_sub(pt.smartshift_window_us);
                if passthrough::is_smartshift_press(report) {
                    if elapsed <= window_us {
                        // Second press within window — consume buffered + this press,
                        // mark the upcoming release for consumption, trigger switch.
                        passthrough::smartshift_reset(pt);
                        pt.smartshift_consume = 1;
                        dev.cfg.switch_requested = true;
                        hal.receive_report(dev_addr, instance);
                        return ReportAction::Handled;
                    }
                    // Window expired — let passthrough_task flush on the next tick;
                    // start a fresh window with this press as the new Down1.
                    flush_smartshift_buffer(pt, hal);
                    pt.smartshift_window_us = now_us;
                    if !passthrough::smartshift_buf_push(pt, dev_inst, report) {
                        // Should not happen with empty buffer, but stay safe.
                        passthrough::smartshift_reset(pt);
                        // Fall through to normal forwarding below.
                    } else {
                        hal.receive_report(dev_addr, instance);
                        return ReportAction::Handled;
                    }
                } else {
                    // Non-SmartShift event during window — buffer to preserve order.
                    if passthrough::smartshift_buf_push(pt, dev_inst, report) {
                        hal.receive_report(dev_addr, instance);
                        return ReportAction::Handled;
                    }
                    // Buffer overflow — abandon double-click detection: flush
                    // everything and let the current event take the normal path.
                    flush_smartshift_buffer(pt, hal);
                    passthrough::smartshift_reset(pt);
                }
            }
            // (c) idle state, first SmartShift press → enter pending
            else if passthrough::is_smartshift_press(report) {
                pt.smartshift_window_us = now_us;
                if passthrough::smartshift_buf_push(pt, dev_inst, report) {
                    hal.receive_report(dev_addr, instance);
                    return ReportAction::Handled;
                }
                // Buffer immediately full (unreachable with empty buffer) → reset.
                passthrough::smartshift_reset(pt);
            }
        }

        let is_active = dev.is_active_output();

        if !is_input || is_active {
            // Protocol responses always forward; input events only on active output
            hal.send_hid_report(dev_inst, 0, report);
        } else {
            // Inactive output: convert HID++ → mouse for peer routing.
            // Always RELATIVE — passthrough activate forces gaming_mode=true,
            // and a stale ABSOLUTE branch here would inject pointer_x/y from a
            // mouse pipeline that's been bypassed by the vendor interface.
            let prev_btn = pt.hidpp_disc.button_state;
            let mut mouse = MouseReportC::default();
            let converted = passthrough::convert_hidpp_to_mouse(pt, report, &mut mouse);

            if pt.hidpp_disc.button_state != prev_btn {
                let btn_report = MouseReportC {
                    buttons: pt.hidpp_disc.button_state,
                    mode: 1, // RELATIVE
                    ..Default::default()
                };
                let report_bytes = mouse_report_to_bytes(&btn_report);
                hal.push_mouse_report(&report_bytes);
            } else if converted {
                mouse.mode = 1; // RELATIVE
                let report_bytes = mouse_report_to_bytes(&mouse);
                hal.push_mouse_report(&report_bytes);
            }
        }

        hal.receive_report(dev_addr, instance);
        return ReportAction::Handled;
    }

    // Non-vendor, non-keyboard: raw passthrough on active output only
    let is_active = dev.is_active_output();
    if is_active {
        hal.send_hid_report(dev_inst, 0, report);
        hal.receive_report(dev_addr, instance);
        return ReportAction::Handled;
    }

    // Inactive output: fall through to DeskHop mouse processing
    hal.receive_report(dev_addr, instance);
    ReportAction::Fallthrough
}

/// Queue an output report from device-side set_report callback.
/// Called when host software (e.g. Logitech Options+) sends SET_REPORT
/// on a passthrough interface.
pub fn on_set_report(
    pt: &mut PassthroughState,
    instance: u8,
    report_id: u8,
    report_type: u8,
    buffer: &[u8],
) {
    let idx = match passthrough::device_to_host_index(pt, instance) {
        Some(i) => i as usize,
        None => return,
    };

    if buffer.len() > pt.out_queue.data.len() {
        return;
    }

    // Queue for deferred send from main loop
    pt.out_queue.dev_addr = pt.ifaces[idx].dev_addr;
    pt.out_queue.instance = pt.ifaces[idx].instance;
    pt.out_queue.report_id = report_id;
    pt.out_queue.report_type = report_type;
    pt.out_queue.len = buffer.len() as u16;
    pt.out_queue.data[..buffer.len()].copy_from_slice(buffer);
    pt.out_queue.pending = true;

    // Passive sniffing: learn HID++ feature indices
    if report_id == HIDPP_REPORT_ID_SHORT && buffer.len() >= 4 {
        let device_idx = buffer[0];
        if (1..=6).contains(&device_idx) && (buffer[2] & 0x0F) != 0 {
            passthrough::sniff_feature_indices(&mut pt.hidpp_disc, buffer);
        }
    }
}

// ================================================================
// Helpers
// ================================================================

/// Forward all events buffered by the SmartShift double-click state machine
/// to the host in capture order, then clear the buffer count. Caller is
/// responsible for resetting smartshift_window_us.
fn flush_smartshift_buffer(pt: &mut PassthroughState, hal: &impl HidQueue) {
    let count = pt.smartshift_buf_count as usize;
    for i in 0..count {
        let entry = &pt.smartshift_buf[i];
        let len = entry.len as usize;
        hal.send_hid_report(entry.dev_inst, 0, &entry.data[..len]);
    }
    pt.smartshift_buf_count = 0;
}

/// Serialize MouseReportC to a byte array for queue pushing.
fn mouse_report_to_bytes(m: &MouseReportC) -> [u8; 8] {
    let x_bytes = m.x.to_le_bytes();
    let y_bytes = m.y.to_le_bytes();
    [
        m.buttons,
        x_bytes[0], x_bytes[1],
        y_bytes[0], y_bytes[1],
        m.wheel as u8,
        m.pan as u8,
        m.mode,
    ]
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::passthrough;
    use crate::domain::structs::{DeviceState, DeviceConfig, DeviceHid, DeviceFw, DeviceLed};
    use crate::hal::mock::MockHal;

    fn setup() -> (PassthroughState, DeviceHid, DeviceConfig, DeviceFw, DeviceLed, MockHal) {
        let pt = PassthroughState::default();
        let (hid, cfg, fw, led) = DeviceState::zeroed_for_test();
        let hal = MockHal::new();
        (pt, hid, cfg, fw, led, hal)
    }

    macro_rules! dev {
        ($hid:expr, $cfg:expr, $fw:expr, $led:expr) => {
            DeviceState { hid: &mut $hid, cfg: &mut $cfg, fw: &mut $fw, led: &mut $led }
        };
    }

    // -- passthrough_task tests --

    #[test]
    fn task_switch_requested_toggles_output() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.switch_requested = true;
        cfg.active_output = 0;
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!cfg.switch_requested);
        assert_eq!(hal.output_switched.get(), Some(1));
    }

    #[test]
    fn task_config_mode_skips() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config_mode_active = true;
        cfg.switch_requested = true;
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!cfg.switch_requested);
    }

    #[test]
    fn task_led_pulse_when_disconnected() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.tud_connected = false;
        led.led_blink_mode = LED_BLINK_NONE;
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert_eq!(led.led_blink_mode, LED_BLINK_PT_WAIT);
    }

    #[test]
    fn task_led_stops_when_connected() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.tud_connected = true;
        led.led_blink_mode = LED_BLINK_PT_WAIT;
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert_eq!(led.led_blink_mode, LED_BLINK_NONE);
    }

    #[test]
    fn task_safety_timeout_no_vendor() {
        // No vendor iface ever appears — safety net should fire at ABSOLUTE_TIMEOUT_US
        // and lock out passthrough activation (gave_up = true).
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        cfg.tud_connected = false;
        // Capture a non-vendor iface (e.g. boot mouse, itf_protocol = 2)
        passthrough::capture_descriptor(&mut pt, 1, 0, 2, &[0x05, 0x01]);
        pt.last_capture_us = 1000;
        hal.set_time(ABSOLUTE_TIMEOUT_US + 1);

        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(pt.gave_up, "gave_up should be set after safety timeout");
        assert!(!pt.active, "passthrough must not activate once gave_up is set");
    }

    #[test]
    fn task_safety_timeout_inhibits_late_activate() {
        // After the safety net fires, even a properly-stabilized vendor capture
        // arriving later must NOT trigger activation — user must reboot.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        pt.gave_up = true; // safety net already fired

        // Stabilized vendor iface capture
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &[0x06, 0x00, 0xFF]);
        pt.last_capture_us = 1000;
        hal.set_time(1000 + CAPTURE_STABILIZE_US + 1);

        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!pt.active, "late capture must not activate after gave_up");
    }

    #[test]
    fn task_safety_timeout_skipped_when_vendor_present() {
        // If a vendor iface has been captured, the safety net does NOT fire
        // — we're still in the normal activation race window.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        cfg.tud_connected = false;
        // Vendor iface (itf_protocol = 0 → always_passthrough)
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &[0x06, 0x00, 0xFF]);
        pt.last_capture_us = ABSOLUTE_TIMEOUT_US + 100;
        hal.set_time(ABSOLUTE_TIMEOUT_US + 200);

        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!pt.gave_up, "gave_up must not be set when vendor iface is present");
    }

    #[test]
    fn task_activate_after_stabilize() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        let desc = [0x05, 0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &desc);
        pt.last_capture_us = 1000;
        let now = 1000 + CAPTURE_STABILIZE_US + 1;
        hal.set_time(now);

        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(pt.active);
        // Activate triggers the disconnect→reconnect cycle so the host
        // re-enumerates with the passthrough composite — connect happens
        // RECONNECT_DELAY_US later, not immediately.
        assert_eq!(hal.device_disconnect_count.get(), 1);
        assert_eq!(hal.device_connect_count.get(), 0);
        assert_eq!(pt.reconnect_at_us, now + RECONNECT_DELAY_US);
    }

    #[test]
    fn task_reconnect_timer() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        pt.reconnect_at_us = 5000;
        hal.set_time(6000);
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert_eq!(hal.device_connect_count.get(), 1);
        assert_eq!(pt.reconnect_at_us, 0);
    }

    #[test]
    fn task_flushes_output_queue() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        pt.out_queue.pending = true;
        pt.out_queue.data[0] = 0xAA;
        pt.out_queue.len = 1;
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!pt.out_queue.pending);
        assert_eq!(hal.host_set_report_count.get(), 1);
    }

    // -- on_device_mount tests --

    #[test]
    fn mount_captures_descriptor() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        hal.set_time(1234);
        on_device_mount(&mut pt, &dev!(hid, cfg, fw, led), 1, 0, 1, &[0x05, 0x01], &hal);
        assert_eq!(pt.iface_count, 1);
        assert_eq!(pt.last_capture_us, 1234);
    }

    #[test]
    fn mount_captures_vid_pid() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        hal.host_upstream_vid_pid.set((0x046D, 0xC548));
        on_device_mount(&mut pt, &dev!(hid, cfg, fw, led), 1, 0, 1, &[0x05], &hal);
        assert_eq!(pt.upstream_vid, 0x046D);
        assert_eq!(pt.upstream_pid, 0xC548);
    }

    #[test]
    fn mount_disabled_noop() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 0;
        on_device_mount(&mut pt, &dev!(hid, cfg, fw, led), 1, 0, 1, &[0x05], &hal);
        assert_eq!(pt.iface_count, 0);
    }

    // -- on_device_unmount tests --

    #[test]
    fn unmount_triggers_reconnect_when_empty() {
        let (mut pt, _hid, _cfg, _fw, _led, hal) = setup();
        let desc = [0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 1, &desc);
        pt.active = true;
        hal.set_time(10000);

        on_device_unmount(&mut pt, 1, &hal);
        assert!(!pt.active);
        assert!(!hal.pt_config_desc_ready.get());
        assert_eq!(hal.device_disconnect_count.get(), 1);
        assert_eq!(pt.reconnect_at_us, 10000 + RECONNECT_DELAY_US);
    }

    #[test]
    fn unmount_partial_no_reconnect() {
        let (mut pt, _hid, _cfg, _fw, _led, hal) = setup();
        let desc = [0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 1, &desc);
        passthrough::capture_descriptor(&mut pt, 2, 0, 1, &desc);
        pt.active = true;

        on_device_unmount(&mut pt, 1, &hal);
        assert!(pt.active);
        assert_eq!(hal.device_disconnect_count.get(), 0);
    }

    // -- on_set_report tests --

    #[test]
    fn set_report_queues_output() {
        let mut pt = PassthroughState::default();
        let desc = [0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &desc);

        on_set_report(&mut pt, ITF_NUM_PT_BASE, 0x10, 2, &[0x01, 0x08, 0x21, 0x03]);
        assert!(pt.out_queue.pending);
        assert_eq!(pt.out_queue.report_id, 0x10);
        assert_eq!(pt.out_queue.len, 4);
    }

    #[test]
    fn set_report_sniffs_hires_scroll() {
        let mut pt = PassthroughState::default();
        let desc = [0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &desc);

        on_set_report(&mut pt, ITF_NUM_PT_BASE, HIDPP_REPORT_ID_SHORT, 2,
                      &[0x01, 0x08, 0x21, 0x03, 0x00, 0x00]);
        assert_eq!(pt.hidpp_disc.fi_hires_scroll, 0x08);
    }

    #[test]
    fn set_report_invalid_instance_noop() {
        let mut pt = PassthroughState::default();
        on_set_report(&mut pt, 0, 0x10, 2, &[0x01]);
        assert!(!pt.out_queue.pending);
    }

    // -- SmartShift double-click state machine --

    /// Build a HID++ short input event for SmartShift CID (0xC4).
    /// `pressed` true = press (action != 0), false = release.
    fn smartshift_event(pressed: bool) -> [u8; 7] {
        [
            HIDPP_REPORT_ID_SHORT,
            0x01,         // device_idx
            0x05,         // feature_idx (ReprogControls, learned)
            0x20,         // (fn=2 << 4) | sw_id=0
            0x00,
            passthrough::SMARTSHIFT_CID,
            if pressed { 0x01 } else { 0x00 },
        ]
    }

    fn ss_setup() -> (PassthroughState, DeviceHid, DeviceConfig, DeviceFw, DeviceLed, MockHal) {
        let mut t = setup();
        // Vendor iface (always_passthrough = true via itf_protocol = 0)
        passthrough::capture_descriptor(&mut t.0, 1, 0, 0, &[0x06, 0x00, 0xFF]);
        t.0.active = true;
        t.2.config.smartshift_double_click_ms = 350;
        t
    }

    #[test]
    fn smartshift_first_press_buffers() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        let report = smartshift_event(true);
        let action = on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                                        &report, 1, 0, &hal);
        assert!(matches!(action, ReportAction::Handled));
        assert_eq!(pt.smartshift_window_us, 1_000_000);
        assert_eq!(pt.smartshift_buf_count, 1);
        assert_eq!(hal.hid_sent.borrow().len(), 0, "press must not be forwarded immediately");
    }

    #[test]
    fn smartshift_second_press_within_window_triggers_switch() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        let press = smartshift_event(true);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        // 100ms later, second press
        hal.set_time(1_100_000);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        assert!(cfg.switch_requested, "second press within window must trigger switch");
        assert_eq!(pt.smartshift_window_us, 0, "window must clear");
        assert_eq!(pt.smartshift_buf_count, 0, "buffer must clear");
        assert_eq!(pt.smartshift_consume, 1, "release of second press must be marked for consumption");
        assert_eq!(hal.hid_sent.borrow().len(), 0, "no events forwarded on switch trigger");
    }

    #[test]
    fn smartshift_release_after_switch_consumed() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        pt.smartshift_consume = 1;
        let release = smartshift_event(false);
        let action = on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                                        &release, 1, 0, &hal);
        assert!(matches!(action, ReportAction::Handled));
        assert_eq!(pt.smartshift_consume, 0);
        assert_eq!(hal.hid_sent.borrow().len(), 0, "release must be silently dropped");
    }

    #[test]
    fn smartshift_window_expiry_flushes_buffer() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        let press = smartshift_event(true);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        // 400ms later — window expired (350ms threshold)
        hal.set_time(1_400_000);
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert_eq!(pt.smartshift_window_us, 0);
        assert_eq!(pt.smartshift_buf_count, 0);
        assert_eq!(hal.hid_sent.borrow().len(), 1, "buffered press must be flushed");
        assert_eq!(hal.hid_sent.borrow()[0].2, press.to_vec());
    }

    #[test]
    fn smartshift_third_press_starts_new_window() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        let press = smartshift_event(true);
        // First press at T=1s
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        // Third press at T=1.5s — past window expiry (350ms)
        hal.set_time(1_500_000);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        // First press is flushed, new window opened
        assert_eq!(pt.smartshift_window_us, 1_500_000, "new window opened at third press");
        assert_eq!(pt.smartshift_buf_count, 1, "third press buffered as new Down1");
        assert_eq!(hal.hid_sent.borrow().len(), 1, "first press flushed before new window");
    }

    #[test]
    fn smartshift_non_event_during_window_buffered() {
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        let press = smartshift_event(true);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        // A scroll event (not SmartShift) during the window must be buffered
        // to preserve ordering when the window flushes.
        let scroll = [HIDPP_REPORT_ID_SHORT, 0x01, 0x06, 0x00, 0x00, 0x05, 0x00];
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &scroll, 1, 0, &hal);
        assert_eq!(pt.smartshift_buf_count, 2, "scroll event buffered alongside press");
        assert_eq!(hal.hid_sent.borrow().len(), 0);
    }
}
