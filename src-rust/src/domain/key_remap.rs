// Key Remap Engine — SIMPLE and TAP_HOLD key remapping.
// Pure domain logic, no HAL dependencies.

use crate::domain::constants::{OS_MACOS, OS_ANDROID, HID_KEY_CAPS_LOCK, HID_KEY_SPACE,
    KEYBOARD_MODIFIER_LEFTSHIFT};
use crate::domain::keyboard::key_in_report;
use crate::domain::structs::HidKeyboardReport;

/// HID usage code for LANG1 (한/영 toggle)
pub const HID_KEY_LANG1: u8 = 0x90;

// ================================================================
// Constants
// ================================================================

pub const MAX_REMAP_ENTRIES: usize = 16;
pub const TAP_HOLD_DEFAULT_US: u64 = 350_000;

// ================================================================
// Types
// ================================================================

#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RemapType {
    #[default]
    Simple = 0,
    TapHold = 1,
}

#[derive(Clone, Copy, Default)]
pub struct KeyAction {
    pub keycode: u8,
    pub modifier: u8,
}

/// Single remapping rule. Uses flattened struct instead of C union
/// (both simple and tap_hold fields always present, type selects which is valid).
#[derive(Clone, Copy, Default)]
pub struct RemapEntry {
    pub trigger: u8,
    pub remap_type: RemapType,
    pub output_mask: u8, // bit0=Output A, bit1=Output B, 0xFF=all

    // Simple fields
    pub simple_replacement: KeyAction,

    // TapHold fields
    pub tap_action: KeyAction,
    pub hold_action: KeyAction,
    pub threshold_us: u64,
}

