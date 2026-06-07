// PicoHal — real hardware implementation of HAL traits.
// Wraps extern "C" functions from device.rs with trait-based interface.

use super::device;
use super::traits::*;
use crate::domain::constants::{PACKET_LENGTH, RAW_PACKET_LENGTH, START1, START2, START_LENGTH};

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
    fn enable_watchdog(&self) {
        unsafe { device::hal_watchdog_enable() }
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

    #[inline]
    fn device_disconnect(&self) {
        unsafe { device::hal_tud_disconnect() }
    }

    #[inline]
    fn device_connect(&self) {
        unsafe { device::hal_tud_connect() }
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

    #[inline]
    fn queue_hid_report(&self, instance: u8, report_id: u8, data: &[u8]) {
        unsafe { device::hal_queue_hid_report(instance, report_id, data.as_ptr(), data.len() as u8) }
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

    #[inline]
    fn set_board_led(&self, on: bool) {
        unsafe { device::hal_gpio_put_led(on) }
    }
}

// ---- DmaRx ----
//
// The UART RX ring is DMA-filled in C (uart_rxbuf, setup.c); Rust owns the read
// cursor and does the ring math here — previously the hal_*_packet helpers in
// hal_shim.c. The only remaining hardware call is the DMA write position.

const DMA_RX_BUFFER_SIZE: u32 = 1024;
/// NEXT_RING_IDX mask (0x3FF) — the ring is a power-of-two so wrap is `& mask`.
const DMA_RX_RING_MASK: u32 = DMA_RX_BUFFER_SIZE - 1;

extern "C" {
    /// DMA-filled UART receive ring (defined in setup.c). MUST be `static mut`
    /// and read via `read_volatile`: the DMA writes it asynchronously behind the
    /// compiler's back, so an immutable static (or a plain `*ptr` load) lets the
    /// optimizer treat the bytes as invariant and cache/elide the reads, breaking
    /// packet detection on real hardware (the C side read a plain mutable global,
    /// which is re-read every access).
    static mut uart_rxbuf: [u8; DMA_RX_BUFFER_SIZE as usize];
}

/// Software read cursor into uart_rxbuf. Touched only by packet_receive_tick
/// (Core1), so a plain static suffices. Starts at 0, matching the
/// zero-initialized C `global_hw.dma_ptr` it replaces.
static mut RX_READ_POS: u32 = 0;
/// Most recently fetched packet, preamble stripped: type + data[8] + checksum.
/// Returned by `in_packet_ptr` for packet_receive_tick to parse.
static mut IN_PACKET: [u8; PACKET_LENGTH] = [0; PACKET_LENGTH];

#[inline]
unsafe fn rxbuf_byte(idx: u32) -> u8 {
    let base = core::ptr::addr_of_mut!(uart_rxbuf).cast::<u8>();
    // Volatile: force a real load each call — the DMA may have written this byte
    // since the last read (see the static's comment).
    core::ptr::read_volatile(base.add((idx & DMA_RX_RING_MASK) as usize))
}

impl DmaRx for PicoHal {
    #[inline]
    fn dma_rx_current_pos(&self) -> u32 {
        unsafe { device::hal_dma_rx_remaining() }
    }

    #[inline]
    fn dma_rx_read_pos(&self) -> u32 {
        unsafe { *core::ptr::addr_of!(RX_READ_POS) }
    }

    #[inline]
    fn dma_rx_advance_one(&self) {
        unsafe {
            let p = core::ptr::addr_of_mut!(RX_READ_POS);
            *p = (*p + 1) & DMA_RX_RING_MASK;
        }
    }

    #[inline]
    fn is_start_of_packet(&self) -> bool {
        unsafe {
            let pos = *core::ptr::addr_of!(RX_READ_POS);
            rxbuf_byte(pos) == START1 && rxbuf_byte(pos + 1) == START2
        }
    }

    #[inline]
    fn fetch_packet(&self) {
        // Copy RAW_PACKET_LENGTH bytes from the ring into IN_PACKET, dropping the
        // START_LENGTH preamble, advancing the read cursor for every byte.
        unsafe {
            let dst = core::ptr::addr_of_mut!(IN_PACKET).cast::<u8>();
            let p = core::ptr::addr_of_mut!(RX_READ_POS);
            for i in 0..RAW_PACKET_LENGTH {
                if i >= START_LENGTH {
                    *dst.add(i - START_LENGTH) = rxbuf_byte(*p);
                }
                *p = (*p + 1) & DMA_RX_RING_MASK;
            }
        }
    }

    #[inline]
    fn in_packet_ptr(&self) -> *const u8 {
        core::ptr::addr_of!(IN_PACKET).cast::<u8>()
    }
}

// ---- UsbHost ----

impl UsbHost for PicoHal {
    #[inline]
    fn send_set_report(
        &self, dev_addr: u8, itf_num: u8, report_id: u8,
        report_type: u8, data: &[u8],
    ) -> bool {
        unsafe {
            device::hal_tuh_set_report(
                dev_addr, itf_num, report_id, report_type,
                data.as_ptr(), data.len() as u16,
            )
        }
    }

    #[inline]
    fn get_upstream_vid_pid(&self, dev_addr: u8) -> (u16, u16) {
        let mut vid: u16 = 0;
        let mut pid: u16 = 0;
        unsafe { device::hal_tuh_vid_pid_get(dev_addr, &mut vid, &mut pid) }
        (vid, pid)
    }

    #[inline]
    fn receive_report(&self, dev_addr: u8, instance: u8) {
        unsafe { device::hal_tuh_hid_receive_report(dev_addr, instance); }
    }
}

// ---- PassthroughHal ----

impl PassthroughHal for PicoHal {
    fn build_config_desc(&self) -> bool {
        // Implemented in Phase 5 (FFI integration) — needs pt_config_desc_buf
        // from ffi::tasks which stores the global passthrough state.
        unsafe { crate::hal::ffi::passthrough_hal_build_config_desc() }
    }

    fn clear_config_desc(&self) {
        unsafe { crate::hal::ffi::passthrough_hal_clear_config_desc() }
    }
}

// ---- ConfigFlash ----

impl crate::domain::config::ConfigFlash for PicoHal {
    #[inline]
    fn flash_read_config(&self, buf: &mut [u8]) {
        unsafe { device::hal_flash_read_config(buf.as_mut_ptr(), buf.len() as u32) }
    }

    #[inline]
    fn flash_write_config(&self, buf: &[u8]) {
        unsafe { device::hal_flash_write_config(buf.as_ptr()) }
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
