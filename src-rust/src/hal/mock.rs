// MockHal — test-only HAL implementation for host-side unit testing.

use core::cell::Cell;

use super::traits::*;

/// Minimal mock HAL for testing. Expand fields as tests require them.
pub struct MockHal {
    pub time_us: Cell<u64>,
}

impl MockHal {
    pub fn new() -> Self {
        Self {
            time_us: Cell::new(0),
        }
    }

    pub fn advance_time(&self, us: u64) {
        self.time_us.set(self.time_us.get() + us);
    }
}

impl Timer for MockHal {
    fn now_us_64(&self) -> u64 {
        self.time_us.get()
    }

    fn now_us_32(&self) -> u32 {
        self.time_us.get() as u32
    }
}
