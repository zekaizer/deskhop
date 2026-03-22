#![no_std]

pub mod constants;
pub mod crc;
pub mod hid_report;
pub mod keyboard;
pub mod mouse;
pub mod packet;
pub mod trace;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

/// Entry point called from C after hardware initialization.
/// Currently a no-op stub — returns immediately to let C main loop run.
#[no_mangle]
pub extern "C" fn rust_main_loop() {
    traceln!("rust_main_loop: entered");
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        // Verify Rust test infrastructure works
        assert!(true);
    }
}
