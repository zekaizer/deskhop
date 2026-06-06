use crate::domain::constants::MAX_SCREEN_COORD;

/// Determine which screen border (top or bottom) to set based on pointer Y.
/// If pointer is above halfway, sets bottom; otherwise sets top.
pub fn get_border_position(pointer_y: i16) -> BorderUpdate {
    if pointer_y > (MAX_SCREEN_COORD / 2) {
        BorderUpdate::Bottom(pointer_y as i32)
    } else {
        BorderUpdate::Top(pointer_y as i32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderUpdate {
    Top(i32),
    Bottom(i32),
}

/// Serialize top/bottom border values into 8-byte LE representation.
pub fn border_to_bytes(top: i32, bottom: i32) -> [u8; 8] {
    let t = top.to_le_bytes();
    let b = bottom.to_le_bytes();
    [t[0], t[1], t[2], t[3], b[0], b[1], b[2], b[3]]
}

/// Firmware upgrade request (distinct from the #[repr(C)] FwUpgradeState in structs.rs)
#[derive(Debug, Clone, Copy)]
pub struct FwUpgradeRequest {
    pub upgrade_in_progress: bool,
    pub byte_done: bool,
    pub address: u32,
    pub checksum: u32,
}

impl FwUpgradeRequest {
    pub const fn idle() -> Self {
        Self {
            upgrade_in_progress: false,
            byte_done: false,
            address: 0,
            checksum: 0,
        }
    }
}

/// Determine if a firmware upgrade should be initiated based on heartbeat.
/// Returns Some(initial_state) if upgrade should start, None otherwise.
///
/// Upgrade triggers:
/// 1. Other board runs a newer version → always upgrade.
/// 2. (Debug only) Same version but different CRC, and we are Board B →
///    auto-propagate so only Board A needs to be flashed during development.
pub fn should_start_fw_upgrade(
    other_version: u16,
    our_version: u16,
    other_crc16: u16,
    our_crc16: u16,
    board_role: u8,
    already_upgrading: bool,
) -> Option<FwUpgradeRequest> {
    if already_upgrading {
        return None;
    }

    let start = other_version > our_version
        || (cfg!(feature = "dh_debug")
            && other_version == our_version
            && other_crc16 != our_crc16
            && board_role == 1); // OUTPUT_B

    if !start {
        return None;
    }

    Some(FwUpgradeRequest {
        upgrade_in_progress: true,
        byte_done: true,
        address: 0,
        checksum: 0xFFFF_FFFF,
    })
}

/// Validate firmware response address matches expected.
/// Returns false if mismatch (upgrade should be aborted).
pub fn validate_fw_response(response_addr: u32, expected_addr: u32) -> bool {
    response_addr == expected_addr
}

/// Lock screen key combo for a given OS type.
/// Returns (modifier, keycode) or None if OS doesn't support it.
pub fn screenlock_keys(os_type: u8) -> Option<(u8, u8)> {
    const WINDOWS: u8 = 3;
    const LINUX: u8 = 1;
    const MACOS: u8 = 2;

    const KEYBOARD_MODIFIER_LEFTGUI: u8 = 0x08;
    const KEYBOARD_MODIFIER_LEFTCTRL: u8 = 0x01;
    const HID_KEY_L: u8 = 0x0F;
    const HID_KEY_Q: u8 = 0x14;

    match os_type {
        WINDOWS | LINUX => Some((KEYBOARD_MODIFIER_LEFTGUI, HID_KEY_L)),
        MACOS => Some((KEYBOARD_MODIFIER_LEFTCTRL | KEYBOARD_MODIFIER_LEFTGUI, HID_KEY_Q)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_border_position_top() {
        assert_eq!(get_border_position(1000), BorderUpdate::Top(1000));
        assert_eq!(get_border_position(0), BorderUpdate::Top(0));
    }

    #[test]
    fn test_border_position_bottom() {
        assert_eq!(get_border_position(20000), BorderUpdate::Bottom(20000));
        assert_eq!(get_border_position(MAX_SCREEN_COORD), BorderUpdate::Bottom(MAX_SCREEN_COORD as i32));
    }

    #[test]
    fn test_border_position_halfway() {
        // Exactly at half = top (not > half)
        assert_eq!(get_border_position(MAX_SCREEN_COORD / 2), BorderUpdate::Top((MAX_SCREEN_COORD / 2) as i32));
    }

    #[test]
    fn test_fw_upgrade_newer_version() {
        let result = should_start_fw_upgrade(200, 100, 0, 0, 0, false);
        assert!(result.is_some());
        let state = result.unwrap();
        assert!(state.upgrade_in_progress);
        assert!(state.byte_done);
        assert_eq!(state.address, 0);
        assert_eq!(state.checksum, 0xFFFF_FFFF);
    }

    #[test]
    fn test_fw_upgrade_same_version() {
        assert!(should_start_fw_upgrade(100, 100, 0, 0, 0, false).is_none());
    }

    #[test]
    fn test_fw_upgrade_older_version() {
        assert!(should_start_fw_upgrade(50, 100, 0, 0, 0, false).is_none());
    }

    #[test]
    fn test_fw_upgrade_already_upgrading() {
        assert!(should_start_fw_upgrade(200, 100, 0, 0, 0, true).is_none());
    }

    #[test]
    fn test_fw_upgrade_crc_mismatch_board_b_debug() {
        // Same version, different CRC, Board B → upgrade only in debug
        let result = should_start_fw_upgrade(100, 100, 0xAA, 0xBB, 1, false);
        if cfg!(feature = "dh_debug") {
            assert!(result.is_some());
        } else {
            assert!(result.is_none());
        }
    }

    #[test]
    fn test_fw_upgrade_crc_mismatch_board_a_no_upgrade() {
        // Same version, different CRC, Board A → never upgrade (A is source)
        assert!(should_start_fw_upgrade(100, 100, 0xAA, 0xBB, 0, false).is_none());
    }

    #[test]
    fn test_fw_upgrade_older_version_crc_diff() {
        // A strictly-older peer must never trigger a pull, even with a differing
        // CRC — the crc-diff branch is gated on EQUAL versions, so an older peer
        // returns None regardless of dh_debug or board_role (no downgrade hole).
        assert!(should_start_fw_upgrade(50, 100, 0xAA, 0xBB, 1, false).is_none());
        assert!(should_start_fw_upgrade(50, 100, 0xAA, 0xBB, 0, false).is_none());
    }

    #[test]
    fn test_fw_upgrade_crc_equal_board_b_steady_state() {
        // Two synced boards: same version, EQUAL crc, Board B (role 1) must NOT
        // upgrade — in debug AND release — or synced boards would ping-pong
        // upgrades every heartbeat. The crc-mismatch guard prevents this.
        assert!(should_start_fw_upgrade(100, 100, 0xAA, 0xAA, 1, false).is_none());
    }

    #[test]
    fn test_fw_upgrade_pushfw_sentinel() {
        // The pushfw dev command (callbacks.rs dbg_force_push) fakes ver=0xFFFF.
        // 0xFFFF (u16 max) strictly exceeds any real version → peer always pulls,
        // via the plain `>` comparison (no 0xFFFF special-case), independent of
        // role/crc/feature.
        assert!(should_start_fw_upgrade(0xFFFF, 1, 0x11, 0x22, 0, false).is_some());
        assert!(should_start_fw_upgrade(0xFFFF, 65534, 0, 0, 1, false).is_some());
        // Collision edge: if our version is already 0xFFFF the version clause does
        // NOT fire (equal, not greater) — in release that is None.
        let same = should_start_fw_upgrade(0xFFFF, 0xFFFF, 0x11, 0x11, 1, false);
        assert!(same.is_none());
    }

    #[test]
    fn test_validate_fw_response() {
        assert!(validate_fw_response(0x1000, 0x1000));
        assert!(!validate_fw_response(0x1004, 0x1000));
    }

    #[test]
    fn test_screenlock_windows() {
        let (modifier, key) = screenlock_keys(3).unwrap(); // WINDOWS
        assert_eq!(modifier, 0x08); // LEFTGUI
        assert_eq!(key, 0x0F);     // L
    }

    #[test]
    fn test_screenlock_macos() {
        let (modifier, key) = screenlock_keys(2).unwrap(); // MACOS
        assert_eq!(modifier, 0x09); // LEFTCTRL | LEFTGUI
        assert_eq!(key, 0x14);     // Q
    }

    #[test]
    fn test_screenlock_unknown() {
        assert!(screenlock_keys(255).is_none());
    }
}
