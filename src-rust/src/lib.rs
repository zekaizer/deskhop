#![no_std]

pub mod constants;
pub mod crc;
pub mod device;
pub mod hid_report;
pub mod keyboard;
pub mod mouse;
pub mod packet;
pub mod scheduler;
pub mod trace;
pub mod usb;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

use core::ffi::c_void;

// C task functions called by the Rust scheduler
extern "C" {
    fn usb_device_task(dev: *mut c_void);
    fn kick_watchdog_task(dev: *mut c_void);
    fn process_kbd_queue_task(dev: *mut c_void);
    fn process_mouse_queue_task(dev: *mut c_void);
    fn process_hid_queue_task(dev: *mut c_void);
    fn process_uart_tx_task(dev: *mut c_void);
}

/// Core0 main loop — called from C main() after initial_setup().
/// Never returns. Replaces the C while(true) task scheduler loop.
#[no_mangle]
pub extern "C" fn rust_main_loop(dev: *mut c_void) -> ! {
    traceln!("rust_main_loop: core0 scheduler active");

    let mut tasks = [
        scheduler::Task::new(usb_device_task, scheduler::top()),
        scheduler::Task::new(kick_watchdog_task, scheduler::hz(30)),
        scheduler::Task::new(process_kbd_queue_task, scheduler::hz(2000)),
        scheduler::Task::new(process_mouse_queue_task, scheduler::hz(2000)),
        scheduler::Task::new(process_hid_queue_task, scheduler::hz(1000)),
        scheduler::Task::new(process_uart_tx_task, scheduler::top()),
    ];

    loop {
        scheduler::run_all_tasks(&mut tasks, dev);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        // Verify Rust test infrastructure works
        assert!(true);
    }
}
