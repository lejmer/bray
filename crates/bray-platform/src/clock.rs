use std::time::{Duration, Instant};

/// Monotonic process-local instant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicInstant(Instant);

impl MonotonicInstant {
    /// Returns elapsed monotonic time, saturating at zero.
    pub fn elapsed_since(self, earlier: Self) -> Duration {
        self.0.saturating_duration_since(earlier.0)
    }

    /// Creates a deadline after this instant when representable.
    pub fn checked_add(self, duration: Duration) -> Option<MonotonicDeadline> {
        self.0.checked_add(duration).map(MonotonicDeadline)
    }
}

/// Monotonic deadline used by native waits.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicDeadline(Instant);

impl MonotonicDeadline {
    /// Returns the remaining duration, saturating at zero.
    pub fn remaining(self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    /// Returns whether this deadline has elapsed.
    pub fn has_elapsed(self) -> bool {
        self.0 <= Instant::now()
    }
}

/// Access to the host monotonic clock.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct MonotonicClock;

impl MonotonicClock {
    /// Observes the current monotonic instant.
    pub fn now(self) -> MonotonicInstant {
        MonotonicInstant(Instant::now())
    }

    /// Creates a deadline relative to the current instant when representable.
    pub fn deadline_after(self, duration: Duration) -> Option<MonotonicDeadline> {
        Instant::now().checked_add(duration).map(MonotonicDeadline)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::MonotonicClock;

    #[test]
    fn monotonic_deadlines_do_not_report_negative_remaining_time() {
        let clock = MonotonicClock;
        let start = clock.now();

        let Some(deadline) = start.checked_add(Duration::ZERO) else {
            panic!("zero duration must be representable");
        };

        assert!(deadline.has_elapsed());
        assert_eq!(deadline.remaining(), Duration::ZERO);
        assert!(clock.now().elapsed_since(start) >= Duration::ZERO);
    }
}
