// Hotkey handler FFI — dispatches app logic + HAL side effects.

use crate::app::constants::PacketType;
use crate::app::handlers::{get_border_position, BorderUpdate};
use crate::app::keyboard::HotkeyAction;

/// Execute a hotkey action by its enum variant. Called from kbd_process.
pub unsafe fn execute_hotkey_action(dev: *mut core::ffi::c_void, action: HotkeyAction) {
    match action {
        HotkeyAction::OutputToggle => rust_output_toggle(dev),
        HotkeyAction::MouseZoomToggle => rust_mouse_zoom_toggle(dev),
        HotkeyAction::SwitchLockToggle => rust_switch_lock_toggle(dev),
        HotkeyAction::ScreenLock => rust_screenlock_handler(dev),
        HotkeyAction::GamingModeToggle => rust_gaming_mode_toggle(dev),
        HotkeyAction::ScreensaverPong => rust_screensaver_pong_enable(dev),
        HotkeyAction::ScreensaverJitter => rust_screensaver_jitter_enable(dev),
        HotkeyAction::ScreensaverDisable => rust_screensaver_disable(dev),
        HotkeyAction::WipeConfig => rust_wipe_config_hotkey(dev),
        HotkeyAction::ScreenBorder => rust_screen_border_hotkey(dev),
        HotkeyAction::ConfigEnable => rust_config_enable(dev),
        HotkeyAction::FwUpgradeA => rust_fw_upgrade_a(),
        HotkeyAction::FwUpgradeB => rust_fw_upgrade_b(),
    }
}

fn border_to_bytes(top: i32, bottom: i32) -> [u8; 8] {
    let t = top.to_le_bytes();
    let b = bottom.to_le_bytes();
    [t[0], t[1], t[2], t[3], b[0], b[1], b[2], b[3]]
}

#[no_mangle]
pub unsafe extern "C" fn rust_output_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if crate::app::hotkey_handlers::output_toggle(state) {
        crate::hal::device::hal_set_active_output(dev, state.active_output);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_mouse_zoom_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let val = crate::app::hotkey_handlers::mouse_zoom_toggle(state);
    crate::hal::device::hal_send_value(val as u8, PacketType::MouseZoom as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_switch_lock_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let val = crate::app::hotkey_handlers::switch_lock_toggle(state);
    crate::hal::device::hal_send_value(val as u8, PacketType::SwitchLock as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_gaming_mode_toggle(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let val = crate::app::hotkey_handlers::gaming_mode_toggle(state);
    crate::hal::device::hal_send_value(val as u8, PacketType::GamingMode as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_a() { crate::hal::device::hal_reset_usb_boot(); }

#[no_mangle]
pub unsafe extern "C" fn rust_fw_upgrade_b() {
    crate::hal::device::hal_send_value(1, PacketType::FirmwareUpgrade as u8);
}

#[no_mangle]
pub unsafe extern "C" fn rust_wipe_config_hotkey(dev: *mut core::ffi::c_void) {
    crate::hal::device::hal_wipe_config();
    crate::hal::device::hal_load_config(dev);
    crate::hal::device::hal_send_value(1, PacketType::WipeConfig as u8);
}

fn screensaver_dispatch(state: &mut crate::app::structs::Device, mode: u8) {
    use crate::app::hotkey_handlers::ScreensaverAction;
    match crate::app::hotkey_handlers::screensaver_set(state, mode) {
        ScreensaverAction::UpdatedLocally => {}
        ScreensaverAction::SendToRemote(m) => {
            unsafe { crate::hal::device::hal_send_value(m, PacketType::Screensaver as u8); }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_pong_enable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if let Some(mode) = crate::app::hotkey_handlers::screensaver_pong_mode(state) {
        screensaver_dispatch(state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_jitter_enable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    if let Some(mode) = crate::app::hotkey_handlers::screensaver_jitter_mode(state) {
        screensaver_dispatch(state, mode);
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_screensaver_disable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    screensaver_dispatch(state, 0);
}

#[no_mangle]
pub unsafe extern "C" fn rust_config_enable(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    // Order matters: set scratch FIRST, release keys, THEN request reboot.
    // Original C: hal_set_config_mode_scratch → release_all_keys → reboot_requested=true
    if !state.config_mode_active {
        crate::hal::device::hal_set_config_mode_scratch();
    }
    crate::hal::device::hal_release_all_keys(dev);
    state.reboot_requested = true;
}

#[no_mangle]
pub unsafe extern "C" fn rust_screen_border_hotkey(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    let idx = state.active_output as usize;
    if idx >= state.config.output.len() { return; }
    if state.is_active_output() {
        match get_border_position(state.pointer_y) {
            BorderUpdate::Top(v) => state.config.output[idx].border.top = v,
            BorderUpdate::Bottom(v) => state.config.output[idx].border.bottom = v,
        }
        crate::hal::device::hal_save_config(dev);
    }
    let b = &state.config.output[idx].border;
    let bytes = border_to_bytes(b.top, b.bottom);
    crate::hal::device::hal_queue_packet(
        bytes.as_ptr(), PacketType::SyncBorders as u8, 8,
    );
}

#[no_mangle]
pub unsafe extern "C" fn rust_screenlock_handler(dev: *mut core::ffi::c_void) {
    let state = crate::app::structs::device_from_ptr(dev);
    for out in 0..2u8 {
        if let Some((modifier, key)) = crate::app::handlers::screenlock_keys(state.config.output[out as usize].os) {
            let mut report = [0u8; 8];
            report[0] = modifier; report[2] = key;
            if state.board_role == out {
                crate::hal::device::hal_queue_kbd_report(dev, report.as_ptr());
                crate::hal::device::hal_release_all_keys(dev);
            } else {
                crate::hal::device::hal_queue_packet(report.as_ptr(), PacketType::KeyboardReport as u8, 8);
                crate::hal::device::hal_queue_packet([0u8; 8].as_ptr(), PacketType::KeyboardReport as u8, 8);
            }
        }
    }
}
