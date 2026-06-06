//! Boot-stage LED — non-blocking, timer-driven boot indicator.
//!
//! ## Behavior spec
//!
//! During `initial_setup` the task scheduler is not running yet, so the boot
//! stages cannot be shown with a cooperative blink task. The old approach
//! busy-waited the blink pattern in `diag_led`, which blocked Core0 — and thus
//! the main loop / `tud_task` — for ~4s, stalling USB enumeration. Instead, a
//! Core0 hardware repeating timer (100ms tick, see `hal_boot_led_start` in
//! hal_shim.c) drives the LED while setup runs unblocked.
//!
//! - Pattern: stage `N` blinks `N` times (100ms on / 100ms off each), then a
//!   ~600ms pause, repeating. So you count the blinks to read the stage.
//! - Normal boot: stages advance in milliseconds; the last stage shows briefly
//!   before the main loop hands the LED off. A boot **hang** leaves the timer
//!   blinking the last reached stage forever — count it to find where it stuck.
//! - Release-capable: the timer is non-blocking, so unlike the old blocking
//!   blinks there is no boot penalty and the stage indication runs in release
//!   too (useful for diagnosing a production board that won't enumerate).
//! - Ownership: while the boot LED is active (`owns_led()`), the runtime LED
//!   task (`led_blink_tick`, Core1) defers so the two don't fight over the GPIO.
//!   `release()` (called at main-loop entry, alongside `hal_boot_led_stop`)
//!   hands the LED to the runtime task.
//!
//! Halt patterns (Panic/HardFault) are NOT handled here — they keep their
//! blocking playback in `diag_led`, since the system is stopped anyway.

/// Off ticks between blink cycles (600ms at the 100ms timer tick).
const PAUSE_TICKS: u32 = 6;

/// Pure: desired LED state for a given stage blink-count and tick step.
/// Stage `blinks` = that many (on,off) pairs, then `PAUSE_TICKS` off, repeating.
pub const fn blink_on(blinks: u8, step: u32) -> bool {
    if blinks == 0 {
        return false;
    }
    let on_steps = 2 * blinks as u32;
    let cycle = on_steps + PAUSE_TICKS;
    let s = step % cycle;
    // Even sub-steps are "on" (each blink = 1 tick on + 1 tick off).
    s < on_steps && (s & 1 == 0)
}

// Runtime state. BLINKS/STEP are touched only on Core0 (set in setup context,
// advanced by the Core0 boot timer). OWNS is written on Core0 and read on Core1
// (led_blink_tick) — a bool load/store is atomic on the bus; raw-pointer access
// avoids creating references to a `static mut`.
static mut BLINKS: u8 = 0;
static mut STEP: u32 = 0;
static mut OWNS: bool = false;

/// Set the current boot stage (blink count), reset the cycle, and take LED
/// ownership so the runtime task defers.
///
/// # Safety
/// Call from the Core0 setup context (single writer).
pub unsafe fn set_stage(blinks: u8) {
    core::ptr::write(core::ptr::addr_of_mut!(BLINKS), blinks);
    core::ptr::write(core::ptr::addr_of_mut!(STEP), 0);
    core::ptr::write(core::ptr::addr_of_mut!(OWNS), true);
}

/// Advance one timer tick and return the desired LED state.
///
/// # Safety
/// Call only from the Core0 boot timer callback (single advancer).
pub unsafe fn tick() -> bool {
    let n = core::ptr::read(core::ptr::addr_of!(BLINKS));
    let s = core::ptr::read(core::ptr::addr_of!(STEP));
    core::ptr::write(core::ptr::addr_of_mut!(STEP), s.wrapping_add(1));
    blink_on(n, s)
}

/// True while the boot LED owns the indicator; the runtime LED task must defer.
pub fn owns_led() -> bool {
    unsafe { core::ptr::read(core::ptr::addr_of!(OWNS)) }
}

/// Release the LED to the runtime task (call once at main-loop entry).
///
/// # Safety
/// Call from Core0 at main-loop entry, after stopping the boot timer.
pub unsafe fn release() {
    core::ptr::write(core::ptr::addr_of_mut!(OWNS), false);
}

#[cfg(test)]
mod tests {
    use super::{blink_on, PAUSE_TICKS};

    #[test]
    fn zero_blinks_is_off() {
        for s in 0..20 {
            assert!(!blink_on(0, s));
        }
    }

    #[test]
    fn stage1_one_blink_then_pause() {
        // cycle = 2*1 + 6 = 8: on, off, then 6 off, then repeat.
        assert!(blink_on(1, 0)); // on
        assert!(!blink_on(1, 1)); // off
        for s in 2..(2 + PAUSE_TICKS) {
            assert!(!blink_on(1, s)); // pause
        }
        assert!(blink_on(1, 8)); // next cycle on
    }

    #[test]
    fn stage3_has_three_on_pulses_per_cycle() {
        let on_count = (0..(2 * 3 + PAUSE_TICKS)).filter(|&s| blink_on(3, s)).count();
        assert_eq!(on_count, 3);
    }

    #[test]
    fn count_of_on_pulses_equals_stage() {
        for n in 1u8..=7 {
            let cycle = 2 * n as u32 + PAUSE_TICKS;
            let on = (0..cycle).filter(|&s| blink_on(n, s)).count();
            assert_eq!(on, n as usize, "stage {n} should blink {n} times");
        }
    }
}
