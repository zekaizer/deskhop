// PicoHal — real hardware implementation of HAL traits.
// Wraps extern "C" functions from device.rs with trait-based interface.

use super::device;
use super::traits::*;

/// Real HAL backed by platform SDK via C FFI.
pub struct PicoHal;

impl Default for PicoHal {
    fn default() -> Self {
        Self
    }
}

impl PicoHal {
    #[inline(always)]
    pub fn new() -> Self {
        Self
    }
}

// ---- Timer ----

impl Timer for PicoHal {
    #[inline]
    fn now_us_64(&self) -> u64 {
        unsafe { device::hal_time_us_64() }
    }

    #[inline]
    fn now_us_32(&self) -> u32 {
        unsafe { device::hal_time_us_32() }
    }
}

// ---- Watchdog ----

impl Watchdog for PicoHal {
    #[inline]
    fn kick(&self) {
        unsafe { device::hal_watchdog_update() }
    }

    #[inline]
    fn reboot(&self) -> ! {
        unsafe {
            device::reboot();
            core::hint::unreachable_unchecked()
        }
    }

    #[inline]
    fn reboot_to_bootloader(&self) -> ! {
        unsafe {
            device::hal_reset_usb_boot();
            core::hint::unreachable_unchecked()
        }
    }

    #[inline]
    fn set_boot_flag(&self) {
        unsafe { device::hal_set_config_mode_scratch() }
    }

    #[inline]
    fn is_bootsel_pressed(&self) -> bool {
        unsafe { device::hal_is_bootsel_pressed() }
    }
}

// ---- UsbDevice ----

impl UsbDevice for PicoHal {
    #[inline]
    fn is_ready(&self) -> bool {
        unsafe { device::hal_tud_ready() }
    }

    #[inline]
    fn is_suspended(&self) -> bool {
        unsafe { device::hal_tud_suspended() }
    }

    #[inline]
    fn remote_wakeup(&self) {
        unsafe { device::hal_tud_remote_wakeup() }
    }

    #[inline]
    fn hid_ready(&self, instance: u8) -> bool {
        unsafe { device::hal_tud_hid_n_ready(instance) }
    }

    #[inline]
    fn send_keyboard_report(&self, report_id: u8, modifier: u8, keycode: &[u8]) -> bool {
        unsafe { device::hal_tud_hid_keyboard_report(report_id, modifier, keycode.as_ptr()) }
    }

    #[inline]
    fn send_mouse_report(
        &self,
        mode: u8,
        buttons: u8,
        x: i16,
        y: i16,
        wheel: i8,
        pan: i8,
    ) -> bool {
        unsafe { device::hal_tud_mouse_report(mode, buttons, x, y, wheel, pan) }
    }
}

// ---- ReportQueue ----

impl ReportQueue for PicoHal {
    #[inline]
    fn push_mouse_report(&self, report: &[u8]) {
        unsafe { device::hal_queue_mouse_report(report.as_ptr()) }
    }

    #[inline]
    fn push_kbd_report(&self, report: &[u8]) {
        unsafe { device::hal_queue_kbd_report(report.as_ptr()) }
    }

    #[inline]
    fn peek_kbd_report(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_kbd_queue_peek(out.as_mut_ptr()) }
    }

    #[inline]
    fn pop_kbd_report(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_kbd_queue_remove(out.as_mut_ptr()) }
    }

    #[inline]
    fn peek_mouse_report(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_mouse_queue_peek(out.as_mut_ptr()) }
    }

    #[inline]
    fn pop_mouse_report(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_mouse_queue_remove(out.as_mut_ptr()) }
    }
}

// ---- HidQueue ----

impl HidQueue for PicoHal {
    #[inline]
    fn peek_hid_report(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_hid_queue_peek(out.as_mut_ptr()) }
    }

    #[inline]
    fn pop_hid_report(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_hid_queue_remove(out.as_mut_ptr()) }
    }

    #[inline]
    fn send_hid_report(&self, instance: u8, report_id: u8, data: &[u8]) -> bool {
        unsafe { device::hal_tud_hid_n_report(instance, report_id, data.as_ptr(), data.len() as u8) }
    }
}

