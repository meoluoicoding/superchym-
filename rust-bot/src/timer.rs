// timer.rs — from timer.hpp

use std::time::Instant;

pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Timer { start: Instant::now() }
    }

    pub fn start(&mut self) {
        self.start = Instant::now();
    }

    pub fn elapsed_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }

    pub fn timed_out(&self, budget_ms: i64) -> bool {
        self.elapsed_ms() >= budget_ms
    }
}
