use bray_profile::CompilationProfileDurationDistribution;

const EXACT_BUCKETS: usize = 16;
const SUBDIVISIONS: usize = 4;
const DURATION_BUCKETS: usize = EXACT_BUCKETS + (u64::BITS as usize - 4) * SUBDIVISIONS;

#[derive(Clone, Copy, Debug)]
pub(super) struct DurationHistogram {
    buckets: [u64; DURATION_BUCKETS],
    samples: u64,
    minimum: u64,
    maximum: u64,
}

impl DurationHistogram {
    pub(super) const fn new() -> Self {
        Self {
            buckets: [0; DURATION_BUCKETS],
            samples: 0,
            minimum: u64::MAX,
            maximum: 0,
        }
    }

    pub(super) fn record(&mut self, duration: u64) {
        let bucket = duration_bucket(duration);

        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
        self.samples = self.samples.saturating_add(1);
        self.minimum = self.minimum.min(duration);
        self.maximum = self.maximum.max(duration);
    }

    pub(super) fn merge(&mut self, other: &Self) {
        for (destination, source) in self.buckets.iter_mut().zip(other.buckets) {
            *destination = destination.saturating_add(source);
        }

        self.samples = self.samples.saturating_add(other.samples);

        if other.samples > 0 {
            self.minimum = self.minimum.min(other.minimum);
            self.maximum = self.maximum.max(other.maximum);
        }
    }

    pub(super) fn report(&self) -> CompilationProfileDurationDistribution {
        if self.samples == 0 {
            return CompilationProfileDurationDistribution::default();
        }

        CompilationProfileDurationDistribution {
            samples: self.samples,
            minimum_nanoseconds: self.minimum,
            median_upper_bound_nanoseconds: self.quantile_upper_bound(50, 100),
            p95_upper_bound_nanoseconds: self.quantile_upper_bound(95, 100),
            maximum_nanoseconds: self.maximum,
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
                return duration_bucket_upper_bound(bucket);
            }
        }

        self.maximum
    }
}

impl Default for DurationHistogram {
    fn default() -> Self {
        Self::new()
    }
}

const fn duration_bucket(duration: u64) -> usize {
    if duration < EXACT_BUCKETS as u64 {
        return duration as usize;
    }

    let exponent = (u64::BITS - 1 - duration.leading_zeros()) as usize;
    let base = 1_u64 << exponent;
    let step = base / SUBDIVISIONS as u64;
    let subdivision = ((duration - base) / step) as usize;

    EXACT_BUCKETS + (exponent - 4) * SUBDIVISIONS + subdivision
}

const fn duration_bucket_upper_bound(bucket: usize) -> u64 {
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
    use super::DurationHistogram;

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
