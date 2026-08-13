use super::model::ExecutionStatistics;

pub(super) fn summarize(
    mut samples: Vec<u64>,
    units: u64,
    scope: &str,
) -> Option<ExecutionStatistics> {
    if samples.is_empty() {
        return None;
    }

    samples.sort_unstable();

    let minimum_nanoseconds = samples[0];
    let maximum_nanoseconds = samples[samples.len() - 1];
    let median_nanoseconds = median(&samples);

    let mut deviations = samples
        .iter()
        .map(|sample| sample.abs_diff(median_nanoseconds))
        .collect::<Vec<_>>();

    deviations.sort_unstable();

    let median_absolute_deviation_nanoseconds = median(&deviations);

    let median_units_per_second = units
        .saturating_mul(1_000_000_000)
        .checked_div(median_nanoseconds.max(1))
        .unwrap_or(u64::MAX);

    Some(ExecutionStatistics {
        scope: scope.to_owned(),
        samples_nanoseconds: samples,
        minimum_nanoseconds,
        median_nanoseconds,
        median_absolute_deviation_nanoseconds,
        maximum_nanoseconds,
        median_units_per_second,
    })
}

fn median(sorted: &[u64]) -> u64 {
    let middle = sorted.len() / 2;

    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        sorted[middle - 1].midpoint(sorted[middle])
    }
}
