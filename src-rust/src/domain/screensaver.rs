use crate::domain::constants::{MAX_SCREEN_COORD, MIN_SCREEN_COORD, RELATIVE};

const JITTER_DISTANCE: i16 = 2;

/// Mouse report for screensaver output
// WORKAROUND(c-compat): Uses #[repr(C)] to match C mouse_report_t layout.
// When passed to C via FFI, must be converted to packed format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(C)]
pub struct MouseReport {
    pub buttons: u8,
    pub x: i16,
    pub y: i16,
    pub wheel: i8,
    pub pan: i8,
    pub mode: u8,
}

/// Screensaver mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Disabled = 0,
    Pong = 1,
    Jitter = 2,
}

impl Mode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Disabled),
            1 => Some(Self::Pong),
            2 => Some(Self::Jitter),
            _ => None,
        }
    }

    /// Delay in microseconds between cursor movements
    pub fn delay_us(self) -> u32 {
        match self {
            Mode::Disabled => 0,
            Mode::Pong => 5_000,      // 5 ms for high framerate
            Mode::Jitter => 10_000_000, // 10 sec
        }
    }
}

/// State for the pong screensaver
pub struct PongState {
    pub x: i16,
    pub y: i16,
    pub dx: i16,
    pub dy: i16,
}

impl Default for PongState {
    fn default() -> Self {
        Self::new()
    }
}

impl PongState {
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            dx: 20,
            dy: 25,
        }
    }

    /// Advance pong cursor and bounce off walls. Returns absolute mouse report.
    pub fn step(&mut self) -> MouseReport {
        if (self.x as i32 + self.dx as i32) < MIN_SCREEN_COORD as i32
            || (self.x as i32 + self.dx as i32) > MAX_SCREEN_COORD as i32
        {
            self.dx = -self.dx;
        }

        if (self.y as i32 + self.dy as i32) < MIN_SCREEN_COORD as i32
            || (self.y as i32 + self.dy as i32) > MAX_SCREEN_COORD as i32
        {
            self.dy = -self.dy;
        }

        self.x += self.dx;
        self.y += self.dy;

        MouseReport {
            x: self.x,
            y: self.y,
            ..MouseReport::default()
        }
    }
}

/// State for the jitter screensaver
pub struct JitterState {
    pub y: i16,
}

impl Default for JitterState {
    fn default() -> Self {
        Self::new()
    }
}

impl JitterState {
    pub const fn new() -> Self {
        Self {
            y: JITTER_DISTANCE,
        }
    }

    /// Toggle jitter direction and return relative mouse report.
    pub fn step(&mut self) -> MouseReport {
        self.y = -self.y;

        MouseReport {
            y: self.y,
            mode: RELATIVE,
            ..MouseReport::default()
        }
    }
}

/// Parameters for screensaver activation check
pub struct ScreensaverConfig {
    pub mode: u8,
    pub only_if_inactive: bool,
    pub idle_time_us: u64,
    pub max_time_us: u64,
}

