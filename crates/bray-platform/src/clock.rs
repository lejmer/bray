use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static PROCESS_CLOCK_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Monotonic process-local instant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicInstant(Instant);

/// Wall-clock timestamp relative to the Unix epoch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WallClockTimestamp {
    seconds: i64,
    nanoseconds: u32,
}

impl WallClockTimestamp {
    /// Converts a host wall-clock value when its whole seconds fit the platform ABI.
    pub fn from_system_time(time: SystemTime) -> Option<Self> {
        let (seconds, nanoseconds) = match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => (
                i64::try_from(duration.as_secs()).ok()?,
                duration.subsec_nanos(),
            ),
            Err(error) => timestamp_before_epoch(error.duration())?,
        };

        Some(Self {
            seconds,
            nanoseconds,
        })
    }

    /// Returns whole seconds relative to the Unix epoch.
    pub const fn seconds(self) -> i64 {
        self.seconds
    }

    /// Returns fractional nanoseconds within the current second.
    pub const fn nanoseconds(self) -> u32 {
        self.nanoseconds
    }
}

fn timestamp_before_epoch(duration: Duration) -> Option<(i64, u32)> {
    let seconds = i64::try_from(duration.as_secs()).ok()?;
    let nanoseconds = duration.subsec_nanos();

    if nanoseconds == 0 {
        Some((-seconds, 0))
    } else {
        Some((
            seconds.checked_neg()?.checked_sub(1)?,
            1_000_000_000 - nanoseconds,
        ))
    }
}

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

    /// Observes nanoseconds elapsed in the process-local monotonic clock domain.
    pub fn ticks(self) -> Option<u64> {
        let origin = PROCESS_CLOCK_ORIGIN.get_or_init(Instant::now);
        let ticks = u64::try_from(origin.elapsed().as_nanos()).ok()?;

        (ticks != u64::MAX).then_some(ticks)
    }

    /// Creates a deadline relative to the current instant when representable.
    pub fn deadline_after(self, duration: Duration) -> Option<MonotonicDeadline> {
        Instant::now().checked_add(duration).map(MonotonicDeadline)
    }

    /// Blocks the current native thread for at least the requested duration.
    pub fn sleep(self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Access to the host wall clock.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct WallClock;

impl WallClock {
    /// Observes the current wall-clock timestamp when representable by the platform ABI.
    pub fn now(self) -> Option<WallClockTimestamp> {
        WallClockTimestamp::from_system_time(SystemTime::now())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{MonotonicClock, WallClock, timestamp_before_epoch};

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

    #[test]
    fn scalar_clock_readings_preserve_clock_contracts() {
        let first = MonotonicClock
            .ticks()
            .unwrap_or_else(|| panic!("reading must fit"));

        let second = MonotonicClock
            .ticks()
            .unwrap_or_else(|| panic!("reading must fit"));

        assert!(second >= first);

        let wall = WallClock
            .now()
            .unwrap_or_else(|| panic!("wall time must fit"));

        assert!(wall.nanoseconds() < 1_000_000_000);
    }

    #[test]
    fn wall_clock_conversion_normalizes_pre_epoch_values() {
        assert_eq!(
            timestamp_before_epoch(Duration::from_nanos(1)),
            Some((-1, 999_999_999))
        );
    }
}
