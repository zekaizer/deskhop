// MockHal — test-only HAL implementation for host-side unit testing.
// All trait methods record calls for verification. No real hardware access.

use core::cell::{Cell, RefCell};

use super::traits::*;

extern crate alloc;
use alloc::vec::Vec;

pub struct MockHal {
    pub time_us: Cell<u64>,
    pub watchdog_kicked: Cell<bool>,
    pub rebooted: Cell<bool>,
    pub boot_flag_set: Cell<bool>,
    pub usb_ready: Cell<bool>,
    pub usb_suspended: Cell<bool>,
    pub hid_ready_map: Cell<u8>,
    pub tx_busy: Cell<bool>,
    // Output captures
    pub mouse_reports: RefCell<Vec<[u8; 8]>>,
    pub kbd_reports: RefCell<Vec<[u8; 8]>>,
    pub outbound_packets: RefCell<Vec<[u8; 10]>>,
    pub sent_values: RefCell<Vec<(u8, u8)>>,
    pub sent_packets: RefCell<Vec<(Vec<u8>, u8)>>,
    pub cc_packets: RefCell<Vec<[u8; 4]>>,
    pub system_packets: RefCell<Vec<[u8; 2]>>,
    pub config_packets: RefCell<Vec<[u8; 10]>>,
    pub config_saved: Cell<u32>,
    pub config_loaded: Cell<u32>,
    pub config_wiped: Cell<u32>,
    pub blink_count: Cell<u32>,
    pub toggle_count: Cell<u32>,
    pub output_switched: Cell<Option<u8>>,
    pub leds_synced: Cell<u32>,
    pub dump_count: Cell<u32>,
    pub debug_blinks: RefCell<Vec<(i32, i32)>>,
    pub transmitted: RefCell<Vec<(Vec<u8>, u32)>>,
    // Input queues for peek/pop simulation
    pub kbd_queue_in: RefCell<Vec<[u8; 8]>>,
    pub mouse_queue_in: RefCell<Vec<[u8; 8]>>,
    pub outbound_queue_in: RefCell<Vec<[u8; 10]>>,
}

impl MockHal {
    pub fn new() -> Self {
        Self {
            time_us: Cell::new(0),
            watchdog_kicked: Cell::new(false),
            rebooted: Cell::new(false),
            boot_flag_set: Cell::new(false),
            usb_ready: Cell::new(true),
            usb_suspended: Cell::new(false),
            hid_ready_map: Cell::new(0xFF),
            tx_busy: Cell::new(false),
            mouse_reports: RefCell::new(Vec::new()),
            kbd_reports: RefCell::new(Vec::new()),
            outbound_packets: RefCell::new(Vec::new()),
            sent_values: RefCell::new(Vec::new()),
            sent_packets: RefCell::new(Vec::new()),
            cc_packets: RefCell::new(Vec::new()),
            system_packets: RefCell::new(Vec::new()),
            config_packets: RefCell::new(Vec::new()),
            config_saved: Cell::new(0),
            config_loaded: Cell::new(0),
            config_wiped: Cell::new(0),
            blink_count: Cell::new(0),
            toggle_count: Cell::new(0),
            output_switched: Cell::new(None),
            leds_synced: Cell::new(0),
            dump_count: Cell::new(0),
            debug_blinks: RefCell::new(Vec::new()),
            transmitted: RefCell::new(Vec::new()),
            kbd_queue_in: RefCell::new(Vec::new()),
            mouse_queue_in: RefCell::new(Vec::new()),
            outbound_queue_in: RefCell::new(Vec::new()),
        }
    }

    pub fn advance_time(&self, us: u64) {
        self.time_us.set(self.time_us.get() + us);
    }

    pub fn set_time(&self, us: u64) {
        self.time_us.set(us);
    }

    fn copy_buf<const N: usize>(src: &[u8]) -> [u8; N] {
        let mut buf = [0u8; N];
        let len = src.len().min(N);
        buf[..len].copy_from_slice(&src[..len]);
        buf
    }

    fn pop_from<const N: usize>(queue: &RefCell<Vec<[u8; N]>>, out: &mut [u8]) -> bool {
        let mut q = queue.borrow_mut();
        if q.is_empty() { return false; }
        let item = q.remove(0);
        let len = out.len().min(N);
        out[..len].copy_from_slice(&item[..len]);
        true
    }

    fn peek_from<const N: usize>(queue: &RefCell<Vec<[u8; N]>>, out: &mut [u8]) -> bool {
        let q = queue.borrow();
        if q.is_empty() { return false; }
        let len = out.len().min(N);
        out[..len].copy_from_slice(&q[0][..len]);
        true
    }
}

// ---- Timer ----

impl Timer for MockHal {
    fn now_us_64(&self) -> u64 { self.time_us.get() }
    fn now_us_32(&self) -> u32 { self.time_us.get() as u32 }
}

// ---- Watchdog ----

impl Watchdog for MockHal {
    fn kick(&self) { self.watchdog_kicked.set(true); }
    fn reboot(&self) -> ! { self.rebooted.set(true); panic!("MockHal::reboot"); }
    fn reboot_to_bootloader(&self) -> ! { panic!("MockHal::reboot_to_bootloader"); }
    fn set_boot_flag(&self) { self.boot_flag_set.set(true); }
}

