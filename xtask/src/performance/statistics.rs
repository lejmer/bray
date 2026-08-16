use std::num::NonZeroU64;

use super::model::ExecutionStatistics;

pub(super) fn calibrated_inner_iterations<'a>(
    seed_inner_iterations: NonZeroU64,
    target_interval_nanoseconds: u64,
    implementation_samples: impl IntoIterator<Item = &'a [u64]>,
) -> Result<NonZeroU64, String> {
    let fastest_typical_interval = implementation_samples
        .into_iter()
        .map(calibration_median)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or_else(|| "batch calibration requires implementation intervals".to_owned())?;

    let numerator = u128::from(seed_inner_iterations.get())
        .checked_mul(u128::from(target_interval_nanoseconds))
        .ok_or_else(|| "batch calibration count overflowed".to_owned())?;

    let selected = numerator
        .div_ceil(u128::from(fastest_typical_interval))
        .max(1);

    let selected =
        u64::try_from(selected).map_err(|_| "batch calibration count exceeds u64".to_owned())?;

    NonZeroU64::new(selected)
        .ok_or_else(|| "batch calibration selected a zero repetition count".to_owned())
}

fn calibration_median(samples: &[u64]) -> Result<u64, String> {
    if samples.is_empty() || samples.contains(&0) {
        return Err("batch calibration requires positive controlled intervals".to_owned());
    }

    let mut samples = samples.to_vec();

    samples.sort_unstable();

    Ok(median(&samples))
}

pub(super) fn summarize(
    mut raw_samples_nanoseconds: Vec<u64>,
    units: u64,
    scope: &str,
    inner_iterations: u64,
    timer_resolution_nanoseconds: u64,
) -> Option<ExecutionStatistics> {
    if raw_samples_nanoseconds.is_empty()
        || inner_iterations == 0
        || timer_resolution_nanoseconds == 0
    {
        return None;
    }

    raw_samples_nanoseconds.sort_unstable();

    let samples_picoseconds = raw_samples_nanoseconds
        .iter()
        .map(|sample| {
            let adjusted = u128::from(*sample) * 1_000 / u128::from(inner_iterations);

            u64::try_from(adjusted).unwrap_or(u64::MAX)
        })
        .collect::<Vec<_>>();

    let minimum_picoseconds = samples_picoseconds[0];
    let maximum_picoseconds = samples_picoseconds[samples_picoseconds.len() - 1];
    let median_picoseconds = median(&samples_picoseconds);

    let mut deviations = samples_picoseconds
        .iter()
        .map(|sample| sample.abs_diff(median_picoseconds))
        .collect::<Vec<_>>();

    deviations.sort_unstable();

    let median_absolute_deviation_picoseconds = median(&deviations);

    let median_units_per_second = units
        .saturating_mul(1_000_000_000_000)
        .checked_div(median_picoseconds.max(1))
        .unwrap_or(u64::MAX);

    Some(ExecutionStatistics {
        scope: scope.to_owned(),
        inner_iterations,
        timer_resolution_nanoseconds,
        raw_samples_nanoseconds,
        samples_picoseconds,
        minimum_picoseconds,
        median_picoseconds,
        median_absolute_deviation_picoseconds,
        maximum_picoseconds,
        median_units_per_second,
    })
}

pub(super) fn timer_resolution_nanoseconds() -> Result<u64, String> {
    const OBSERVATIONS: usize = 10_000;

    let mut previous = std::time::Instant::now();
    let mut minimum = None;

    for _ in 0..OBSERVATIONS {
        let current = std::time::Instant::now();
        let elapsed = current.duration_since(previous).as_nanos();

        if elapsed > 0 {
            let elapsed = u64::try_from(elapsed)
                .map_err(|_| "timer resolution is not representable".to_owned())?;

            minimum = Some(minimum.map_or(elapsed, |value: u64| value.min(elapsed)));
        }

        previous = current;
    }

    minimum.ok_or_else(|| "timer did not advance while measuring its resolution".to_owned())
}

pub(super) fn median(sorted: &[u64]) -> u64 {
    let middle = sorted.len() / 2;

    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        sorted[middle - 1].midpoint(sorted[middle])
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::calibrated_inner_iterations;

    #[test]
    fn calibration_uses_the_fastest_median_and_ignores_one_short_outlier() {
        let selected = calibrated_inner_iterations(
            NonZeroU64::new(1_000_000)
                .unwrap_or_else(|| panic!("fixture calibration seed must be nonzero")),
            100_000_000,
            [
                [1, 1_000_000, 1_000_000].as_slice(),
                [2_000_000, 2_000_000, 4_000_000].as_slice(),
                [3_000_000, 3_000_000, 3_000_000].as_slice(),
            ],
        )
        .unwrap_or_else(|error| panic!("fixture calibration must succeed: {error}"));

        assert_eq!(selected.get(), 100_000_000);
    }

    #[test]
    fn calibration_rejects_missing_zero_and_unrepresentable_observations() {
        let seed = NonZeroU64::MIN;

        assert!(
            calibrated_inner_iterations(seed, 100_000_000, std::iter::empty::<&[u64]>(),).is_err()
        );

        assert!(calibrated_inner_iterations(seed, 100_000_000, [&[][..]]).is_err());

        assert!(calibrated_inner_iterations(seed, 100_000_000, [&[0][..]]).is_err());

        assert!(
            calibrated_inner_iterations(
                NonZeroU64::new(u64::MAX).unwrap_or_else(|| panic!("maximum u64 must be nonzero")),
                u64::MAX,
                [&[1][..]],
            )
            .is_err()
        );
    }
}
