use super::model::ExecutionStatistics;

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
