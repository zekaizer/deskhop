// Keyboard state management — update, combine, and send keyboard reports.
// Uses HAL wrappers for queue access and UART transmission.

use core::ffi::c_void;
use crate::app::constants::PacketType;
use crate::app::structs::Device;
use crate::app::structs::{HidKeyboardReport, MAX_DEVICES, KBD_REPORT_LENGTH};
use crate::hal::device;

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
        // Skip if already present
        if dest.keycode.iter().any(|&k| k == key) {
            continue;
        }
        // Find empty slot
        if let Some(slot) = dest.keycode.iter_mut().find(|k| **k == 0) {
            *slot = key;
        }
    }
}

/// Combine all keyboard states into a single report
pub fn combine_kbd_states(state: &Device) -> HidKeyboardReport {
    let mut combined = HidKeyboardReport::default();

    // Combine all local keyboards
    for i in 0..=(state.max_kbd_idx as usize) {
        if i >= MAX_DEVICES {
            break;
        }
        combined.modifier |= state.local_kbd_states[i].modifier;
        add_keys(&mut combined, &state.local_kbd_states[i]);
    }

    // Add remote keyboard
    combined.modifier |= state.remote_kbd_state.modifier;
    add_keys(&mut combined, &state.remote_kbd_state);

    combined
}

/// Release all keys — clear all keyboard states and send empty report
pub unsafe fn release_all_keys(dev: *mut c_void, state: &mut Device) {
    for i in 0..MAX_DEVICES {
        state.local_kbd_states[i] = HidKeyboardReport::default();
    }
    state.remote_kbd_state = HidKeyboardReport::default();

    let empty = HidKeyboardReport::default();
    device::hal_queue_kbd_report(dev, &empty as *const _ as *const u8);
}

/// Send key report — combine all states and route to local queue or UART
pub unsafe fn send_key(dev: *mut c_void, state: &mut Device) {
    let combined = combine_kbd_states(state);

    if state.is_active_output() {
        device::hal_queue_kbd_report(dev, &combined as *const _ as *const u8);
        let role = state.board_role as usize;
        if role < state.last_activity.len() {
            state.last_activity[role] = device::hal_time_us_64();
        }
    } else {
        device::hal_queue_packet(
            &combined as *const _ as *const u8,
            PacketType::KeyboardReport as u8,
            KBD_REPORT_LENGTH as i32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_kbd_state() {
        let mut state = unsafe { core::mem::zeroed::<Device>() };
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
        let mut state = unsafe { core::mem::zeroed::<Device>() };
        let report = HidKeyboardReport::default();
        update_kbd_state(&mut state, &report, 255); // out of bounds
        // Should not crash
    }

    #[test]
    fn test_combine_kbd_states() {
        let mut state = unsafe { core::mem::zeroed::<Device>() };
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
        assert_eq!(combined.modifier, 0x07); // 0x01 | 0x02 | 0x04
        assert!(combined.keycode.contains(&0x04));
        assert!(combined.keycode.contains(&0x05));
        assert!(combined.keycode.contains(&0x06));
    }

    #[test]
    fn test_combine_no_duplicates() {
        let mut state = unsafe { core::mem::zeroed::<Device>() };
        state.local_kbd_states[0] = HidKeyboardReport {
            modifier: 0, reserved: 0, keycode: [0x04, 0x05, 0, 0, 0, 0],
        };
        state.local_kbd_states[1] = HidKeyboardReport {
            modifier: 0, reserved: 0, keycode: [0x04, 0x06, 0, 0, 0, 0], // 0x04 duplicate
        };
        state.max_kbd_idx = 1;

        let combined = combine_kbd_states(&state);
        let count = combined.keycode.iter().filter(|&&k| k == 0x04).count();
        assert_eq!(count, 1); // no duplicates
    }
}