#[derive(Clone, Copy, Default)]
pub struct RemapConfig {
    pub entries: [RemapEntry; MAX_REMAP_ENTRIES],
    pub count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RemapState {
    #[default]
    Idle = 0,
    Waiting = 1,
    Held = 2,
}

#[derive(Clone, Copy, Default)]
pub struct RemapRuntime {
    pub state: RemapState,
    pub timestamp: u64,
    pub consumed: bool,
}

#[derive(Clone, Copy, Default)]
pub struct RemapEngine {
    pub config: RemapConfig,
    pub runtime: [RemapRuntime; MAX_REMAP_ENTRIES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapResult {
    Pass,
    Modified,
}

// ================================================================
// Report helpers
// ================================================================

fn report_replace_key(report: &mut HidKeyboardReport, old_key: u8, new_key: u8) -> bool {
    for k in report.keycode.iter_mut() {
        if *k == old_key {
            *k = new_key;
            return true;
        }
    }
    false
}

fn report_remove_key(report: &mut HidKeyboardReport, keycode: u8) {
    for k in report.keycode.iter_mut() {
        if *k == keycode {
            *k = 0;
            return;
        }
    }
}

// ================================================================
// Engine functions
// ================================================================

/// Initialize remap engine with OS-aware defaults.
/// - macOS: skip (native CapsLock → 한영)
/// - Android: CapsLock tap → LeftShift+Space / hold → CapsLock
/// - Linux/Windows: CapsLock tap → LANG1 / hold → CapsLock
pub fn remap_engine_init(engine: &mut RemapEngine, os_a: u8, os_b: u8) {
    engine.runtime = [RemapRuntime::default(); MAX_REMAP_ENTRIES];
    engine.config.count = 0;

    // Build per-OS-group output masks in a single pass
    let mut lang1_mask: u8 = 0;
    let mut android_mask: u8 = 0;
    for (i, &os) in [os_a, os_b].iter().enumerate() {
        match os {
            OS_MACOS => {}
            OS_ANDROID => android_mask |= 1 << i,
            _ => lang1_mask |= 1 << i,
        }
    }

    // Shared base for CapsLock TAP_HOLD entries
    let base = RemapEntry {
        trigger: HID_KEY_CAPS_LOCK,
        remap_type: RemapType::TapHold,
        hold_action: KeyAction { keycode: HID_KEY_CAPS_LOCK, modifier: 0 },
        threshold_us: TAP_HOLD_DEFAULT_US,
        ..RemapEntry::default()
    };

    // Linux/Windows: CapsLock tap → LANG1
    if lang1_mask != 0 {
        let idx = engine.config.count as usize;
        engine.config.entries[idx] = RemapEntry {
            output_mask: lang1_mask,
            tap_action: KeyAction { keycode: HID_KEY_LANG1, modifier: 0 },
            ..base
        };
        engine.config.count += 1;
    }

    // Android: CapsLock tap → LeftShift+Space
    if android_mask != 0 {
        let idx = engine.config.count as usize;
        engine.config.entries[idx] = RemapEntry {
            output_mask: android_mask,
            tap_action: KeyAction { keycode: HID_KEY_SPACE, modifier: KEYBOARD_MODIFIER_LEFTSHIFT },
            ..base
        };
        engine.config.count += 1;
    }
}

/// Process a keyboard report through the remap engine.
/// Modifies the report in-place. Returns Pass if unchanged, Modified if altered.
/// `now_us` stamps the press time for TAP_HOLD entries so remap_engine_tick can
/// detect the hold threshold; without it the Waiting→Held transition never fires.
pub fn remap_engine_process(
    engine: &mut RemapEngine,
    report: &mut HidKeyboardReport,
    active_output: u8,
    now_us: u64,
) -> RemapResult {
    if engine.config.count == 0 {
        return RemapResult::Pass;
    }

    let mut result = RemapResult::Pass;

    for i in 0..engine.config.count as usize {
        let e = &engine.config.entries[i];

        if e.output_mask != 0xFF && (e.output_mask & (1 << active_output)) == 0 {
            // Entry inactive on this output. A tap-hold that began before an
            // output switch must still be cancelled here: remap_engine_tick
            // ignores output_mask, so a stranded Waiting entry would ripen and
            // get_active_output would inject the hold key into the WRONG
            // output, and the next press back on the original output would
            // land in a stale Held branch and swallow the tap.
            if e.remap_type == RemapType::TapHold {
                let r = &mut engine.runtime[i];
                if r.state != RemapState::Idle {
                    r.state = RemapState::Idle;
                    r.timestamp = 0;
                    r.consumed = false; // no tap into the wrong output
                    if key_in_report(e.trigger, report) {
                        // Consume the in-flight press once; after the cancel
                        // the trigger acts natively on this output.
                        report_remove_key(report, e.trigger);
                        result = RemapResult::Modified;
                    }
                }
            }
            continue;
        }

        let key_pressed = key_in_report(e.trigger, report);

        match e.remap_type {
            RemapType::Simple => {
                if key_pressed {
                    report_replace_key(report, e.trigger, e.simple_replacement.keycode);
                    report.modifier |= e.simple_replacement.modifier;
                    result = RemapResult::Modified;
                }
            }
            RemapType::TapHold => {
                let r = &mut engine.runtime[i];

                if key_pressed && r.state == RemapState::Idle {
                    // Key just pressed: start waiting, stamp the press time so
                    // remap_engine_tick can fire the hold threshold.
                    r.state = RemapState::Waiting;
                    r.timestamp = now_us;
                    report_remove_key(report, e.trigger);
                    result = RemapResult::Modified;
                } else if key_pressed && r.state == RemapState::Waiting {
                    // Still held, keep consuming
                    report_remove_key(report, e.trigger);
                    result = RemapResult::Modified;
                } else if key_pressed && r.state == RemapState::Held {
                    // Remove trigger — hold key injected via get_active_output
                    report_remove_key(report, e.trigger);
                    result = RemapResult::Modified;
                } else if !key_pressed && r.state == RemapState::Waiting {
                    // Released before threshold: emit tap
                    r.state = RemapState::Idle;
                    r.consumed = true;
                    result = RemapResult::Modified;
                } else if !key_pressed && r.state == RemapState::Held {
                    // Released after hold
                    r.state = RemapState::Idle;
                    result = RemapResult::Modified;
                }
            }
        }
    }

    result
}

/// Tick the remap engine timer. Call periodically with current time.
/// Returns true if any state transition occurred (RS_WAITING → RS_HELD).
pub fn remap_engine_tick(engine: &mut RemapEngine, now_us: u64) -> bool {
    if engine.config.count == 0 {
        return false;
    }

    let mut state_changed = false;

    for i in 0..engine.config.count as usize {
        let e = &engine.config.entries[i];
        let r = &mut engine.runtime[i];

        if e.remap_type != RemapType::TapHold {
            continue;
        }

        if r.state == RemapState::Waiting && r.timestamp > 0 {
            let threshold = if e.threshold_us == 0 { TAP_HOLD_DEFAULT_US } else { e.threshold_us };
            if now_us.wrapping_sub(r.timestamp) >= threshold {
                r.state = RemapState::Held;
                state_changed = true;
            }
        }
    }

    state_changed
}

/// Get pending tap action if a TAP_HOLD entry was released before threshold.
/// Clears the consumed flag. Returns true if a tap report was generated.
pub fn remap_engine_get_pending(
    engine: &mut RemapEngine,
    out: &mut HidKeyboardReport,
) -> bool {
    if engine.config.count == 0 {
        return false;
    }
    for i in 0..engine.config.count as usize {
        let r = &mut engine.runtime[i];
        if r.consumed {
            r.consumed = false;
            let e = &engine.config.entries[i];
            if e.remap_type == RemapType::TapHold {
                *out = HidKeyboardReport::default();
                out.keycode[0] = e.tap_action.keycode;
                out.modifier = e.tap_action.modifier;
                return true;
            }
        }
    }
    false
}

/// Get combined output of all active hold keys (RS_HELD entries).
pub fn remap_engine_get_active_output(
    engine: &RemapEngine,
    out: &mut HidKeyboardReport,
) {
    *out = HidKeyboardReport::default();
    if engine.config.count == 0 {
        return;
    }

    for i in 0..engine.config.count as usize {
        let e = &engine.config.entries[i];
        let r = &engine.runtime[i];

        if e.remap_type == RemapType::TapHold && r.state == RemapState::Held {
            // Add hold action key to first empty slot
            for k in out.keycode.iter_mut() {
                if *k == 0 {
                    *k = e.hold_action.keycode;
                    break;
                }
            }
            out.modifier |= e.hold_action.modifier;
        }
    }
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn new_engine() -> RemapEngine {
        RemapEngine::default()
    }

    fn make_report(keys: &[u8]) -> HidKeyboardReport {
        let mut r = HidKeyboardReport::default();
        for (i, &k) in keys.iter().enumerate().take(6) {
            r.keycode[i] = k;
        }
        r
    }

    fn make_simple(trigger: u8, replacement: u8, modifier: u8) -> RemapEntry {
        RemapEntry {
            trigger,
            remap_type: RemapType::Simple,
            output_mask: 0xFF,
            simple_replacement: KeyAction { keycode: replacement, modifier },
            ..RemapEntry::default()
        }
    }

    fn make_tap_hold(trigger: u8, tap_key: u8, hold_key: u8) -> RemapEntry {
        RemapEntry {
            trigger,
            remap_type: RemapType::TapHold,
            output_mask: 0xFF,
            tap_action: KeyAction { keycode: tap_key, modifier: 0 },
            hold_action: KeyAction { keycode: hold_key, modifier: 0 },
            threshold_us: TAP_HOLD_DEFAULT_US,
            ..RemapEntry::default()
        }
    }

    // -- Init tests --

    #[test]
    fn init_zeroes_runtime() {
        let mut engine = new_engine();
        engine.runtime[0].state = RemapState::Held;
        remap_engine_init(&mut engine, 1, 3); // Linux, Windows
        assert_eq!(engine.runtime[0].state, RemapState::Idle);
        assert_eq!(engine.config.count, 1); // CapsLock entry
    }

    #[test]
    fn init_macos_both_no_entry() {
        let mut engine = new_engine();
        remap_engine_init(&mut engine, OS_MACOS, OS_MACOS);
        assert_eq!(engine.config.count, 0);
    }

    #[test]
    fn init_macos_one_side() {
        let mut engine = new_engine();
        remap_engine_init(&mut engine, OS_MACOS, 1); // macOS A, Linux B
        assert_eq!(engine.config.count, 1);
        assert_eq!(engine.config.entries[0].output_mask, 0x02); // B only
    }

    #[test]
    fn init_android_uses_shift_space() {
        let mut engine = new_engine();
        remap_engine_init(&mut engine, OS_ANDROID, OS_ANDROID);
        assert_eq!(engine.config.count, 1); // Android entry only
        let e = &engine.config.entries[0];
        assert_eq!(e.tap_action.keycode, HID_KEY_SPACE);
        assert_eq!(e.tap_action.modifier, KEYBOARD_MODIFIER_LEFTSHIFT);
        assert_eq!(e.output_mask, 0x03); // both outputs
    }

    #[test]
    fn init_android_and_linux_two_entries() {
        let mut engine = new_engine();
        remap_engine_init(&mut engine, 1, OS_ANDROID); // Linux A, Android B
        assert_eq!(engine.config.count, 2);
        // Entry 0: LANG1 for Linux (Output A)
        assert_eq!(engine.config.entries[0].tap_action.keycode, HID_KEY_LANG1);
        assert_eq!(engine.config.entries[0].output_mask, 0x01);
        // Entry 1: Shift+Space for Android (Output B)
        assert_eq!(engine.config.entries[1].tap_action.keycode, HID_KEY_SPACE);
        assert_eq!(engine.config.entries[1].tap_action.modifier, KEYBOARD_MODIFIER_LEFTSHIFT);
        assert_eq!(engine.config.entries[1].output_mask, 0x02);
    }

    // -- Output-switch-while-held (masked in-flight cancel) --

    fn make_masked_tap_hold(mask: u8) -> RemapEntry {
        RemapEntry {
            output_mask: mask,
            ..make_tap_hold(0x39, 0x90, 0x39) // CapsLock: tap LANG1, hold CapsLock
        }
    }

    #[test]
    fn output_switch_while_waiting_cancels_entry() {
        // Press on output 0 (masked in), then the output switches to 1 (masked
        // out) with the key still held: the in-flight entry must cancel, not
        // stay Waiting forever (the tick ignores output_mask and would ripen
        // it into a phantom hold on the wrong output).
        let mut engine = new_engine();
        engine.config.entries[0] = make_masked_tap_hold(0x01);
        engine.config.count = 1;

        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(engine.runtime[0].state, RemapState::Waiting);

        // Switched to output 1, key still held
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 1, 2000);
        assert_eq!(engine.runtime[0].state, RemapState::Idle);
        assert!(!engine.runtime[0].consumed); // no tap into the wrong output
        assert_eq!(report.keycode[0], 0); // in-flight press stays consumed

        // The tick must not ripen the cancelled entry into Held
        assert!(!remap_engine_tick(&mut engine, 2000 + TAP_HOLD_DEFAULT_US * 2));
        let mut out = HidKeyboardReport::default();
        remap_engine_get_active_output(&engine, &mut out);
        assert_eq!(out.keycode[0], 0); // no phantom hold injection
    }

    #[test]
    fn output_switch_while_held_cancels_entry() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_masked_tap_hold(0x01);
        engine.config.count = 1;

        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        remap_engine_tick(&mut engine, 1000 + TAP_HOLD_DEFAULT_US + 1);
        assert_eq!(engine.runtime[0].state, RemapState::Held);

        // Switch to masked-out output with the key still held
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 1, 2_000_000);
        assert_eq!(engine.runtime[0].state, RemapState::Idle);

