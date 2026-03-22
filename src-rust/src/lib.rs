#![no_std]

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
    // Phase 0: no-op stub, control returns to C main loop
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        // Verify Rust test infrastructure works
        assert!(true);
    }
}
