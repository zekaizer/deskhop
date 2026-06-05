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

// Thin wrappers for C-defined tasks (adapt extern "C" fn → Rust ABI unsafe fn)
#[cfg(not(test))]
mod c_tasks {
    extern "C" {
        fn usb_device_task_c();
        fn usb_host_task_c();
        fn firmware_upgrade_task_c();
    }
    pub(super) unsafe fn usb_device_task() { usb_device_task_c() }
    pub(super) unsafe fn usb_host_task() { usb_host_task_c() }
    pub(super) unsafe fn firmware_upgrade_task() { firmware_upgrade_task_c() }
}

// Rust-defined tasks — direct reference, no export_name indirection
#[cfg(not(test))]
use hal::ffi::tasks;

/// Core0 main loop
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_main_loop() -> ! {
    let hal = hal::pico::PicoHal::new();

    // Kick watchdog before scheduler starts (initial_setup enables it)
    hal.kick();

    let mut task_list = [
        scheduler::Task::new(c_tasks::usb_device_task, scheduler::top()),
        scheduler::Task::new(tasks::kick_watchdog_task, scheduler::hz(30)),
        scheduler::Task::new(tasks::process_kbd_queue_task, scheduler::hz(2000)),
        scheduler::Task::new(tasks::process_mouse_queue_task, scheduler::hz(2000)),
        scheduler::Task::new(tasks::process_hid_queue_task, scheduler::hz(1000)),
        scheduler::Task::new(tasks::process_uart_tx_task, scheduler::top()),
        scheduler::Task::new(tasks::debug_log_flush_task, scheduler::hz(1000)),
        scheduler::Task::new(tasks::passthrough_task, scheduler::hz(100)),
        scheduler::Task::new(tasks::remap_engine_tick_task, scheduler::hz(1000)),
    ];

    loop {
        scheduler::run_all_tasks(&mut task_list, &hal);
    }
}

/// Core1 main loop — updates core1_last_loop_pass in Device directly.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_core1_loop() -> ! {
    let hal = hal::pico::PicoHal::new();

    let mut task_list = [
        scheduler::Task::new(c_tasks::usb_host_task, scheduler::top()),
        scheduler::Task::new(tasks::packet_receiver_task, scheduler::top()),
        scheduler::Task::new(tasks::led_blinking_task, scheduler::hz(30)),
        scheduler::Task::new(tasks::screensaver_task, scheduler::hz(120)),
        scheduler::Task::new(c_tasks::firmware_upgrade_task, scheduler::hz(4000)),
        scheduler::Task::new(tasks::heartbeat_output_task, scheduler::hz(1)),
        scheduler::Task::new(tasks::passthrough_host_tx_task, scheduler::hz(1000)),
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
        scheduler::run_all_tasks(&mut task_list, &hal);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        assert!(true);
    }
}
