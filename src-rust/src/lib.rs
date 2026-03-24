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
        extern "C" { fn hal_toggle_led(); }
        loop {
            hal_toggle_led();
            // Busy-wait ~50ms at 125MHz (no sleep_ms — might not be safe in panic)
            for _ in 0..500_000 { core::hint::black_box(()); }
        }
    }
}

use ::core::ffi::c_void;
use hal::{device, scheduler};

// C task functions — still take device_t* (c_void from Rust's perspective)
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

extern "C" { fn hal_debug_blink(count: i32, delay_ms: i32); }

/// Core0 main loop
#[no_mangle]
pub extern "C" fn rust_main_loop(dev: *mut c_void) -> ! {
    // Store device pointer for FFI functions without dev parameter
    unsafe { app::structs::set_global_device(dev); }

    // Debug: 3 fast blinks = Rust main loop entered
    // Keep total under 500ms watchdog timeout (3 × 60ms × 2 = 360ms)
    unsafe {
        extern "C" { fn watchdog_update(); }
        watchdog_update();
        hal_debug_blink(3, 60);
    }

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

/// Core1 main loop — receives C device_t* for C task functions.
/// Updates core1_last_loop_pass in Device directly.
#[no_mangle]
pub extern "C" fn rust_core1_loop(dev: *mut c_void) -> ! {
    let mut tasks = [
        scheduler::Task::new(usb_host_task, scheduler::top()),
        scheduler::Task::new(packet_receiver_task, scheduler::top()),
        scheduler::Task::new(led_blinking_task, scheduler::hz(30)),
        scheduler::Task::new(screensaver_task, scheduler::hz(120)),
        scheduler::Task::new(firmware_upgrade_task, scheduler::hz(4000)),
        scheduler::Task::new(heartbeat_output_task, scheduler::hz(1)),
    ];

    loop {
        // Write directly to device_t->core1_last_loop_pass
        unsafe {
            let device = app::structs::device_from_ptr(dev);
            device.core1_last_loop_pass = device::hal_time_us_64();
        }
        scheduler::run_all_tasks(&mut tasks, dev);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        assert!(true);
    }
}
