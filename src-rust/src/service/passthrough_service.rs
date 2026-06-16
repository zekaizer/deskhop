// Passthrough service — orchestrates domain passthrough logic with HAL calls.
// Ports C's passthrough_task(), USB callback integration, and report routing.

use crate::domain::passthrough::{self, PassthroughState, HIDPP_REPORT_ID_SHORT,
    HIDPP_SWID_DESKHOP, ITF_NUM_PT_BASE, HidppScanState};
use crate::domain::passthrough_scan;
use crate::domain::structs::{DeviceState, MouseReportC, LED_BLINK_NONE, LED_BLINK_PT_WAIT};
use crate::hal::traits::*;
use crate::service::router::ReportRouter;

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
        crate::service::dlog::i(b"pt").s(b"switch out=").hx(new_output).done();
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
        crate::service::dlog::w(b"pt").s(b"safety_net fired").done();
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
        if !desc_ready {
            crate::service::dlog::w(b"pt").s(b"desc build FAILED (HID budget exceeded?)").done();
        }
        if passthrough::activate(pt, desc_ready) {
            crate::service::dlog::i(b"pt")
                .s(b"activate n=").hx(pt.iface_count)
                .s(b" vid=").hx16(pt.upstream_vid)
                .s(b" disconnect+reconnect")
                .done();
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

    // Deferred device removal: on_device_unmount (Core1) only flags the request;
    // the tud_disconnect must run here on Core0, which owns the device stack.
    if pt.disconnect_requested {
        pt.disconnect_requested = false;
        crate::service::dlog::i(b"pt").s(b"unmount disconnect").done();
        hal.clear_config_desc();
        hal.device_disconnect();
        pt.reconnect_at_us = now + RECONNECT_DELAY_US;
    }

    // Phase 2 fallback: device removal → reconnect with new descriptors
    if pt.reconnect_at_us > 0 && now >= pt.reconnect_at_us {
        crate::service::dlog::i(b"pt").s(b"reconnect connect").done();
        hal.device_connect();
        pt.reconnect_at_us = 0;
    }

    // NOTE: the deferred HID++ output report (out_queue) is flushed by
    // flush_output_report() on Core1, NOT here. The flush issues a host
    // tuh_control_xfer; doing that from this Core0 task races the Core1 USB
    // host stack (tuh_task) with no lock and hangs Core1. See flush_output_report.

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

/// Flush a pending HID++ output report (queued by on_set_report from the
/// device-side SET_REPORT callback, or by the HID++ scan) to the upstream
/// device via a host SET_REPORT control transfer.
///
/// MUST run on Core1, where the TinyUSB host stack (tuh_task) lives. The send
/// issues a blocking tuh_control_xfer; the cooperative Core1 scheduler
/// serializes it with tuh_task, so there is no concurrent access to the host
/// stack. Calling this from Core0 instead races tuh_task with no lock and
/// hangs Core1 (observed: core1_last_loop_pass freezes when Logitech Options+
/// streams HID++ SET_REPORTs, killing the passed-through mouse).
///
/// The out_queue is a single slot: a SET_REPORT arriving while a previous one
/// is still pending overwrites it. That is a known robustness limitation, not
/// a correctness hazard here — losing a HID++ config command at worst makes
/// host software retry; it cannot hang the stack.
pub fn flush_output_report(pt: &mut PassthroughState, hal: &impl UsbHost) {
    if !pt.out_queue.pending {
        return;
    }
    let ok = hal.send_set_report(
        pt.out_queue.dev_addr,
        pt.out_queue.instance,
        pt.out_queue.report_id,
        pt.out_queue.report_type,
        &pt.out_queue.data[..pt.out_queue.len as usize],
    );
    if !ok {
        crate::service::dlog::w(b"pt").s(b"host_tx busy/failed (report dropped)").done();
    }
    pt.out_queue.pending = false;
}

// ================================================================
// USB callback integration
// ================================================================

/// Called from tuh_hid_mount_cb when a host device is mounted.
/// Captures descriptor for passthrough if enabled.
///
/// KNOWN LIMITATION (cross-core, documented & deferred): this runs on Core1 and
/// `capture_descriptor` mutates `ifaces[]`/`iface_count`, which the device
/// descriptor callbacks (rust_get_*_descriptor) read on Core0 during the host's
/// GET_DESCRIPTOR. In the normal flow these never overlap — captures finish and
/// stabilize (CAPTURE_STABILIZE_US) before activation triggers the re-enum the
/// host reads. They could only race in the narrow window where an upstream
/// mount/unmount lands while the host is mid-enumeration (e.g. a very fast
/// dongle re-plug). The textbook fix is to serialize mount/umount to Core0 via
/// a Core1→Core0 event queue (carrying the captured descriptor), but that
/// re-architects this enumeration path; deferred until a concrete descriptor-
/// corruption symptom is observed. The device-stack hazards on this path
/// (device_disconnect / report forwarding) ARE already moved to Core0.
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

    let ok = passthrough::capture_descriptor(pt, dev_addr, instance, itf_protocol, desc);
    pt.last_capture_us = hal.now_us_64();
    crate::service::dlog::i(b"pt")
        .s(b"cap addr=").hx(dev_addr)
        .s(b" inst=").hx(instance)
        .s(b" proto=").hx(itf_protocol)
        .s(b" len=").hx(desc.len() as u8)
        .s(b" n=").hx(pt.iface_count)
        .s(if ok { b" ok" } else { b" FAIL(full?)" })
        .done();

    // Capture upstream VID/PID once per device
    if pt.upstream_vid == 0 {
        let (vid, pid) = hal.get_upstream_vid_pid(dev_addr);
        pt.upstream_vid = vid;
        pt.upstream_pid = pid;
        crate::service::dlog::i(b"pt")
            .s(b"upstream vid=").hx16(vid)
            .s(b" pid=").hx16(pid)
            .done();
    }
}

