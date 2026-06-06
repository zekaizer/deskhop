//! HID++ control-ID -> standard-HID remapping for outputs that don't speak
//! HID++.
//!
//! On the active output a captured HID++ report is forwarded raw on the vendor
//! interface. A host with the Logitech driver (Options+) interprets it, but a
//! host without it (Android) ignores the vendor interface entirely — so the MX
//! gesture/thumb button (Control-ID 0xC3, which `cid_to_button_bit` doesn't map
//! to a standard mouse button) does nothing there.
//!
//! This translates that button, per the ACTIVE output's configured OS, into a
//! standard key the OS understands. Currently: when the active output is
//! Android, the gesture button emits a Consumer Control "AC Select
//! Task/Application" tap (HID usage 0x01A2) — which the Linux HID driver maps to
//! KEY_APPSELECT and Android surfaces as APP_SWITCH (the recent-apps overview).
//! A single tap opens the (persistent) overview; no key needs to be held.
//!
//! Pure logic — emission/routing live in service::passthrough_service.

use crate::domain::constants::OS_ANDROID;
use crate::domain::hid_routing::CONSUMER_CONTROL_LENGTH;
use crate::domain::passthrough;

/// MX gesture/thumb button — Logitech HID++ Control-ID (low byte).
pub const GESTURE_CID: u8 = 0xC3;

/// Consumer page "AC Select Task/Application" -> KEY_APPSELECT -> Android
/// APP_SWITCH (recent-apps overview).
pub const AC_APP_SWITCH: u16 = 0x01A2;

/// Build a Consumer Control report carrying `usage` (16-bit little-endian,
/// NUL-padded). `usage == 0` is the release report.
pub fn consumer_report(usage: u16) -> [u8; CONSUMER_CONTROL_LENGTH] {
    let mut r = [0u8; CONSUMER_CONTROL_LENGTH];
    r[0] = (usage & 0xFF) as u8;
    r[1] = (usage >> 8) as u8;
    r
}

/// Inspect a HID++ input event and, on the gesture button's press edge, return
/// the Consumer Control usage to tap for the given active-output OS.
///
/// Only the ReprogControls divertedButtonsEvent (fn=0) on the learned feature
/// index carries the gesture button as a CID bitmap. `prev_pressed` tracks the
/// button's last state so we tap once per press (not on repeats or release).
/// Returns `None` until the feature index is learned (a normal click learns it).
pub fn on_hidpp_event(
    report: &[u8],
    fi_reprog: u8,
    active_os: u8,
    prev_pressed: &mut bool,
) -> Option<u16> {
    // Require a learned ReprogControls feature + a divertedButtonsEvent (fn=0)
    // on it, so we don't confuse the wheel/thumbwheel fn=0 streams (other fi).
    if report.len() < 6
        || fi_reprog == 0
        || report[2] != fi_reprog
        || (report[3] >> 4) & 0x0F != 0
    {
        return None;
    }

    let pressed = passthrough::diverted_has_cid(report, GESTURE_CID);
    let press_edge = pressed && !*prev_pressed;
    *prev_pressed = pressed;

    if press_edge && active_os == OS_ANDROID {
        Some(AC_APP_SWITCH)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::constants::{OS_ANDROID, OS_LINUX};

    const FI: u8 = 0x09;
    // Short HID++ report: [rid, dev, fi, fn|sw, cid_hi, cid_lo, ...]
    fn diverted(fi: u8, cid_lo: u8) -> [u8; 7] {
        [0x10, 0x01, fi, 0x00, 0x00, cid_lo, 0x00]
    }

    #[test]
    fn consumer_report_is_le_padded() {
        assert_eq!(consumer_report(0x01A2), [0xA2, 0x01, 0x00, 0x00]);
        assert_eq!(consumer_report(0), [0, 0, 0, 0]);
    }

    #[test]
    fn gesture_press_on_android_taps_app_switch() {
        let mut prev = false;
        let r = diverted(FI, GESTURE_CID);
        assert_eq!(on_hidpp_event(&r, FI, OS_ANDROID, &mut prev), Some(AC_APP_SWITCH));
        assert!(prev);
    }

    #[test]
    fn only_taps_on_press_edge_not_repeat() {
        let mut prev = false;
        let down = diverted(FI, GESTURE_CID);
        let up = diverted(FI, 0x00);
        assert_eq!(on_hidpp_event(&down, FI, OS_ANDROID, &mut prev), Some(AC_APP_SWITCH));
        // Repeat of the down report (still pressed) does not re-tap.
        assert_eq!(on_hidpp_event(&down, FI, OS_ANDROID, &mut prev), None);
        // Release, then a fresh press taps again.
        assert_eq!(on_hidpp_event(&up, FI, OS_ANDROID, &mut prev), None);
        assert_eq!(on_hidpp_event(&down, FI, OS_ANDROID, &mut prev), Some(AC_APP_SWITCH));
    }

    #[test]
    fn no_remap_on_non_android() {
        let mut prev = false;
        let r = diverted(FI, GESTURE_CID);
        assert_eq!(on_hidpp_event(&r, FI, OS_LINUX, &mut prev), None);
        // State still tracked so a later OS change behaves correctly.
        assert!(prev);
    }

    #[test]
    fn ignores_other_feature_index() {
        let mut prev = false;
        // Same bitmap but on a different fi (e.g. thumbwheel) — must not match.
        let r = diverted(0x08, GESTURE_CID);
        assert_eq!(on_hidpp_event(&r, FI, OS_ANDROID, &mut prev), None);
    }

    #[test]
    fn unlearned_feature_does_nothing() {
        let mut prev = false;
        let r = diverted(FI, GESTURE_CID);
        assert_eq!(on_hidpp_event(&r, 0, OS_ANDROID, &mut prev), None);
    }
}
