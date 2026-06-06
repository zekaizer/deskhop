use crate::domain::structs::HidKeyboardReport;

/// What action to perform when a hotkey matches
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    OutputToggle,
    MouseZoomToggle,
    SwitchLockToggle,
    ScreenLock,
    GamingModeToggle,
    ScreensaverPong,
    ScreensaverJitter,
    ScreensaverDisable,
    WipeConfig,
    ScreenBorder,
    ConfigEnable,
    FwUpgradeA,
    FwUpgradeB,
}

impl HotkeyAction {
    /// Short name for debug logging.
    pub const fn name(&self) -> &'static [u8] {
        match self {
            Self::OutputToggle => b"OutputToggle",
            Self::MouseZoomToggle => b"MouseZoomToggle",
            Self::SwitchLockToggle => b"SwitchLockToggle",
            Self::ScreenLock => b"ScreenLock",
            Self::GamingModeToggle => b"GamingModeToggle",
            Self::ScreensaverPong => b"ScreensaverPong",
            Self::ScreensaverJitter => b"ScreensaverJitter",
            Self::ScreensaverDisable => b"ScreensaverDisable",
            Self::WipeConfig => b"WipeConfig",
            Self::ScreenBorder => b"ScreenBorder",
            Self::ConfigEnable => b"ConfigEnable",
            Self::FwUpgradeA => b"FwUpgradeA",
            Self::FwUpgradeB => b"FwUpgradeB",
        }
    }
}

/// Hotkey definition for matching keyboard combos
pub struct HotkeyCombo {
    pub modifier: u8,
    pub keys: &'static [u8],
    pub pass_to_os: bool,
    pub acknowledge: bool,
    pub action: HotkeyAction,
}

/// Result of check_all_hotkeys
pub struct HotkeyMatch {
    pub action: HotkeyAction,
    pub pass_to_os: bool,
    pub acknowledge: bool,
}

/// Check all hotkeys against a report. Returns first match.
pub fn check_all_hotkeys(report: &HidKeyboardReport) -> Option<HotkeyMatch> {
    use crate::domain::constants::*;

    static HOTKEYS: &[HotkeyCombo] = &[
        HotkeyCombo { modifier: HOTKEY_MODIFIER, keys: &[HOTKEY_TOGGLE], pass_to_os: false, acknowledge: false, action: HotkeyAction::OutputToggle },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTALT | KEYBOARD_MODIFIER_RIGHTCTRL, keys: &[], pass_to_os: true, acknowledge: true, action: HotkeyAction::MouseZoomToggle },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTCTRL, keys: &[HID_KEY_K], pass_to_os: false, acknowledge: true, action: HotkeyAction::SwitchLockToggle },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTCTRL, keys: &[HID_KEY_L], pass_to_os: false, acknowledge: true, action: HotkeyAction::ScreenLock },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_G], pass_to_os: false, acknowledge: true, action: HotkeyAction::GamingModeToggle },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_S], pass_to_os: false, acknowledge: true, action: HotkeyAction::ScreensaverPong },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_J], pass_to_os: false, acknowledge: true, action: HotkeyAction::ScreensaverJitter },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_X], pass_to_os: false, acknowledge: true, action: HotkeyAction::ScreensaverDisable },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_F12, HID_KEY_D], pass_to_os: false, acknowledge: true, action: HotkeyAction::WipeConfig },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_F12, HID_KEY_Y], pass_to_os: false, acknowledge: true, action: HotkeyAction::ScreenBorder },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_RIGHTSHIFT, keys: &[HID_KEY_C, HID_KEY_O], pass_to_os: false, acknowledge: true, action: HotkeyAction::ConfigEnable },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTSHIFT | KEYBOARD_MODIFIER_LEFTSHIFT, keys: &[HID_KEY_A], pass_to_os: false, acknowledge: true, action: HotkeyAction::FwUpgradeA },
        HotkeyCombo { modifier: KEYBOARD_MODIFIER_RIGHTSHIFT | KEYBOARD_MODIFIER_LEFTSHIFT, keys: &[HID_KEY_B], pass_to_os: false, acknowledge: true, action: HotkeyAction::FwUpgradeB },
    ];

    for hotkey in HOTKEYS {
        if check_specific_hotkey(hotkey, report) {
            return Some(HotkeyMatch {
                action: hotkey.action,
                pass_to_os: hotkey.pass_to_os,
                acknowledge: hotkey.acknowledge,
            });
        }
    }
    None
}

/// Check if a key exists in a keyboard report.
// WORKAROUND(c-compat): Matches C behavior where key=0x00 returns true
// because empty slots contain 0x00. Could filter key==0 in the future.
pub fn key_in_report(key: u8, report: &HidKeyboardReport) -> bool {
    report.keycode.contains(&key)
}

/// Check if a keyboard report matches a specific hotkey combo
pub fn check_specific_hotkey(hotkey: &HotkeyCombo, report: &HidKeyboardReport) -> bool {
    // All specified modifiers must be present
    if hotkey.modifier != (report.modifier & hotkey.modifier) {
        return false;
    }

    // All specified keys must be present
    hotkey.keys.iter().all(|&key| key_in_report(key, report))
}

