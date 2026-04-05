#![no_std]

pub mod domain;
pub mod hal;
pub mod service;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    extern "C" { fn diag_led(event: u8); }
    // 0xF0 = DiagEvent::Panic — rapid continuous blink, never returns
    unsafe { diag_led(0xF0); }
    loop { core::hint::spin_loop(); }
}

#[cfg(not(test))]
use hal::scheduler;
#[cfg(not(test))]
use hal::traits::{Timer, Watchdog};

// C task functions — now take no parameters (device_t* removed)
#[cfg(not(test))]
extern "C" {
    fn usb_device_task();
    fn kick_watchdog_task();
    fn process_kbd_queue_task();
    fn process_mouse_queue_task();
    fn process_hid_queue_task();
    fn process_uart_tx_task();

    fn usb_host_task();
    fn packet_receiver_task();
    fn led_blinking_task();
    fn screensaver_task();
    fn firmware_upgrade_task();
    fn heartbeat_output_task();
}

/// Core0 main loop
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_main_loop() -> ! {
    let hal = hal::pico::PicoHal::new();

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
        scheduler::run_all_tasks(&mut tasks, &hal);
    }
}

/// Core1 main loop — updates core1_last_loop_pass in Device directly.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_core1_loop() -> ! {
    let hal = hal::pico::PicoHal::new();

    let mut tasks = [
        scheduler::Task::new(usb_host_task, scheduler::top()),
        scheduler::Task::new(packet_receiver_task, scheduler::top()),
        scheduler::Task::new(led_blinking_task, scheduler::hz(30)),
        scheduler::Task::new(screensaver_task, scheduler::hz(120)),
        scheduler::Task::new(firmware_upgrade_task, scheduler::hz(4000)),
        scheduler::Task::new(heartbeat_output_task, scheduler::hz(1)),
    ];

    loop {
        // SAFETY(dual-core): u64 write is NOT atomic on Cortex-M0+. Core0 reads
        // this field in check_system_health() (service/tasks.rs). A torn read may
        // yield a stale timestamp, causing at most one missed watchdog kick or one
        // false hang detection. This is tolerable — the next iteration corrects it.
        unsafe {
            let ds = domain::structs::DeviceState::from_globals();
            ds.cfg.core1_last_loop_pass = hal.now_us_64();
        }
        scheduler::run_all_tasks(&mut tasks, &hal);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        assert!(true);
    }
}
