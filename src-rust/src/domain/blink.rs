// LED blinking state machine — pure logic, no HAL dependencies.
//
// The blink sequence: 5 transitions at 80ms intervals.
// "OFF, ON, OFF, ON, OFF" — ensures LED ends in a known-off state
// before restore_leds() puts it back to the correct active-output state.

pub const BLINK_INTERVAL_US: u32 = 80_000;
pub const BLINK_COUNT: i32 = 5;

/// Result of evaluating one blink step.
#[derive(Debug, PartialEq)]
pub enum BlinkAction {
    /// No blinks pending.
    Idle,
    /// Blink pending but interval not elapsed yet.
    Wait,
    /// Toggle LED and set keyboard LEDs to match.
    Toggle,
    /// Final toggle — also restore LEDs to normal state.
    ToggleAndRestore,
}

/// Evaluate whether a blink step should execute.
///
/// Uses wrapping subtraction for timer overflow safety (u32 wraps every ~71 min).
pub fn blink_step(blinks_left: i32, last_change: i32, now: u32) -> BlinkAction {
    if blinks_left == 0 {
        return BlinkAction::Idle;
    }
    if now.wrapping_sub(last_change as u32) < BLINK_INTERVAL_US {
        return BlinkAction::Wait;
    }
    if blinks_left <= 1 {
        BlinkAction::ToggleAndRestore
    } else {
        BlinkAction::Toggle
    }
}

/// Initialize a blink sequence (5 transitions, ~400ms total).
pub fn start_blink(blinks_left: &mut i32, last_change: &mut i32, now: u32) {
    *blinks_left = BLINK_COUNT;
    *last_change = now as i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_when_no_blinks() {
        assert_eq!(blink_step(0, 0, 1_000_000), BlinkAction::Idle);
    }

    #[test]
    fn wait_when_interval_not_elapsed() {
        assert_eq!(blink_step(5, 0, 50_000), BlinkAction::Wait);
    }

    #[test]
    fn toggle_when_interval_elapsed() {
        assert_eq!(blink_step(5, 0, 80_000), BlinkAction::Toggle);
        assert_eq!(blink_step(3, 0, 100_000), BlinkAction::Toggle);
    }

    #[test]
    fn toggle_and_restore_on_last_blink() {
        assert_eq!(blink_step(1, 0, 80_000), BlinkAction::ToggleAndRestore);
    }

    #[test]
    fn timer_wrapping() {
        let last = (u32::MAX - 10_000) as i32;
        let now = 70_001u32; // delta = 70_001 + 10_001 = 80_002 >= 80_000
        assert_eq!(blink_step(3, last, now), BlinkAction::Toggle);
    }

    #[test]
    fn start_blink_sets_fields() {
        let mut blinks = 0i32;
        let mut last = 0i32;
        start_blink(&mut blinks, &mut last, 500_000);
        assert_eq!(blinks, BLINK_COUNT);
        assert_eq!(last, 500_000);
    }
}
