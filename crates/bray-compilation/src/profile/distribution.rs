use bray_profile::{CompilationProfileCountDistribution, CompilationProfileDurationDistribution};

const EXACT_BUCKETS: usize = 16;
const SUBDIVISIONS: usize = 4;
const DURATION_BUCKETS: usize = EXACT_BUCKETS + (u64::BITS as usize - 4) * SUBDIVISIONS;

#[derive(Clone, Copy, Debug)]
pub(super) struct DurationHistogram {
    values: BoundedHistogram,
}

impl DurationHistogram {
    pub(super) const fn new() -> Self {
        Self {
            values: BoundedHistogram::new(),
        }
    }

    pub(super) fn record(&mut self, duration: u64) {
        self.values.record(duration);
    }

    pub(super) fn merge(&mut self, other: &Self) {
        self.values.merge(&other.values);
    }

    pub(super) fn report(&self) -> CompilationProfileDurationDistribution {
        let summary = self.values.summary();

        CompilationProfileDurationDistribution {
            samples: summary.samples,
            minimum_nanoseconds: summary.minimum,
            median_upper_bound_nanoseconds: summary.median_upper_bound,
            p95_upper_bound_nanoseconds: summary.p95_upper_bound,
            maximum_nanoseconds: summary.maximum,
        }
    }
}

impl Default for DurationHistogram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CountHistogram {
    values: BoundedHistogram,
}

impl CountHistogram {
    pub(super) fn record(&mut self, value: u64) {
        self.values.record(value);
    }

    pub(super) fn merge(&mut self, other: &Self) {
        self.values.merge(&other.values);
    }

    pub(super) fn report(&self) -> CompilationProfileCountDistribution {
        let summary = self.values.summary();

        CompilationProfileCountDistribution {
            samples: summary.samples,
            minimum: summary.minimum,
            median_upper_bound: summary.median_upper_bound,
            p95_upper_bound: summary.p95_upper_bound,
            maximum: summary.maximum,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BoundedHistogram {
    buckets: [u64; DURATION_BUCKETS],
    samples: u64,
    minimum: u64,
    maximum: u64,
}

impl BoundedHistogram {
    const fn new() -> Self {
        Self {
            buckets: [0; DURATION_BUCKETS],
            samples: 0,
            minimum: u64::MAX,
            maximum: 0,
        }
    }

    fn record(&mut self, value: u64) {
        let bucket = value_bucket(value);

        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
        self.samples = self.samples.saturating_add(1);
        self.minimum = self.minimum.min(value);
        self.maximum = self.maximum.max(value);
    }

    fn merge(&mut self, other: &Self) {
        for (destination, source) in self.buckets.iter_mut().zip(other.buckets) {
            *destination = destination.saturating_add(source);
        }

        self.samples = self.samples.saturating_add(other.samples);

        if other.samples > 0 {
            self.minimum = self.minimum.min(other.minimum);
            self.maximum = self.maximum.max(other.maximum);
        }
    }

    fn summary(&self) -> DistributionSummary {
        if self.samples == 0 {
            return DistributionSummary::default();
        }

        DistributionSummary {
            samples: self.samples,
            minimum: self.minimum,
            median_upper_bound: self.quantile_upper_bound(50, 100),
            p95_upper_bound: self.quantile_upper_bound(95, 100),
            maximum: self.maximum,
        }
    }

    fn quantile_upper_bound(&self, numerator: u64, denominator: u64) -> u64 {
        let rank = self
            .samples
            .saturating_mul(numerator)
            .saturating_add(denominator.saturating_sub(1))
            / denominator;

        let mut observed = 0_u64;

        for (bucket, count) in self.buckets.iter().copied().enumerate() {
            observed = observed.saturating_add(count);

            if observed >= rank {
                return value_bucket_upper_bound(bucket);
            }
        }

        self.maximum
    }
}

impl Default for BoundedHistogram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DistributionSummary {
    samples: u64,
    minimum: u64,
    median_upper_bound: u64,
    p95_upper_bound: u64,
    maximum: u64,
}

const fn value_bucket(value: u64) -> usize {
    if value < EXACT_BUCKETS as u64 {
        return value as usize;
    }

    let exponent = (u64::BITS - 1 - value.leading_zeros()) as usize;
    let base = 1_u64 << exponent;
    let step = base / SUBDIVISIONS as u64;
    let subdivision = ((value - base) / step) as usize;

    EXACT_BUCKETS + (exponent - 4) * SUBDIVISIONS + subdivision
}

const fn value_bucket_upper_bound(bucket: usize) -> u64 {
    if bucket < EXACT_BUCKETS {
        return bucket as u64;
    }

    let offset = bucket - EXACT_BUCKETS;
    let exponent = 4 + offset / SUBDIVISIONS;
    let subdivision = offset % SUBDIVISIONS;
    let base = 1_u64 << exponent;
    let step = base / SUBDIVISIONS as u64;

    if exponent == 63 && subdivision == SUBDIVISIONS - 1 {
        return u64::MAX;
    }

    base + step * (subdivision as u64 + 1) - 1
}

#[cfg(test)]
mod tests {
    use super::{CountHistogram, DurationHistogram};

    #[test]
    fn count_histograms_share_bounded_distribution_semantics() {
        let mut histogram = CountHistogram::default();

        for value in [0, 4, 8, 16] {
            histogram.record(value);
        }

        let report = histogram.report();

        assert_eq!(report.samples, 4);
        assert_eq!(report.minimum, 0);
        assert_eq!(report.median_upper_bound, 4);
        assert_eq!(report.maximum, 16);
    }

    #[test]
    fn duration_histograms_report_bounded_quantiles_and_exact_extrema() {
        let mut histogram = DurationHistogram::new();

        for duration in [0, 1, 2, 3, 4, 8, 16, u64::MAX] {
            histogram.record(duration);
        }

        let report = histogram.report();

        assert_eq!(report.samples, 8);
        assert_eq!(report.minimum_nanoseconds, 0);
        assert_eq!(report.median_upper_bound_nanoseconds, 3);
        assert_eq!(report.p95_upper_bound_nanoseconds, u64::MAX);
        assert_eq!(report.maximum_nanoseconds, u64::MAX);
    }

    #[test]
    fn duration_histograms_merge_without_retaining_samples() {
        let mut left = DurationHistogram::new();
        let mut right = DurationHistogram::new();

        left.record(2);
        right.record(9);
        left.merge(&right);

        let report = left.report();

        assert_eq!(report.samples, 2);
        assert_eq!(report.minimum_nanoseconds, 2);
        assert_eq!(report.maximum_nanoseconds, 9);
    }

    #[test]
    fn duration_histogram_bounds_remain_close_to_long_observations() {
        let duration = 165_530_916_500;
        let mut histogram = DurationHistogram::new();

        histogram.record(duration);

        let upper = histogram.report().median_upper_bound_nanoseconds;

        assert!(upper >= duration);
        assert!(upper <= duration.saturating_add(duration / 4));
    }
}
