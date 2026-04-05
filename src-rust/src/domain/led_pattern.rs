// LED diagnostics pattern definitions — pure data, no HAL dependency.
// Playback lives in hal/ffi/callbacks.rs (diag_led).

/// Whether a pattern plays in all builds or debug only.
pub enum DiagLevel {
    Always,    // Plays in both release and debug
    DebugOnly, // Plays only when dh_debug feature is active
}

pub struct LedPattern {
    pub blinks: u8,
    pub on_ms: u16,
    pub off_ms: u16,
    pub pause_ms: u16,
    pub repeat: bool,
    pub level: DiagLevel,
}

#[repr(u8)]
pub enum DiagEvent {
    // Boot stages (sequential, 1-7)
    BootClock     = 1,
    BootConfig    = 2,
    BootProbe     = 3,
    BootSerial    = 4,
    BootRustInit  = 5,
    BootUsb       = 6,
    BootComplete  = 7,

    // Runtime
    DeviceConnected = 10,

    // Halt (repeat = true, never returns)
    Panic     = 0xF0,
    HardFault = 0xF1,
}

impl DiagEvent {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1  => Some(Self::BootClock),
            2  => Some(Self::BootConfig),
            3  => Some(Self::BootProbe),
            4  => Some(Self::BootSerial),
            5  => Some(Self::BootRustInit),
            6  => Some(Self::BootUsb),
            7  => Some(Self::BootComplete),
            10 => Some(Self::DeviceConnected),
            0xF0 => Some(Self::Panic),
            0xF1 => Some(Self::HardFault),
            _ => None,
        }
    }

    pub const fn pattern(&self) -> LedPattern {
        match self {
            // Boot detail — debug only
            Self::BootClock     => LedPattern { blinks: 1, on_ms: 50, off_ms: 50, pause_ms: 300, repeat: false, level: DiagLevel::DebugOnly },
            Self::BootConfig    => LedPattern { blinks: 2, on_ms: 50, off_ms: 50, pause_ms: 300, repeat: false, level: DiagLevel::DebugOnly },
            Self::BootProbe     => LedPattern { blinks: 3, on_ms: 50, off_ms: 50, pause_ms: 300, repeat: false, level: DiagLevel::DebugOnly },
            Self::BootSerial    => LedPattern { blinks: 4, on_ms: 50, off_ms: 50, pause_ms: 300, repeat: false, level: DiagLevel::DebugOnly },
            Self::BootRustInit  => LedPattern { blinks: 5, on_ms: 50, off_ms: 50, pause_ms: 300, repeat: false, level: DiagLevel::DebugOnly },
            Self::BootUsb       => LedPattern { blinks: 6, on_ms: 50, off_ms: 50, pause_ms: 300, repeat: false, level: DiagLevel::DebugOnly },

            // Boot complete — always visible
            Self::BootComplete  => LedPattern { blinks: 3, on_ms: 100, off_ms: 100, pause_ms: 0, repeat: false, level: DiagLevel::Always },

            // Runtime
            Self::DeviceConnected => LedPattern { blinks: 5, on_ms: 80, off_ms: 80, pause_ms: 0, repeat: false, level: DiagLevel::Always },

            // Halt — always visible
            Self::Panic     => LedPattern { blinks: 1, on_ms: 50,  off_ms: 50,  pause_ms: 0, repeat: true, level: DiagLevel::Always },
            Self::HardFault => LedPattern { blinks: 1, on_ms: 500, off_ms: 500, pause_ms: 0, repeat: true, level: DiagLevel::Always },
        }
    }
}