/// Check if the screensaver should activate based on current conditions.
/// Returns true if the screensaver should produce a movement.
pub fn should_activate(
    config: &ScreensaverConfig,
    inactivity_us: u64,
    is_active_output: bool,
    usb_ready: bool,
    last_move_us: u32,
    current_time_us: u32,
) -> bool {
    let mode = match Mode::from_u8(config.mode) {
        Some(Mode::Disabled) | None => return false,
        Some(m) => m,
    };

    // Not idle long enough
    if inactivity_us < config.idle_time_us {
        return false;
    }

    // Exceeded maximum runtime
    if config.max_time_us > 0
        && inactivity_us > config.max_time_us + config.idle_time_us
    {
        return false;
    }

    // Only run on inactive output
    if config.only_if_inactive && is_active_output {
        return false;
    }

    // Not connected
    if !usb_ready {
        return false;
    }

    // Not time yet
    if current_time_us.wrapping_sub(last_move_us) < mode.delay_us() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pong_bounces() {
        let mut pong = PongState::new();

        // Move many times — should stay within bounds
        for _ in 0..10000 {
            let report = pong.step();
            assert!(report.x >= MIN_SCREEN_COORD);
            assert!(report.x <= MAX_SCREEN_COORD);
            assert!(report.y >= MIN_SCREEN_COORD);
            assert!(report.y <= MAX_SCREEN_COORD);
        }
    }

    #[test]
    fn test_pong_initial_direction() {
        let mut pong = PongState::new();
        let r1 = pong.step();
        assert_eq!(r1.x, 20);
        assert_eq!(r1.y, 25);

        let r2 = pong.step();
        assert_eq!(r2.x, 40);
        assert_eq!(r2.y, 50);
    }

    #[test]
    fn test_jitter_alternates() {
        let mut jitter = JitterState::new();
        let r1 = jitter.step();
        let r2 = jitter.step();

        assert_eq!(r1.y, -JITTER_DISTANCE);
        assert_eq!(r2.y, JITTER_DISTANCE);
        assert_eq!(r1.mode, RELATIVE);
    }

    #[test]
    fn test_mode_delay() {
        assert_eq!(Mode::Pong.delay_us(), 5_000);
        assert_eq!(Mode::Jitter.delay_us(), 10_000_000);
        assert_eq!(Mode::Disabled.delay_us(), 0);
    }

    #[test]
    fn test_should_activate_disabled() {
        let config = ScreensaverConfig {
            mode: 0, // Disabled
            only_if_inactive: false,
            idle_time_us: 0,
            max_time_us: 0,
        };
        assert!(!should_activate(&config, 1_000_000, false, true, 0, 100_000));
    }

    #[test]
    fn test_should_activate_not_idle_enough() {
        let config = ScreensaverConfig {
            mode: 1, // Pong
            only_if_inactive: false,
            idle_time_us: 60_000_000, // 60 sec
            max_time_us: 0,
        };
        assert!(!should_activate(&config, 30_000_000, false, true, 0, 100_000));
    }

    #[test]
    fn test_should_activate_ready() {
        let config = ScreensaverConfig {
            mode: 1,
            only_if_inactive: false,
            idle_time_us: 1_000,
            max_time_us: 0,
        };
        assert!(should_activate(&config, 1_000_000, false, true, 0, 100_000));
    }

    #[test]
    fn test_should_activate_not_connected() {
        let config = ScreensaverConfig {
            mode: 1,
            only_if_inactive: false,
            idle_time_us: 0,
            max_time_us: 0,
        };
        assert!(!should_activate(&config, 1_000_000, false, false, 0, 100_000));
    }

    #[test]
    fn test_should_activate_exceeded_max_time() {
        let config = ScreensaverConfig {
            mode: 1,
            only_if_inactive: false,
            idle_time_us: 1_000,
            max_time_us: 5_000,
        };
        // inactivity = 100_000 > idle + max = 6_000
        assert!(!should_activate(&config, 100_000, false, true, 0, 100_000));
    }

    #[test]
    fn test_should_activate_only_if_inactive() {
        let config = ScreensaverConfig {
            mode: 1,
            only_if_inactive: true,
            idle_time_us: 0,
            max_time_us: 0,
        };
        // We ARE the active output — should not activate
        assert!(!should_activate(&config, 1_000_000, true, true, 0, 100_000));
        // We are NOT the active output — should activate
        assert!(should_activate(&config, 1_000_000, false, true, 0, 100_000));
    }

    #[test]
    fn test_should_activate_too_soon_after_last_move() {
        let config = ScreensaverConfig {
            mode: 1, // PONG delay = 5000 us
            only_if_inactive: false,
            idle_time_us: 0,
            max_time_us: 0,
        };
        // last_move = 99000, current = 100000, delta = 1000 < 5000
        assert!(!should_activate(&config, 1_000_000, false, true, 99000, 100000));
        // last_move = 90000, current = 100000, delta = 10000 > 5000
        assert!(should_activate(&config, 1_000_000, false, true, 90000, 100000));
    }

    #[test]
    fn test_jitter_mode_delay() {
        let config = ScreensaverConfig {
            mode: 2, // JITTER delay = 10_000_000 us
            only_if_inactive: false,
            idle_time_us: 0,
            max_time_us: 0,
        };
        // Too soon
        assert!(!should_activate(&config, 1_000_000, false, true, 5_000_000, 10_000_000));
        // Enough time passed
        assert!(should_activate(&config, 1_000_000, false, true, 0, 10_000_001));
    }

    #[test]
    fn test_mode_invalid() {
        let config = ScreensaverConfig {
            mode: 99, // Invalid
            only_if_inactive: false,
            idle_time_us: 0,
            max_time_us: 0,
        };
        assert!(!should_activate(&config, 1_000_000, false, true, 0, 100_000));
    }

    #[test]
    fn test_pong_direction_change() {
        let mut pong = PongState::new();
        // Run until first x bounce
        let mut prev_x = 0i16;
        let mut bounced = false;
        for _ in 0..2000 {
            let r = pong.step();
            if r.x < prev_x && prev_x > 0 {
                bounced = true;
                break;
            }
            prev_x = r.x;
        }
        assert!(bounced, "Pong should bounce");
    }
}