/// Called from tuh_hid_umount_cb when a host device interface is removed.
/// Cleans up passthrough state and triggers reconnection when all interfaces are gone.
pub fn on_device_unmount(pt: &mut PassthroughState, dev_addr: u8) {
    let was_active = pt.active;
    passthrough::remove_device(pt, dev_addr);

    // Re-enumerate as DeskHop when all passthrough interfaces are gone.
    // Note: remove_device() resets active=false when iface_count reaches 0,
    // so we check `was_active` to know if passthrough was engaged before removal.
    //
    // This runs on Core1 (tuh umount callback) and must NOT touch the device
    // stack: calling tud_disconnect here races Core0's tud_task and hangs Core1.
    // Flag the request; passthrough_task performs clear_config_desc +
    // device_disconnect + reconnect on Core0.
    if was_active && pt.iface_count == 0 {
        crate::service::dlog::i(b"pt").s(b"unmount request disconnect").done();
        pt.disconnect_requested = true;
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
    hal: &(impl UsbHost + HidQueue + ReportQueue + PacketQueue + PeerLink + Timer),
) -> ReportAction {
    if !pt.active || dev.cfg.config_mode_active {
        return ReportAction::Fallthrough;
    }

    // Device side is mid-re-enumeration: activate() disconnected it and tud has
    // not re-mounted yet (tud_connected goes true again only on rust_on_tud_mount).
    // The passthrough forward path calls tud_hid_n_report directly from this
    // Core1 callback; doing that into a device stack Core0 is still bringing up
    // races tud_task and hangs Core1 (observed: moving the mouse continuously
    // during boot freezes core1_last_loop_pass and kills the mouse). Drop the
    // report until the device re-mounts — but still re-arm reception so reports
    // resume afterwards. Silent on purpose: logging per dropped report would
    // overflow the peer_log ring under continuous movement.
    if !dev.cfg.tud_connected {
        hal.receive_report(dev_addr, instance);
        return ReportAction::Handled;
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
            if pt.smartshift_consume > 0 && passthrough::is_smartshift_release(report) {
                pt.smartshift_consume -= 1;
                hal.receive_report(dev_addr, instance);
                return ReportAction::Handled;
            }

            // (b) inside an active double-click window
            if pt.smartshift_window_us > 0 {
                let elapsed = now_us.wrapping_sub(pt.smartshift_window_us);
                if passthrough::is_smartshift_press(report) {
                    if elapsed <= window_us && pt.smartshift_saw_release {
                        // Down2 within the window AND a Down1->Up1 release was seen
                        // in between: a genuine double-click. Consume the buffered
                        // events + this press, mark the upcoming release for
                        // consumption, trigger the switch — the host never sees it.
                        crate::service::dlog::i(b"pt").s(b"smartshift double-click").done();
                        passthrough::smartshift_reset(pt);
                        pt.smartshift_consume = 1;
                        dev.cfg.switch_requested = true;
                        hal.receive_report(dev_addr, instance);
                        return ReportAction::Handled;
                    }
                    if elapsed > window_us {
                        // Window expired — let passthrough_task flush on the next
                        // tick; start a fresh window with this press as the new Down1.
                        flush_smartshift_buffer(pt, hal);
                        pt.smartshift_window_us = now_us;
                        pt.smartshift_saw_release = false;
                        if !passthrough::smartshift_buf_push(pt, dev_inst, report) {
                            // Should not happen with empty buffer, but stay safe.
                            passthrough::smartshift_reset(pt);
                            // Fall through to normal forwarding below.
                        } else {
                            hal.receive_report(dev_addr, instance);
                            return ReportAction::Handled;
                        }
                    } else {
                        // A press inside the window with no intervening release: a
                        // repeated/duplicated Down for the SAME physical actuation
                        // (e.g. an MX Master emitting the wheel-mode key as both an
                        // fn=2 analyticsKeyEvent and an fn=0 divertedButtons frame, or
                        // a held divertedButtons bitmap re-sent). NOT a double-click —
                        // buffer it and keep waiting for a real Up->Down sequence.
                        if passthrough::smartshift_buf_push(pt, dev_inst, report) {
                            hal.receive_report(dev_addr, instance);
                            return ReportAction::Handled;
                        }
                        flush_smartshift_buffer(pt, hal);
                        passthrough::smartshift_reset(pt);
                    }
                } else {
                    // Non-press HID++ input event during the window. If it is the
                    // release (Up1) of the SmartShift button, record it so the next
                    // press can complete the double-click.
                    if passthrough::is_smartshift_release(report) {
                        pt.smartshift_saw_release = true;
                    }
                    // Buffer to preserve event order.
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
                pt.smartshift_saw_release = false;
                if passthrough::smartshift_buf_push(pt, dev_inst, report) {
                    hal.receive_report(dev_addr, instance);
                    return ReportAction::Handled;
                }
                // Buffer immediately full (unreachable with empty buffer) → reset.
                passthrough::smartshift_reset(pt);
            }
        }

        // Log discrete HID++ control events (MX Master special keys etc.) on the
        // receiver board regardless of active output, so the [pt] key trace shows
        // up even when this board isn't the active one. Scroll/thumbwheel
        // streams are suppressed inside.
        if is_input {
            log_hidpp_event(pt, report, hal.now_us_64());
        }

        // HID++ -> standard-key remap for the active output. A host without the
        // Logitech driver (Android) ignores the vendor HID++ interface, so the
        // gesture button is dead there. On Android, map it to Alt+Tab (the
        // recent-apps switcher) with HOLD semantics: pressing the gesture button
        // opens the switcher and holds Alt so the overlay stays; releasing the
        // button releases Alt and commits the selection. Routed via route_kbd so
        // it reaches the active output whether this board is active or forwards
        // to the peer. The raw HID++ still forwards below (Android ignores it).
        if is_input {
            use crate::domain::hidpp_keymap::{self, GestureEdge, GesturePress,
                MOD_LEFT_ALT, KEY_TAB, APP_DRAWER_USAGE};
            let active_os = dev.cfg.config.output[dev.cfg.active_output as usize].os;
            let fi_reprog = pt.hidpp_disc.fi_reprog_controls;
            match hidpp_keymap::on_hidpp_event(report, fi_reprog, active_os, &mut pt.gesture_pressed) {
                GestureEdge::Pressed => {
                    // Defer the action to release: the held duration decides
                    // tap (Alt+Tab) vs hold (app drawer). Nothing is emitted now,
                    // so a long press never flashes the switcher.
                    pt.gesture_press_us = hal.now_us_64();
                }
                GestureEdge::Released => {
                    // Guard against a release with no matching Android press
                    // (e.g. the press landed before an OS switch).
                    if pt.gesture_press_us != 0 {
                        let elapsed = hal.now_us_64().wrapping_sub(pt.gesture_press_us);
                        pt.gesture_press_us = 0;
                        match hidpp_keymap::classify_press(elapsed) {
                            GesturePress::Tap => {
                                // One-shot Alt+Tab -> toggle to the previous app.
                                hal.route_kbd(dev, &[MOD_LEFT_ALT, 0, KEY_TAB, 0, 0, 0, 0, 0]);
                                hal.route_kbd(dev, &[0u8; 8]);
                                crate::service::dlog::i(b"pt").s(b"remap gesture tap -> Alt+Tab").done();
                            }
                            GesturePress::Hold => {
                                // One-shot consumer 0x1A2 -> open the all-apps drawer
                                // (persistent overlay, touch-selectable).
                                hal.route_consumer(dev, &hidpp_keymap::consumer_report(APP_DRAWER_USAGE));
                                hal.route_consumer(dev, &hidpp_keymap::consumer_report(0));
                                crate::service::dlog::i(b"pt").s(b"remap gesture hold -> app drawer").done();
                            }
                        }
                    }
                }
                GestureEdge::None => {}
            }
        }

        let is_active = dev.is_active_output();

        if !is_input || is_active {
            // Protocol responses always forward; input events only on active output.
            // Queue for the Core0 sender — never call the device stack from this
            // Core1 callback (it races tud_task and hangs Core1).
            hal.queue_hid_report(dev_inst, 0, report);
        } else {
            // Inactive output: convert HID++ → mouse and route to the PEER board
            // (the active output). route_mouse sends a UART MouseReport packet
            // when this board is inactive — previously this used push_mouse_report,
            // which always queues locally, so the wheel / HID++ extra buttons
            // landed on THIS (inactive) host instead of the active one.
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
                hal.route_mouse(dev, &report_bytes);
            } else if converted {
                mouse.mode = 1; // RELATIVE
                let report_bytes = mouse_report_to_bytes(&mouse);
                hal.route_mouse(dev, &report_bytes);
            }
        }

        hal.receive_report(dev_addr, instance);
        return ReportAction::Handled;
    }

    // Non-vendor, non-keyboard: raw passthrough on active output only.
    // Log mouse button transitions on the receiver board regardless of active
    // output (pointer movement suppressed) so the [pt] btn trace is consistent.
    log_button_change(pt, report, hal.now_us_64());

    let is_active = dev.is_active_output();
    if is_active {
        // Queue for the Core0 sender (cross-core safe) instead of touching the
        // device stack from this Core1 callback.
        hal.queue_hid_report(dev_inst, 0, report);
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

/// Pointer summary flush interval (1s — slower than the wheels, very chatty).
const PTR_SUMMARY_US: u64 = 1_000_000;

/// Optional pointer-stream logging, toggled at runtime by the `ptr` CDC command
/// (default off — pointer movement is the noisiest stream). Written from the
/// Core0 USB command handler, read from the Core1 receiver; a bool load/store is
/// atomic on the bus.
static mut DBG_PTR_LOG: bool = false;

fn dbg_ptr_log_enabled() -> bool {
    unsafe { core::ptr::read(core::ptr::addr_of!(DBG_PTR_LOG)) }
}

/// Toggle pointer-stream logging. Called from tud_cdc_rx_cb on the `ptr` command.
///
/// # Safety
/// Plain bool store; safe from the USB task.
#[no_mangle]
pub unsafe extern "C" fn rust_dbg_toggle_ptr_log() {
    let new = !dbg_ptr_log_enabled();
    core::ptr::write(core::ptr::addr_of_mut!(DBG_PTR_LOG), new);
    crate::service::dlog::i(b"pt")
        .s(b"ptr log ").s(if new { b"on" } else { b"off" }).done();
}

/// Sign-extend a 12-bit value (standard packed mouse X/Y field).
fn sext12(v: u16) -> i32 {
    let v = (v & 0x0FFF) as i32;
    if v & 0x0800 != 0 { v - 0x1000 } else { v }
}

/// Emit `[pt] ptr n=<count> dx=<signed> dy=<signed>` (decimal, signed).
fn log_ptr_summary(n: u16, dx: i32, dy: i32) {
    let mut log = crate::service::dlog::i(b"pt");
    log.s(b"ptr n=").u(n as u32).s(b" dx=");
    if dx < 0 { log.s(b"-").u((-dx) as u32); } else { log.u(dx as u32); }
    log.s(b" dy=");
    if dy < 0 { log.s(b"-").u((-dy) as u32); } else { log.u(dy as u32); }
    log.done();
}

/// Log a mouse button transition (DH_DEBUG observability). The button byte is
/// logged only on change so continuous movement doesn't flood the ring. When the
/// `ptr` command has enabled pointer logging, also accumulate the (12-bit packed)
/// X/Y deltas and flush a rate-limited summary at most once per PTR_SUMMARY_US.
fn log_button_change(pt: &mut PassthroughState, report: &[u8], now_us: u64) {
    if report.len() < 2 {
        return;
    }
    let buttons = report[1];
    if buttons != pt.dbg_last_buttons {
        pt.dbg_last_buttons = buttons;
        crate::service::dlog::i(b"pt").s(b"btn=0x").hx(buttons).done();
    }

    // Optional pointer summary. report[3..6] = packed 12-bit X,Y (standard mouse
    // layout); for devices with a different layout the count is still correct.
    if dbg_ptr_log_enabled() && report.len() >= 6 {
        let x = sext12(report[3] as u16 | ((report[4] as u16 & 0x0F) << 8));
        let y = sext12(((report[4] as u16) >> 4) | ((report[5] as u16) << 4));
        let s = &mut pt.dbg_stream;
        if s.ptr_last_us == 0 {
            s.ptr_last_us = now_us;
        }
        s.ptr_dx += x;
        s.ptr_dy += y;
        s.ptr_n += 1;
        if now_us.wrapping_sub(s.ptr_last_us) >= PTR_SUMMARY_US {
            let (n, dx, dy) = (s.ptr_n, s.ptr_dx, s.ptr_dy);
            s.ptr_n = 0;
            s.ptr_dx = 0;
            s.ptr_dy = 0;
            s.ptr_last_us = now_us;
            log_ptr_summary(n, dx, dy);
        }
    }
}

/// Flush interval for the rate-limited wheel/thumbwheel summaries (300ms).
const WHEEL_SUMMARY_US: u64 = 300_000;

/// Emit a `[pt] <tag> n=<count> sum=<signed>` summary line (decimal, signed).
fn log_stream_summary(tag: &[u8], n: u16, sum: i32) {
    let mut log = crate::service::dlog::i(b"pt");
    log.s(tag).s(b" n=").u(n as u32).s(b" sum=");
    if sum < 0 {
        log.s(b"-").u((-sum) as u32);
    } else {
        log.u(sum as u32);
    }
    log.done();
}

/// Log HID++ control events (DH_DEBUG). Discrete keys (ReprogControls
/// cid/action) log immediately as `[pt] key fi=XX fn=XX c=XX a=XX`. The
/// high-frequency vertical (HiResWheel) and horizontal (thumbwheel) streams are
/// NOT logged per event (that would flood the peer_log ring) — instead their
/// delta is accumulated and flushed at most once per WHEEL_SUMMARY_US as
/// `[pt] scroll/thumb n=<count> sum=<delta>`.
fn log_hidpp_event(pt: &mut PassthroughState, report: &[u8], now_us: u64) {
    if report.len() < 7 {
        return;
    }
    let fi = report[2];
    let (hires, thumb) = (pt.hidpp_disc.fi_hires_scroll, pt.hidpp_disc.fi_thumbwheel);
    // HID++ wheel delta = params[1..3] big-endian = report[5..7].
    let delta = (((report[5] as i16) << 8) | report[6] as i16) as i32;

    if hires != 0 && fi == hires {
        let s = &mut pt.dbg_stream;
        if s.wheel_last_us == 0 {
            s.wheel_last_us = now_us;
        }
        s.wheel_sum += delta;
        s.wheel_n += 1;
        if now_us.wrapping_sub(s.wheel_last_us) >= WHEEL_SUMMARY_US {
            let (n, sum) = (s.wheel_n, s.wheel_sum);
            s.wheel_n = 0;
            s.wheel_sum = 0;
            s.wheel_last_us = now_us;
            log_stream_summary(b"scroll", n, sum);
        }
        return;
    }

    if thumb != 0 && fi == thumb {
        let s = &mut pt.dbg_stream;
        if s.thumb_last_us == 0 {
            s.thumb_last_us = now_us;
        }
        s.thumb_sum += delta;
        s.thumb_n += 1;
        if now_us.wrapping_sub(s.thumb_last_us) >= WHEEL_SUMMARY_US {
            let (n, sum) = (s.thumb_n, s.thumb_sum);
            s.thumb_n = 0;
            s.thumb_sum = 0;
            s.thumb_last_us = now_us;
            log_stream_summary(b"thumb", n, sum);
        }
        return;
    }

    // ReprogControls divertedRawXYEvent (fn=1) floods at ~70/s while a diverted
    // button (e.g. the gesture/thumb button) is held. Its layout is known (long
    // report, big-endian dx@[4..6], dy@[6..8]) and unused outside the remap, so
    // suppress the per-event spam to keep the peer_log ring readable.
    if fi != 0 && fi == pt.hidpp_disc.fi_reprog_controls && (report[3] >> 4) & 0x0F == 1 {
        return;
    }

    crate::service::dlog::i(b"pt")
        .s(b"key fi=").hx(report[2])
        .s(b" fn=").hx(report[3])
        .s(b" c=").hx(report[5])
        .s(b" a=").hx(report[6])
        .done();
}

/// Forward all events buffered by the SmartShift double-click state machine
/// to the host in capture order, then clear the buffer count. Caller is
/// responsible for resetting smartshift_window_us.
fn flush_smartshift_buffer(pt: &mut PassthroughState, hal: &impl HidQueue) {
    let count = pt.smartshift_buf_count as usize;
    for i in 0..count {
        let entry = &pt.smartshift_buf[i];
        let len = entry.len as usize;
        hal.queue_hid_report(entry.dev_inst, 0, &entry.data[..len]);
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
    fn task_no_activate_when_build_config_desc_fails() {
        // Models the C-side HID-budget guard rejecting an over-large composite
        // (more HID interfaces than CFG_TUD_HID). build_config_desc() returns
        // false, so we must NOT activate and must NOT disconnect — otherwise the
        // device would drop into LED_BLINK_PT_WAIT forever with no way back.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        cfg.config.passthrough_enabled = 1;
        hal.pt_build_fails.set(true);
        let desc = [0x05, 0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &desc);
        pt.last_capture_us = 1000;
        hal.set_time(1000 + CAPTURE_STABILIZE_US + 1);

        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!pt.active, "must not activate when the composite cannot be built");
        assert_eq!(hal.device_disconnect_count.get(), 0, "must not disconnect");
        assert_eq!(pt.reconnect_at_us, 0, "no reconnect cycle scheduled");
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
    fn flush_output_report_sends_and_clears() {
        // flush_output_report runs on Core1 (not passthrough_task on Core0) to
        // avoid racing the host stack. It must send the pending report and clear
        // the pending flag.
        let (mut pt, _hid, _cfg, _fw, _led, hal) = setup();
        pt.out_queue.pending = true;
        pt.out_queue.data[0] = 0xAA;
        pt.out_queue.len = 1;
        flush_output_report(&mut pt, &hal);
        assert!(!pt.out_queue.pending);
        assert_eq!(hal.host_set_report_count.get(), 1);
    }

    #[test]
    fn passthrough_task_does_not_send_set_report() {
        // The Core0 task must NOT issue the host SET_REPORT — that is the bug
        // this split fixes (cross-core host-stack access hangs Core1).
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        pt.out_queue.pending = true;
        pt.out_queue.len = 1;
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert_eq!(hal.host_set_report_count.get(), 0, "Core0 task must not send to host");
        assert!(pt.out_queue.pending, "out_queue stays pending for Core1 to flush");
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
    fn unmount_requests_disconnect_when_empty() {
        // on_device_unmount runs on Core1 and must NOT touch the device stack:
        // it only flags disconnect_requested; passthrough_task (Core0) performs
        // the actual tud_disconnect.
        let (mut pt, _hid, _cfg, _fw, _led, _hal) = setup();
        let desc = [0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 1, &desc);
        pt.active = true;

        on_device_unmount(&mut pt, 1);
        assert!(!pt.active);
        assert!(pt.disconnect_requested, "must flag disconnect for Core0");
    }

    #[test]
    fn unmount_partial_no_disconnect() {
        let (mut pt, _hid, _cfg, _fw, _led, _hal) = setup();
        let desc = [0x01];
        passthrough::capture_descriptor(&mut pt, 1, 0, 1, &desc);
        passthrough::capture_descriptor(&mut pt, 2, 0, 1, &desc);
        pt.active = true;

        on_device_unmount(&mut pt, 1);
        assert!(pt.active);
        assert!(!pt.disconnect_requested);
    }

    #[test]
    fn task_performs_deferred_disconnect_on_core0() {
        // passthrough_task drains the disconnect_requested flag and performs the
        // device-stack disconnect + schedules reconnect (all on Core0).
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        pt.disconnect_requested = true;
        hal.set_time(10000);
        passthrough_task(&mut pt, &mut dev!(hid, cfg, fw, led), &hal);
        assert!(!pt.disconnect_requested);
        assert_eq!(hal.device_disconnect_count.get(), 1);
        assert!(!hal.pt_config_desc_ready.get());
        assert_eq!(pt.reconnect_at_us, 10000 + RECONNECT_DELAY_US);
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

    /// divertedButtonsEvent (fn=0) form: a bitmap of pressed CIDs. SmartShift
    /// (0xC4) present = down, absent (0x00) = up. This is what an MX Master
    /// actually emits for the wheel-mode key.
    fn smartshift_diverted(pressed: bool) -> [u8; 7] {
        [
            HIDPP_REPORT_ID_SHORT,
            0x01,         // device_idx
            0x05,         // feature_idx (ReprogControls)
            0x00,         // (fn=0 << 4) | sw_id=0  → divertedButtonsEvent
            0x00,         // cid_hi
            if pressed { passthrough::SMARTSHIFT_CID } else { 0x00 }, // cid_lo (bitmap)
            0x00,
        ]
    }

    fn ss_setup() -> (PassthroughState, DeviceHid, DeviceConfig, DeviceFw, DeviceLed, MockHal) {
        let mut t = setup();
        // Vendor iface (always_passthrough = true via itf_protocol = 0)
        passthrough::capture_descriptor(&mut t.0, 1, 0, 0, &[0x06, 0x00, 0xFF]);
        t.0.active = true;
        // Active passthrough operating state: device side has re-mounted, so
        // the re-enumeration drop-gate in on_report_received is not engaged.
        t.2.tud_connected = true;
        t.2.config.smartshift_double_click_ms = 350;
        t
    }

    #[test]
    fn report_dropped_during_reenumeration() {
        // pt.active but device side mid-re-enumeration (tud_connected=false):
        // host reports must be dropped (not forwarded to the device stack) to
        // avoid the Core1→Core0 device-stack race that hangs Core1.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = setup();
        passthrough::capture_descriptor(&mut pt, 1, 0, 0, &[0x06, 0x00, 0xFF]);
        pt.active = true;
        cfg.tud_connected = false;
        let report = [0x10, 0x01, 0x02, 0x00, 0x01, 0x00, 0x00];
        let action = on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                                        &report, 1, 0, &hal);
        assert!(matches!(action, ReportAction::Handled), "report must be consumed, not forwarded");
        assert_eq!(hal.hid_sent.borrow().len(), 0, "nothing forwarded to device while re-enumerating");
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
        let release = smartshift_event(false);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        // Up1 — required to arm the second press as a double-click.
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &release, 1, 0, &hal);
        // 100ms later, second press completes Down1->Up1->Down2.
        hal.set_time(1_100_000);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        assert!(cfg.switch_requested, "Down1->Up1->Down2 within window must trigger switch");
        assert_eq!(pt.smartshift_window_us, 0, "window must clear");
        assert_eq!(pt.smartshift_buf_count, 0, "buffer must clear");
        assert_eq!(pt.smartshift_consume, 1, "release of second press must be marked for consumption");
        assert_eq!(hal.hid_sent.borrow().len(), 0, "no events forwarded on switch trigger");
    }

    #[test]
    fn smartshift_diverted_double_click_triggers_switch() {
        // MX Master emits the wheel-mode key as divertedButtonsEvent (fn=0),
        // not analyticsKeyEvent (fn=2). The double-click must still switch.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        // press1 (0xC4 in bitmap) then release1 (empty bitmap)
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &smartshift_diverted(true), 1, 0, &hal);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &smartshift_diverted(false), 1, 0, &hal);
        assert!(pt.smartshift_window_us > 0, "first diverted press opens the window");
        // press2 within window
        hal.set_time(1_100_000);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &smartshift_diverted(true), 1, 0, &hal);
        assert!(cfg.switch_requested, "diverted-button double-click must switch output");
        assert_eq!(pt.smartshift_window_us, 0, "window cleared");
        // release2 (empty bitmap) is consumed, not forwarded
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &smartshift_diverted(false), 1, 0, &hal);
        assert_eq!(pt.smartshift_consume, 0, "post-switch release consumed");
        assert_eq!(hal.hid_queued.borrow().len(), 0, "nothing forwarded for the double-click");
    }

    #[test]
    fn smartshift_two_presses_without_release_do_not_switch() {
        // A single physical actuation can surface as two SmartShift "Down" reports
        // with no Up in between (a held divertedButtons bitmap re-sent, or dual
        // fn=2+fn=0 emission for one click). Without an intervening release this is
        // NOT a double-click and must not switch the output. Regression for the
        // single-click-switches bug exposed once fn=0 detection was added (c4224bb).
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        let press = smartshift_event(true);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        hal.set_time(1_010_000);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &press, 1, 0, &hal);
        assert!(!cfg.switch_requested, "two Downs with no Up between must not switch");
        assert!(pt.smartshift_window_us > 0, "window stays armed, awaiting a real Up->Down");
        assert!(!pt.smartshift_saw_release, "no release observed");
    }

    #[test]
    fn smartshift_dual_shape_single_click_does_not_switch() {
        // An MX Master may report one physical wheel-mode click as BOTH an fn=0
        // divertedButtons frame and an fn=2 analyticsKeyEvent. Two Downs, no Up
        // between → not a double-click.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        hal.set_time(1_000_000);
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &smartshift_diverted(true), 1, 0, &hal);   // fn=0 Down
        on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                           &smartshift_event(true), 1, 0, &hal);      // fn=2 Down, same click
        assert!(!cfg.switch_requested, "dual-shape single click must not switch");
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
        // Flush routes through the cross-core queue (queue_hid_report), not a
        // direct device send, so it lands in hid_queued.
        assert_eq!(hal.hid_queued.borrow().len(), 1, "buffered press must be flushed");
        assert_eq!(hal.hid_queued.borrow()[0].2, press.to_vec());
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
        assert_eq!(hal.hid_queued.borrow().len(), 1, "first press flushed before new window");
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

    #[test]
    fn active_forward_routes_through_queue_not_device_stack() {
        // The active-output passthrough forward must route through queue_hid_report
        // (drained by Core0), NOT call send_hid_report (device stack) from this
        // Core1 callback — the latter races tud_task and hangs Core1.
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        cfg.tud_connected = true; // not re-enumerating
        // is_active_output() is true (active_output == board_role == 0).
        // A non-SmartShift HID++ input event forwards immediately.
        let report = [HIDPP_REPORT_ID_SHORT, 0x01, 0x06, 0x00, 0x00, 0x05, 0x00];
        let action = on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                                        &report, 1, 0, &hal);
        assert!(matches!(action, ReportAction::Handled));
        assert_eq!(hal.hid_sent.borrow().len(), 0, "must NOT touch the device stack from Core1");
        assert_eq!(hal.hid_queued.borrow().len(), 1, "must route via the cross-core queue");
    }

    #[test]
    fn inactive_input_routes_converted_mouse_to_peer() {
        // When the OTHER board is the active output, a converted HID++ input event
        // (here a diverted Left-button press, CID 0x50 → button bit 0x01) must be
        // sent to the PEER over UART, NOT pushed to the local mouse queue. This is
        // the wheel / HID++ extra-button misroute fix: route_mouse replaces the old
        // push_mouse_report (which always queued locally on the inactive board).
        let (mut pt, mut hid, mut cfg, mut fw, mut led, hal) = ss_setup();
        pt.hidpp_disc.fi_reprog_controls = 0x05;
        cfg.tud_connected = true;
        cfg.board_role = 0;
        cfg.active_output = 1; // peer is active → is_active_output() == false
        // divertedButtonsEvent (fn=0) carrying CID 0x50 (Left) in the bitmap.
        let report = [HIDPP_REPORT_ID_SHORT, 0x01, 0x05, 0x00, 0x00, 0x50, 0x00];
        let action = on_report_received(&mut pt, &mut dev!(hid, cfg, fw, led),
                                        &report, 1, 0, &hal);
        assert!(matches!(action, ReportAction::Handled));
        assert_eq!(hal.mouse_reports.borrow().len(), 0, "must not push locally when inactive");
        let pkts = hal.sent_packets.borrow();
        assert_eq!(pkts.len(), 1, "converted input must be sent to the peer board");
        assert_eq!(pkts[0].1, crate::domain::constants::PacketType::MouseReport as u8,
            "routed as a mouse report packet");
        assert_eq!(pkts[0].0[0], 0x01, "Left button bit set in the routed report");
    }
}