/// Add keys from src to dest, skipping zeros and duplicates.
/// Returns how many keys were added.
pub fn add_keys(dest: &mut HidKeyboardReport, src: &HidKeyboardReport) -> usize {
    let mut added = 0;

    for &key in &src.keycode {
        if key == 0 || key_in_report(key, dest) {
            continue;
        }

        // Find first empty slot
        if let Some(slot) = dest.keycode.iter_mut().find(|k| **k == 0) {
            *slot = key;
            added += 1;
        }
    }

    added
}

/// Combine multiple keyboard reports into one, merging modifiers and keys.
pub fn combine_reports(reports: &[HidKeyboardReport]) -> HidKeyboardReport {
    let mut combined = HidKeyboardReport::default();

    for report in reports {
        combined.modifier |= report.modifier;
        add_keys(&mut combined, report);
    }

    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::structs::KEYS_IN_USB_REPORT;

    fn make_report(modifier: u8, keys: &[u8]) -> HidKeyboardReport {
        let mut report = HidKeyboardReport {
            modifier,
            reserved: 0,
            keycode: [0; KEYS_IN_USB_REPORT],
        };
        for (i, &k) in keys.iter().enumerate().take(KEYS_IN_USB_REPORT) {
            report.keycode[i] = k;
        }
        report
    }

    #[test]
    fn test_key_in_report() {
        let report = make_report(0, &[0x04, 0x05, 0x06]);
        assert!(key_in_report(0x04, &report));
        assert!(key_in_report(0x06, &report));
        assert!(!key_in_report(0x07, &report));
        // Note: key_in_report(0x00) returns true because 0x00 exists
        // in empty keycode slots. This matches C behavior.
    }

    #[test]
    fn test_check_specific_hotkey_match() {
        let hotkey = HotkeyCombo {
            modifier: 0x11, // LEFT_CTRL + RIGHT_CTRL
            keys: &[0x04],  // 'a'
            pass_to_os: false,
            acknowledge: false,
            action: HotkeyAction::OutputToggle,
        };
        let report = make_report(0x11, &[0x04]);
        assert!(check_specific_hotkey(&hotkey, &report));
    }

    #[test]
    fn test_check_specific_hotkey_extra_modifiers_ok() {
        // Extra modifiers in report should not prevent match
        let hotkey = HotkeyCombo {
            modifier: 0x01, // LEFT_CTRL
            keys: &[0x04],
            pass_to_os: false,
            acknowledge: false,
            action: HotkeyAction::OutputToggle,
        };
        let report = make_report(0x11, &[0x04]); // LEFT_CTRL + RIGHT_CTRL
        assert!(check_specific_hotkey(&hotkey, &report));
    }

    #[test]
    fn test_check_specific_hotkey_missing_modifier() {
        let hotkey = HotkeyCombo {
            modifier: 0x11,
            keys: &[0x04],
            pass_to_os: false,
            acknowledge: false,
            action: HotkeyAction::OutputToggle,
        };
        let report = make_report(0x01, &[0x04]); // only LEFT_CTRL
        assert!(!check_specific_hotkey(&hotkey, &report));
    }

    #[test]
    fn test_check_specific_hotkey_missing_key() {
        let hotkey = HotkeyCombo {
            modifier: 0x01,
            keys: &[0x04, 0x05],
            pass_to_os: false,
            acknowledge: false,
            action: HotkeyAction::OutputToggle,
        };
        let report = make_report(0x01, &[0x04]); // missing 0x05
        assert!(!check_specific_hotkey(&hotkey, &report));
    }

    #[test]
    fn test_check_hotkey_modifier_only() {
        let hotkey = HotkeyCombo {
            modifier: 0x44, // RIGHT_ALT + RIGHT_CTRL
            keys: &[],
            pass_to_os: true,
            acknowledge: true,
            action: HotkeyAction::OutputToggle,
        };
        let report = make_report(0x44, &[]);
        assert!(check_specific_hotkey(&hotkey, &report));
    }

    #[test]
    fn test_add_keys_no_duplicates() {
        let mut dest = make_report(0, &[0x04, 0x05]);
        let src = make_report(0, &[0x05, 0x06]); // 0x05 duplicate
        let added = add_keys(&mut dest, &src);
        assert_eq!(added, 1); // only 0x06 added
        assert!(key_in_report(0x06, &dest));
    }

    #[test]
    fn test_add_keys_overflow() {
        let mut dest = make_report(0, &[0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
        let src = make_report(0, &[0x0A]); // no room
        let added = add_keys(&mut dest, &src);
        assert_eq!(added, 0);
    }

    #[test]
    fn test_combine_reports() {
        let r1 = make_report(0x01, &[0x04]); // LEFT_CTRL + 'a'
        let r2 = make_report(0x02, &[0x05]); // LEFT_SHIFT + 'b'
        let combined = combine_reports(&[r1, r2]);
        assert_eq!(combined.modifier, 0x03); // both modifiers
        assert!(key_in_report(0x04, &combined));
        assert!(key_in_report(0x05, &combined));
    }

}
