use std::time::{Duration, Instant};

/// Per-frame clock. `tick` returns the elapsed time since the previous tick.
/// Optionally pinned to a fixed dt for deterministic capture / soak tests.
#[derive(Debug)]
pub struct FrameClock {
    last: Instant,
    frame: u64,
    /// When `Some`, `tick` returns this duration regardless of wall time.
    /// Set via `pin_dt`; cleared via `unpin_dt`.
    pinned_dt: Option<Duration>,
}

impl FrameClock {
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            frame: 0,
            pinned_dt: None,
        }
    }

    pub fn tick(&mut self) -> Duration {
        let dt = if let Some(fixed) = self.pinned_dt {
            fixed
        } else {
            let now = Instant::now();
            let dt = now.duration_since(self.last);
            self.last = now;
            dt
        };
        self.frame = self.frame.wrapping_add(1);
        dt
    }

    pub fn frame_index(&self) -> u64 {
        self.frame
    }

    pub fn reset(&mut self) {
        self.last = Instant::now();
        self.frame = 0;
    }

    /// Pin `tick` to return this fixed duration on every call. Used by the
    /// `--deterministic` mode so a 300-frame capture is bit-identical
    /// across runs regardless of wall-clock jitter.
    pub fn pin_dt(&mut self, dt: Duration) {
        self.pinned_dt = Some(dt);
    }

    pub fn unpin_dt(&mut self) {
        self.pinned_dt = None;
        self.last = Instant::now();
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned_dt.is_some()
    }
}

impl Default for FrameClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_clock_returns_fixed_dt() {
        let mut clock = FrameClock::new();
        clock.pin_dt(Duration::from_secs_f32(1.0 / 60.0));
        let dt1 = clock.tick();
        std::thread::sleep(Duration::from_millis(5));
        let dt2 = clock.tick();
        assert_eq!(dt1, dt2);
        assert!((dt1.as_secs_f32() - 1.0 / 60.0).abs() < 1e-6);
        assert_eq!(clock.frame_index(), 2);
        assert!(clock.is_pinned());
    }
}
