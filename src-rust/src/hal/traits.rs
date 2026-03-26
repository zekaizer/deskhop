// HAL trait definitions — hardware-agnostic abstraction layer.
// Method names are implementation-independent (no SDK-specific prefixes).
//
// Design notes:
// - All traits use &self (HAL ops are independent of Rust Device state)
// - Static dispatch only (no_std, no alloc) — use generics, not dyn Trait
// - Check-then-act pattern (e.g. is_busy → transmit) maps cleanly to
//   async poll() if we ever migrate to an async executor
// - See pico.rs for the real hardware implementation, mock.rs for tests

/// Microsecond timestamp source.
pub trait Timer {
    fn now_us_64(&self) -> u64;
    fn now_us_32(&self) -> u32;
}

/// System health: watchdog refresh, reset, and boot mode control.
pub trait Watchdog {
    fn kick(&self);
    fn reboot(&self) -> !;
    fn reboot_to_bootloader(&self) -> !;
    /// Set persistent flag for config-mode boot.
    fn set_boot_flag(&self);
}

/// USB device-side HID operations.
///
/// Provides standard keyboard/mouse convenience methods plus a generic
/// send_raw_report for vendor protocols (HID++, etc.).
pub trait UsbDevice {
    fn is_ready(&self) -> bool;
    fn is_suspended(&self) -> bool;
    fn remote_wakeup(&self);
    fn hid_ready(&self, instance: u8) -> bool;
    fn send_keyboard_report(&self, report_id: u8, modifier: u8, keycode: &[u8]) -> bool;
    fn send_mouse_report(
        &self,
        mode: u8,
        buttons: u8,
        x: i16,
        y: i16,
        wheel: i8,
        pan: i8,
    ) -> bool;
    /// Send an arbitrary HID report on a given instance.
    /// Enables vendor protocol passthrough (HID++ short/long/very-long reports, etc.)
    fn send_raw_report(&self, instance: u8, report_id: u8, data: &[u8]) -> bool {
        let _ = (instance, report_id, data);
        false
    }
}

/// HID report queues (mouse/keyboard) between cores.
pub trait ReportQueue {
    fn push_mouse_report(&self, report: &[u8]);
    fn push_kbd_report(&self, report: &[u8]);
    fn peek_kbd_report(&self, out: &mut [u8]) -> bool;
    fn pop_kbd_report(&self, out: &mut [u8]) -> bool;
    fn peek_mouse_report(&self, out: &mut [u8]) -> bool;
    fn pop_mouse_report(&self, out: &mut [u8]) -> bool;
}

/// Control packet queues (consumer control, system control, config).
pub trait PacketQueue {
    fn push_consumer_control(&self, payload: &[u8]);
    fn push_system_control(&self, payload: &[u8]);
    fn push_config_packet(&self, packet: &[u8]);
}

/// Inter-board communication link (high-level send + outbound queue).
pub trait PeerLink {
    /// Build and enqueue a single-value packet.
    fn send_value(&self, value: u8, packet_type: u8);
    /// Build and enqueue a data packet.
    fn send_packet(&self, data: &[u8], packet_type: u8);
    /// Enqueue a pre-built packet to the outbound buffer.
    fn enqueue(&self, packet: &[u8]);
    /// Try to enqueue raw data to the outbound buffer.
    fn try_enqueue(&self, data: &[u8]) -> bool;
    /// Dequeue one packet from the outbound buffer for transmission.
    fn dequeue(&self, out: &mut [u8]) -> bool;
}

/// Physical data transfer channel.
///
/// Abstracts the underlying transmission mechanism.
/// An async executor could wrap is_busy + transmit into a single future.
pub trait Transfer {
    fn is_busy(&self) -> bool;
    fn transmit(&self, buf: &[u8]);
}

/// Persistent configuration storage.
pub trait ConfigStore {
    fn save(&self) -> bool;
    fn load(&self);
    fn wipe(&self);
    fn read_running_fw(&self, address: u32) -> u32;
}

/// Output switching and associated state synchronization.
///
/// switch_output is a compound operation: releases keys, updates active
/// output, and syncs indicator state. sync_leds re-sends keyboard LED
/// state for the current output.
pub trait OutputControl {
    fn switch_output(&self, output: u8);
    fn sync_leds(&self);
}

/// On-board status indicator (LED or equivalent).
pub trait Indicator {
    /// Start a blink sequence (sets blinks_left counter).
    fn blink(&self);
    /// Toggle on-board LED, returns new state (true = ON).
    fn toggle(&self) -> bool;
    /// Set keyboard LEDs (Num/Caps/Scroll) via USB host SET_REPORT.
    fn set_keyboard_leds(&self, leds: u8);
}

/// Debug/diagnostic output.
pub trait Trace {
    fn blink_debug(&self, count: i32, delay_ms: i32);
    fn dump_state(&self);
}