        let mut out = HidKeyboardReport::default();
        remap_engine_get_active_output(&engine, &mut out);
        assert_eq!(out.keycode[0], 0); // hold key no longer injected
    }

    #[test]
    fn tap_works_after_cancelled_switch_roundtrip() {
        // After the cancel, returning to the original output and tapping must
        // emit the tap — the old stale Held state used to swallow it.
        let mut engine = new_engine();
        engine.config.entries[0] = make_masked_tap_hold(0x01);
        engine.config.count = 1;

        // Press on 0, switch to 1 (cancel), release on 1
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 1, 2000);
        let mut report = make_report(&[]);
        remap_engine_process(&mut engine, &mut report, 1, 3000);

        // Back on 0: tap (press + release before threshold)
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 0, 10_000);
        assert_eq!(report.keycode[0], 0); // trigger consumed
        let mut report = make_report(&[]);
        remap_engine_process(&mut engine, &mut report, 0, 20_000);

        let mut out = HidKeyboardReport::default();
        assert!(remap_engine_get_pending(&mut engine, &mut out));
        assert_eq!(out.keycode[0], 0x90); // tap emitted
    }

    #[test]
    fn masked_idle_entry_stays_untouched() {
        // A masked-out entry with no in-flight state must keep passing the
        // raw trigger through (native CapsLock on the other OS).
        let mut engine = new_engine();
        engine.config.entries[0] = make_masked_tap_hold(0x01);
        engine.config.count = 1;

        let mut report = make_report(&[0x39]);
        let result = remap_engine_process(&mut engine, &mut report, 1, 1000);
        assert_eq!(result, RemapResult::Pass);
        assert_eq!(report.keycode[0], 0x39); // untouched
        assert_eq!(engine.runtime[0].state, RemapState::Idle);
    }

    // -- SIMPLE remap tests --

    #[test]
    fn simple_replacement() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_simple(0x04, 0x05, 0); // A → B
        engine.config.count = 1;

        let mut report = make_report(&[0x04]);
        let result = remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(result, RemapResult::Modified);
        assert_eq!(report.keycode[0], 0x05);
    }

    #[test]
    fn simple_modifier_injection() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_simple(0x04, 0x04, 0x01); // A + LCtrl
        engine.config.count = 1;

        let mut report = make_report(&[0x04]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(report.modifier, 0x01);
    }

    #[test]
    fn simple_no_match_passthrough() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_simple(0x04, 0x05, 0);
        engine.config.count = 1;

        let mut report = make_report(&[0x06]); // not 0x04
        let result = remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(result, RemapResult::Pass);
        assert_eq!(report.keycode[0], 0x06);
    }

    #[test]
    fn simple_output_mask_a_only() {
        let mut engine = new_engine();
        let mut entry = make_simple(0x04, 0x05, 0);
        entry.output_mask = 0x01; // Output A only
        engine.config.entries[0] = entry;
        engine.config.count = 1;

        let mut report = make_report(&[0x04]);
        let result = remap_engine_process(&mut engine, &mut report, 1, 1000); // Output B
        assert_eq!(result, RemapResult::Pass);
        assert_eq!(report.keycode[0], 0x04); // unchanged
    }

    #[test]
    fn simple_output_mask_b_only() {
        let mut engine = new_engine();
        let mut entry = make_simple(0x04, 0x05, 0);
        entry.output_mask = 0x02; // Output B only
        engine.config.entries[0] = entry;
        engine.config.count = 1;

        let mut report = make_report(&[0x04]);
        let result = remap_engine_process(&mut engine, &mut report, 1, 1000);
        assert_eq!(result, RemapResult::Modified);
        assert_eq!(report.keycode[0], 0x05);
    }

    #[test]
    fn empty_config_passthrough() {
        let mut engine = new_engine();
        let mut report = make_report(&[0x04]);
        let result = remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(result, RemapResult::Pass);
    }

    // -- TAP_HOLD tests --

    #[test]
    fn tap_hold_consume_on_press() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;

        let mut report = make_report(&[0x39]);
        let result = remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(result, RemapResult::Modified);
        assert_eq!(report.keycode[0], 0); // trigger removed
        assert_eq!(engine.runtime[0].state, RemapState::Waiting);
    }

    #[test]
    fn tap_hold_tap_on_quick_release() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;

        // Press
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);

        // Release (no key)
        let mut report = make_report(&[]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(engine.runtime[0].state, RemapState::Idle);
        assert!(engine.runtime[0].consumed);

        // Get pending tap
        let mut out = HidKeyboardReport::default();
        assert!(remap_engine_get_pending(&mut engine, &mut out));
        assert_eq!(out.keycode[0], 0x90); // LANG1
        assert!(!engine.runtime[0].consumed); // cleared
    }

    #[test]
    fn tap_hold_hold_after_threshold() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;

        // Press
        let mut report = make_report(&[0x39]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        engine.runtime[0].timestamp = 1000;

        // Tick past threshold
        let changed = remap_engine_tick(&mut engine, 1000 + TAP_HOLD_DEFAULT_US);
        assert!(changed);
        assert_eq!(engine.runtime[0].state, RemapState::Held);
    }

    #[test]
    fn tap_hold_held_key_in_active_output() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;
        engine.runtime[0].state = RemapState::Held;

        let mut out = HidKeyboardReport::default();
        remap_engine_get_active_output(&engine, &mut out);
        assert_eq!(out.keycode[0], 0x39); // CapsLock as hold key
    }

    #[test]
    fn tap_hold_release_after_hold() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;
        engine.runtime[0].state = RemapState::Held;

        // Release
        let mut report = make_report(&[]);
        remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(engine.runtime[0].state, RemapState::Idle);
        assert!(!engine.runtime[0].consumed); // no tap pending
    }

    #[test]
    fn tap_hold_consumed_during_wait() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;
        engine.runtime[0].state = RemapState::Waiting;

        // Key still pressed
        let mut report = make_report(&[0x39]);
        let result = remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(result, RemapResult::Modified);
        assert_eq!(report.keycode[0], 0); // consumed
    }

    #[test]
    fn tick_no_change_when_idle() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;

        let changed = remap_engine_tick(&mut engine, 999_999);
        assert!(!changed);
    }

    #[test]
    fn tick_default_threshold_fallback() {
        let mut engine = new_engine();
        let mut entry = make_tap_hold(0x39, 0x90, 0x39);
        entry.threshold_us = 0; // use default
        engine.config.entries[0] = entry;
        engine.config.count = 1;
        engine.runtime[0].state = RemapState::Waiting;
        engine.runtime[0].timestamp = 1000;

        let changed = remap_engine_tick(&mut engine, 1000 + TAP_HOLD_DEFAULT_US);
        assert!(changed);
    }

    #[test]
    fn multiple_entries() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_simple(0x04, 0x05, 0);
        engine.config.entries[1] = make_simple(0x06, 0x07, 0);
        engine.config.count = 2;

        let mut report = make_report(&[0x04, 0x06]);
        let result = remap_engine_process(&mut engine, &mut report, 0, 1000);
        assert_eq!(result, RemapResult::Modified);
        assert!(report.keycode.contains(&0x05));
        assert!(report.keycode.contains(&0x07));
    }

    #[test]
    fn get_pending_clears_consumed() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;
        engine.runtime[0].consumed = true;

        let mut out = HidKeyboardReport::default();
        assert!(remap_engine_get_pending(&mut engine, &mut out));
        assert!(!engine.runtime[0].consumed);

        // Second call returns false
        assert!(!remap_engine_get_pending(&mut engine, &mut out));
    }

    #[test]
    fn get_active_output_empty_when_idle() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;

        let mut out = HidKeyboardReport::default();
        remap_engine_get_active_output(&engine, &mut out);
        assert_eq!(out.keycode[0], 0);
    }

    #[test]
    fn get_active_output_with_modifier() {
        let mut engine = new_engine();
        let mut entry = make_tap_hold(0x39, 0x90, 0x39);
        entry.hold_action.modifier = 0x04; // LAlt
        engine.config.entries[0] = entry;
        engine.config.count = 1;
        engine.runtime[0].state = RemapState::Held;

        let mut out = HidKeyboardReport::default();
        remap_engine_get_active_output(&engine, &mut out);
        assert_eq!(out.keycode[0], 0x39);
        assert_eq!(out.modifier, 0x04);
    }

    #[test]
    fn tick_returns_true_on_transition() {
        let mut engine = new_engine();
        engine.config.entries[0] = make_tap_hold(0x39, 0x90, 0x39);
        engine.config.count = 1;
        engine.runtime[0].state = RemapState::Waiting;
        engine.runtime[0].timestamp = 1000;

        // Before threshold
        assert!(!remap_engine_tick(&mut engine, 1000 + TAP_HOLD_DEFAULT_US - 1));
        // At threshold
        assert!(remap_engine_tick(&mut engine, 1000 + TAP_HOLD_DEFAULT_US));
    }
}
