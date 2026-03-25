#![no_std]

pub mod app;
pub mod hal;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Rapid LED blink to indicate Rust panic (distinct from watchdog reset pattern)
    unsafe {
        loop {
            hal::device::hal_toggle_led();
            // Busy-wait ~50ms at 125MHz (no sleep_ms — might not be safe in panic)
            for _ in 0..500_000 { core::hint::black_box(()); }
        }
    }
}

#[cfg(not(test))]
use ::core::ffi::c_void;
#[cfg(not(test))]
use hal::scheduler;
#[cfg(not(test))]
use hal::traits::{Timer, Watchdog};

// C task functions — still take device_t* (c_void from Rust's perspective)
#[cfg(not(test))]
extern "C" {
    fn usb_device_task(dev: *mut c_void);
    fn kick_watchdog_task(dev: *mut c_void);
    fn process_kbd_queue_task(dev: *mut c_void);
    fn process_mouse_queue_task(dev: *mut c_void);
    fn process_hid_queue_task(dev: *mut c_void);
    fn process_uart_tx_task(dev: *mut c_void);

    fn usb_host_task(dev: *mut c_void);
    fn packet_receiver_task(dev: *mut c_void);
    fn led_blinking_task(dev: *mut c_void);
    fn screensaver_task(dev: *mut c_void);
    fn firmware_upgrade_task(dev: *mut c_void);
    fn heartbeat_output_task(dev: *mut c_void);
}

/// Core0 main loop
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_main_loop(dev: *mut c_void) -> ! {
    let hal = unsafe { hal::pico::PicoHal::new(dev) };

    // Store device pointer for FFI functions without dev parameter
    app::structs::set_global_device(dev);

    // Kick watchdog before scheduler starts (initial_setup enables it)
    hal.kick();

    let mut tasks = [
        scheduler::Task::new(usb_device_task, scheduler::top()),
        scheduler::Task::new(kick_watchdog_task, scheduler::hz(30)),
        scheduler::Task::new(process_kbd_queue_task, scheduler::hz(2000)),
        scheduler::Task::new(process_mouse_queue_task, scheduler::hz(2000)),
        scheduler::Task::new(process_hid_queue_task, scheduler::hz(1000)),
        scheduler::Task::new(process_uart_tx_task, scheduler::top()),
    ];

    loop {
        scheduler::run_all_tasks(&mut tasks, dev, &hal);
    }
}

/// Core1 main loop — receives C device_t* for C task functions.
/// Updates core1_last_loop_pass in Device directly.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_core1_loop(dev: *mut c_void) -> ! {
    let hal = unsafe { hal::pico::PicoHal::new(dev) };

    let mut tasks = [
        scheduler::Task::new(usb_host_task, scheduler::top()),
        scheduler::Task::new(packet_receiver_task, scheduler::top()),
        scheduler::Task::new(led_blinking_task, scheduler::hz(30)),
        scheduler::Task::new(screensaver_task, scheduler::hz(120)),
        scheduler::Task::new(firmware_upgrade_task, scheduler::hz(4000)),
        scheduler::Task::new(heartbeat_output_task, scheduler::hz(1)),
    ];

    loop {
        unsafe {
            let device = app::structs::device_from_ptr(dev);
            device.core1_last_loop_pass = hal.now_us_64();
        }
        scheduler::run_all_tasks(&mut tasks, dev, &hal);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        assert!(true);
    }
}
