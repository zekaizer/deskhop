// Host link — bidirectional report exchange with the connected PC.
// Output: drain inter-core queues and send HID reports to USB host.
// Input (future): receive vendor reports (HID++, etc.) from host.

use crate::domain::constants::ITF_NUM_HID;
use crate::domain::structs::Device;
use crate::hal::traits::*;

/// Send one pending keyboard report to the host if the endpoint is ready.
pub fn send_pending_kbd(
    state: &Device,
    hal: &(impl ReportQueue + UsbDevice),
) {
    if !state.usb_connected { return; }
    let mut report = [0u8; 8];
    if !hal.peek_kbd_report(&mut report) { return; }
    if hal.is_suspended() { hal.remote_wakeup(); }
    if !hal.hid_ready(ITF_NUM_HID) { return; }
    if hal.send_keyboard_report(1, report[0], &report[2..]) {
        hal.pop_kbd_report(&mut report);
    }
}

/// Send one pending mouse report to the host if the endpoint is ready.
pub fn send_pending_mouse(
    state: &Device,
    hal: &(impl ReportQueue + UsbDevice),
) {
    if !state.usb_connected { return; }
    let mut r = [0u8; 8];
    if !hal.peek_mouse_report(&mut r) { return; }
    if hal.is_suspended() { hal.remote_wakeup(); }
    if !hal.hid_ready(ITF_NUM_HID) { return; }
    if hal.send_mouse_report(
        r[7], r[0],
        i16::from_le_bytes([r[1], r[2]]), i16::from_le_bytes([r[3], r[4]]),
        r[5] as i8, r[6] as i8,
    ) {
        hal.pop_mouse_report(&mut r);
    }
}

/// Clear all keyboard state and send an empty report to the host.
pub fn release_all_keys(
    state: &mut Device,
    hal: &impl ReportQueue,
) {
    crate::domain::kbd_state::release_all_keys(state);
    let empty = crate::domain::structs::HidKeyboardReport::default();
    let bytes = unsafe { core::slice::from_raw_parts(&empty as *const _ as *const u8, core::mem::size_of::<crate::domain::structs::HidKeyboardReport>()) };
    hal.push_kbd_report(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::mock::MockHal;

    #[test]
    fn send_kbd_not_connected() {
        let hal = MockHal::new();
        hal.kbd_queue_in.borrow_mut().push([0x01, 0, 0x04, 0, 0, 0, 0, 0]);
        let mut state = Device::zeroed();
        state.usb_connected = false;
        send_pending_kbd(&state, &hal);
        assert_eq!(hal.kbd_queue_in.borrow().len(), 1);
    }

    #[test]
    fn send_kbd_empty_queue() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        state.usb_connected = true;
        send_pending_kbd(&state, &hal);
    }

    #[test]
    fn send_kbd_delivers_report() {
        let hal = MockHal::new();
        hal.kbd_queue_in.borrow_mut().push([0x01, 0, 0x04, 0, 0, 0, 0, 0]);
        let mut state = Device::zeroed();
        state.usb_connected = true;
        send_pending_kbd(&state, &hal);
        assert!(hal.kbd_queue_in.borrow().is_empty());
    }

    #[test]
    fn send_mouse_not_connected() {
        let hal = MockHal::new();
        hal.mouse_queue_in.borrow_mut().push([1, 10, 0, 20, 0, 0, 0, 0]);
        let mut state = Device::zeroed();
        state.usb_connected = false;
        send_pending_mouse(&state, &hal);
        assert_eq!(hal.mouse_queue_in.borrow().len(), 1);
    }

    #[test]
    fn release_sends_empty_report() {
        let hal = MockHal::new();
        let mut state = Device::zeroed();
        release_all_keys(&mut state, &hal);
        assert_eq!(hal.kbd_reports.borrow().len(), 1);
        assert_eq!(hal.kbd_reports.borrow()[0], [0u8; 8]);
    }
}