// ---- PacketQueue ----

impl PacketQueue for PicoHal {
    #[inline]
    fn push_consumer_control(&self, payload: &[u8]) {
        unsafe { device::hal_queue_cc_packet(payload.as_ptr()) }
    }

    #[inline]
    fn push_system_control(&self, payload: &[u8]) {
        unsafe { device::hal_queue_system_packet(payload.as_ptr()) }
    }

    #[inline]
    fn push_config_packet(&self, packet: &[u8]) {
        unsafe { device::hal_queue_cfg_packet(packet.as_ptr()) }
    }
}

// ---- PeerLink ----

impl PeerLink for PicoHal {
    #[inline]
    fn send_value(&self, value: u8, packet_type: u8) {
        unsafe { device::send_value(value, packet_type) }
    }

    #[inline]
    fn send_packet(&self, data: &[u8], packet_type: u8) {
        unsafe { device::queue_packet(data.as_ptr(), packet_type, data.len() as i32) }
    }

    #[inline]
    fn enqueue(&self, packet: &[u8]) {
        unsafe { device::hal_queue_uart_packet(packet.as_ptr()) }
    }

    #[inline]
    fn try_enqueue(&self, data: &[u8]) -> bool {
        unsafe { device::hal_queue_try_add_uart(data.as_ptr()) }
    }

    #[inline]
    fn dequeue(&self, out: &mut [u8]) -> bool {
        unsafe { device::hal_uart_tx_queue_remove(out.as_mut_ptr()) }
    }
}

// ---- Transfer ----

impl Transfer for PicoHal {
    #[inline]
    fn is_busy(&self) -> bool {
        unsafe { device::hal_dma_channel_is_busy() }
    }

    #[inline]
    fn transmit(&self, buf: &[u8]) {
        unsafe { device::hal_dma_tx_send(buf.as_ptr(), buf.len() as u32) }
    }
}

// ---- ConfigStore ----

impl ConfigStore for PicoHal {
    #[inline]
    fn save(&self) -> bool {
        unsafe { device::save_config() }
        true // C save_config is void; assume success
    }

    #[inline]
    fn load(&self) {
        unsafe { device::load_config() }
    }

    #[inline]
    fn wipe(&self) {
        unsafe { device::wipe_config() }
    }

    #[inline]
    fn read_running_fw(&self, address: u32) -> u32 {
        unsafe { device::hal_read_fw_running_u32(address) }
    }
}

// ---- OutputControl ----

impl OutputControl for PicoHal {
    #[inline]
    fn switch_output(&self, output: u8) {
        unsafe { device::set_active_output(output) }
    }

    #[inline]
    fn sync_leds(&self) {
        unsafe { device::restore_leds() }
    }
}

// ---- Indicator ----

impl Indicator for PicoHal {
    #[inline]
    fn blink(&self) {
        unsafe { device::blink_led() }
    }

    #[inline]
    fn toggle(&self) -> bool {
        unsafe { device::hal_toggle_led() != 0 }
    }

    #[inline]
    fn set_keyboard_leds(&self, leds: u8) {
        unsafe { device::set_keyboard_leds(leds) }
    }
}

// ---- DmaRx ----

impl DmaRx for PicoHal {
    #[inline]
    fn dma_rx_current_pos(&self) -> u32 {
        unsafe { device::hal_dma_rx_remaining() }
    }

    #[inline]
    fn is_start_of_packet(&self) -> bool {
        unsafe { device::hal_is_start_of_packet() }
    }

    #[inline]
    fn fetch_packet(&self) {
        unsafe { device::hal_fetch_packet() }
    }
}

// ---- Trace ----

impl Trace for PicoHal {
    #[inline]
    fn blink_debug(&self, count: i32, delay_ms: i32) {
        unsafe { device::hal_debug_blink(count, delay_ms) }
    }

    #[inline]
    fn dump_state(&self) {
        unsafe { device::hal_debug_dump_state() }
    }
}
