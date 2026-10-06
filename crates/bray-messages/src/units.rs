use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DurationUnit {
    Nanoseconds,
    Microseconds,
    Milliseconds,
    Seconds,
    Minutes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ByteUnit {
    Bytes,
    Kibibytes,
    Mebibytes,
}

// Select units at exact boundaries and truncate to three fractional digits so a
// value below a boundary never rounds into the next unit. Integer arithmetic
// preserves precision even for durations beyond the range of floating point.
pub(crate) fn scaled_duration(duration: Duration) -> (u128, u128, DurationUnit) {
    let nanoseconds = duration.as_nanos();

    let (scale, unit) = match nanoseconds {
        0..1_000 => (1, DurationUnit::Nanoseconds),
        1_000..1_000_000 => (1_000, DurationUnit::Microseconds),
        1_000_000..1_000_000_000 => (1_000_000, DurationUnit::Milliseconds),
        1_000_000_000..60_000_000_000 => (1_000_000_000, DurationUnit::Seconds),
        _ => (60_000_000_000, DurationUnit::Minutes),
    };

    let whole = nanoseconds / scale;
    let fraction = nanoseconds % scale * 1_000 / scale;

    (whole, fraction, unit)
}

pub(crate) fn scaled_bytes(value: u64) -> (f64, ByteUnit) {
    let (scale, unit) = match value {
        0..1_024 => (1, ByteUnit::Bytes),
        1_024..1_048_576 => (1_024, ByteUnit::Kibibytes),
        _ => (1_048_576, ByteUnit::Mebibytes),
    };

    (value as f64 / f64::from(scale), unit)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ByteUnit, DurationUnit, scaled_bytes, scaled_duration};

    #[test]
    fn duration_scaling_preserves_exact_boundaries_and_fractional_precision() {
        for (nanoseconds, expected) in [
            (0, (0, 0, DurationUnit::Nanoseconds)),
            (999, (999, 0, DurationUnit::Nanoseconds)),
            (1_000, (1, 0, DurationUnit::Microseconds)),
            (1_001, (1, 1, DurationUnit::Microseconds)),
            (999_999, (999, 999, DurationUnit::Microseconds)),
            (1_000_000, (1, 0, DurationUnit::Milliseconds)),
            (1_000_001, (1, 0, DurationUnit::Milliseconds)),
            (1_250_000, (1, 250, DurationUnit::Milliseconds)),
            (999_999_999, (999, 999, DurationUnit::Milliseconds)),
            (1_000_000_000, (1, 0, DurationUnit::Seconds)),
            (1_000_000_001, (1, 0, DurationUnit::Seconds)),
            (59_999_999_999, (59, 999, DurationUnit::Seconds)),
            (60_000_000_000, (1, 0, DurationUnit::Minutes)),
            (60_000_000_001, (1, 0, DurationUnit::Minutes)),
            (60_060_000_000, (1, 1, DurationUnit::Minutes)),
            (90_000_000_000, (1, 500, DurationUnit::Minutes)),
        ] {
            assert_eq!(scaled_duration(Duration::from_nanos(nanoseconds)), expected);
        }

        assert_eq!(
            scaled_duration(Duration::MAX),
            (307_445_734_561_825_860, 266, DurationUnit::Minutes)
        );
    }

    #[test]
    fn byte_scaling_uses_binary_unit_boundaries() {
        for (value, expected) in [
            (0, (0.0, ByteUnit::Bytes)),
            (1_023, (1_023.0, ByteUnit::Bytes)),
            (1_024, (1.0, ByteUnit::Kibibytes)),
            (1_536, (1.5, ByteUnit::Kibibytes)),
            (1_048_575, (1_023.999_023_437_5, ByteUnit::Kibibytes)),
            (1_048_576, (1.0, ByteUnit::Mebibytes)),
            (1_572_864, (1.5, ByteUnit::Mebibytes)),
        ] {
            assert_eq!(scaled_bytes(value), expected);
        }
    }
}
