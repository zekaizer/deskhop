// Keyboard state management — pure logic, no HAL dependency.

use crate::domain::structs::Device;
use crate::domain::structs::{HidKeyboardReport, MAX_DEVICES};

/// Update keyboard state for a specific device index
pub fn update_kbd_state(state: &mut Device, report: &HidKeyboardReport, device_idx: u8) {
    let idx = device_idx as usize;
    if idx >= MAX_DEVICES {
        return;
    }
    state.local_kbd_states[idx] = *report;
    if state.max_kbd_idx < device_idx {
        state.max_kbd_idx = device_idx;
    }
}

/// Update remote keyboard state (from other board via UART)
pub fn update_remote_kbd_state(state: &mut Device, report: &HidKeyboardReport) {
    state.remote_kbd_state = *report;
}

/// Add keys from src to dest, skipping zeros and duplicates
fn add_keys(dest: &mut HidKeyboardReport, src: &HidKeyboardReport) {
    for &key in &src.keycode {
        if key == 0 {
            continue;
        }
        if dest.keycode.iter().any(|&k| k == key) {
            continue;
        }
        if let Some(slot) = dest.keycode.iter_mut().find(|k| **k == 0) {
            *slot = key;
        }
    }
}

/// Combine all keyboard states into a single report
pub fn combine_kbd_states(state: &Device) -> HidKeyboardReport {
    let mut combined = HidKeyboardReport::default();

    for i in 0..=(state.max_kbd_idx as usize) {
        if i >= MAX_DEVICES {
            break;
        }
        combined.modifier |= state.local_kbd_states[i].modifier;
        add_keys(&mut combined, &state.local_kbd_states[i]);
    }

    combined.modifier |= state.remote_kbd_state.modifier;
    add_keys(&mut combined, &state.remote_kbd_state);

    combined
}

/// Release all keys — clear all keyboard states. Caller must queue empty report.
pub fn release_all_keys(state: &mut Device) {
    for i in 0..MAX_DEVICES {
        state.local_kbd_states[i] = HidKeyboardReport::default();
    }
    state.remote_kbd_state = HidKeyboardReport::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_kbd_state() {
        let mut state = Device::zeroed();
        let report = HidKeyboardReport {
            modifier: 0x01,
            reserved: 0,
            keycode: [0x04, 0, 0, 0, 0, 0],
        };
        update_kbd_state(&mut state, &report, 0);
        assert_eq!(state.local_kbd_states[0].modifier, 0x01);
        assert_eq!(state.local_kbd_states[0].keycode[0], 0x04);
        assert_eq!(state.max_kbd_idx, 0);

        update_kbd_state(&mut state, &report, 2);
        assert_eq!(state.max_kbd_idx, 2);
    }

    #[test]
    fn test_update_kbd_state_bounds() {
        let mut state = Device::zeroed();
        let report = HidKeyboardReport::default();
        update_kbd_state(&mut state, &report, 255);
    }

    #[test]
    fn test_combine_kbd_states() {
        let mut state = Device::zeroed();
        state.local_kbd_states[0] = HidKeyboardReport {
            modifier: 0x01, reserved: 0, keycode: [0x04, 0, 0, 0, 0, 0],
        };
        state.local_kbd_states[1] = HidKeyboardReport {
            modifier: 0x02, reserved: 0, keycode: [0x05, 0, 0, 0, 0, 0],
        };
        state.max_kbd_idx = 1;
        state.remote_kbd_state = HidKeyboardReport {
            modifier: 0x04, reserved: 0, keycode: [0x06, 0, 0, 0, 0, 0],
        };

        let combined = combine_kbd_states(&state);
        assert_eq!(combined.modifier, 0x07);
        assert!(combined.keycode.contains(&0x04));
        assert!(combined.keycode.contains(&0x05));
        assert!(combined.keycode.contains(&0x06));
    }

    #[test]
    fn test_combine_no_duplicates() {
        let mut state = Device::zeroed();
        state.local_kbd_states[0] = HidKeyboardReport {
            modifier: 0, reserved: 0, keycode: [0x04, 0x05, 0, 0, 0, 0],
        };
        state.local_kbd_states[1] = HidKeyboardReport {
            modifier: 0, reserved: 0, keycode: [0x04, 0x06, 0, 0, 0, 0],
        };
        state.max_kbd_idx = 1;

        let combined = combine_kbd_states(&state);
        let count = combined.keycode.iter().filter(|&&k| k == 0x04).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_release_all_keys() {
        let mut state = Device::zeroed();
        state.local_kbd_states[0].modifier = 0x01;
        state.local_kbd_states[0].keycode[0] = 0x04;
        state.remote_kbd_state.modifier = 0x02;

        release_all_keys(&mut state);

        assert_eq!(state.local_kbd_states[0].modifier, 0);
        assert_eq!(state.local_kbd_states[0].keycode[0], 0);
        assert_eq!(state.remote_kbd_state.modifier, 0);
    }
}