// ---- UsbDevice ----

impl UsbDevice for MockHal {
    fn is_ready(&self) -> bool { self.usb_ready.get() }
    fn is_suspended(&self) -> bool { self.usb_suspended.get() }
    fn remote_wakeup(&self) {}
    fn hid_ready(&self, instance: u8) -> bool { self.hid_ready_map.get() & (1 << instance) != 0 }
    fn send_keyboard_report(&self, _report_id: u8, _modifier: u8, _keycode: &[u8]) -> bool { true }
    fn send_mouse_report(&self, _mode: u8, _buttons: u8, _x: i16, _y: i16, _wheel: i8, _pan: i8) -> bool { true }
}

// ---- ReportQueue ----

impl ReportQueue for MockHal {
    fn push_mouse_report(&self, report: &[u8]) {
        self.mouse_reports.borrow_mut().push(Self::copy_buf::<8>(report));
    }
    fn push_kbd_report(&self, report: &[u8]) {
        self.kbd_reports.borrow_mut().push(Self::copy_buf::<8>(report));
    }
    fn peek_kbd_report(&self, out: &mut [u8]) -> bool { Self::peek_from(&self.kbd_queue_in, out) }
    fn pop_kbd_report(&self, out: &mut [u8]) -> bool { Self::pop_from(&self.kbd_queue_in, out) }
    fn peek_mouse_report(&self, out: &mut [u8]) -> bool { Self::peek_from(&self.mouse_queue_in, out) }
    fn pop_mouse_report(&self, out: &mut [u8]) -> bool { Self::pop_from(&self.mouse_queue_in, out) }
}

// ---- HidQueue ----

impl HidQueue for MockHal {
    fn peek_hid_report(&self, _out: &mut [u8]) -> bool { false }
    fn pop_hid_report(&self, _out: &mut [u8]) -> bool { false }
    fn send_hid_report(&self, _instance: u8, _report_id: u8, _data: &[u8]) -> bool { true }
}

// ---- PacketQueue ----

impl PacketQueue for MockHal {
    fn push_consumer_control(&self, payload: &[u8]) {
        self.cc_packets.borrow_mut().push(Self::copy_buf::<4>(payload));
    }
    fn push_system_control(&self, payload: &[u8]) {
        self.system_packets.borrow_mut().push(Self::copy_buf::<2>(payload));
    }
    fn push_config_packet(&self, packet: &[u8]) {
        self.config_packets.borrow_mut().push(Self::copy_buf::<10>(packet));
    }
}

// ---- PeerLink ----

impl PeerLink for MockHal {
    fn send_value(&self, value: u8, packet_type: u8) {
        self.sent_values.borrow_mut().push((value, packet_type));
    }
    fn send_packet(&self, data: &[u8], packet_type: u8) {
        self.sent_packets.borrow_mut().push((data.to_vec(), packet_type));
    }
    fn enqueue(&self, packet: &[u8]) {
        self.outbound_packets.borrow_mut().push(Self::copy_buf::<10>(packet));
    }
    fn try_enqueue(&self, _data: &[u8]) -> bool { true }
    fn dequeue(&self, out: &mut [u8]) -> bool {
        Self::pop_from(&self.outbound_queue_in, out)
    }
}

// ---- Transfer ----

impl Transfer for MockHal {
    fn is_busy(&self) -> bool { self.tx_busy.get() }
    fn transmit(&self, buf: &[u8]) {
        self.transmitted.borrow_mut().push((buf.to_vec(), buf.len() as u32));
    }
}

// ---- ConfigStore ----

impl ConfigStore for MockHal {
    fn save(&self) -> bool { self.config_saved.set(self.config_saved.get() + 1); true }
    fn load(&self) { self.config_loaded.set(self.config_loaded.get() + 1); }
    fn wipe(&self) { self.config_wiped.set(self.config_wiped.get() + 1); }
    fn read_running_fw(&self, _address: u32) -> u32 { 0 }
}

// ---- OutputControl ----

impl OutputControl for MockHal {
    fn switch_output(&self, output: u8) { self.output_switched.set(Some(output)); }
    fn sync_leds(&self) { self.leds_synced.set(self.leds_synced.get() + 1); }
}

// ---- Indicator ----

impl Indicator for MockHal {
    fn blink(&self) { self.blink_count.set(self.blink_count.get() + 1); }
    fn toggle(&self) -> bool {
        let n = self.toggle_count.get() + 1;
        self.toggle_count.set(n);
        n % 2 == 1 // alternates: false→true→false→...
    }
    fn set_keyboard_leds(&self, _leds: u8) {
        // Tracked via toggle_count for now
    }
}

// ---- Trace ----

impl Trace for MockHal {
    fn blink_debug(&self, count: i32, delay_ms: i32) {
        self.debug_blinks.borrow_mut().push((count, delay_ms));
    }
    fn dump_state(&self) { self.dump_count.set(self.dump_count.get() + 1); }
}
