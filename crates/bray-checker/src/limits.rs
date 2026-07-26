/// Deterministic resource limits shared by semantic-analysis requests.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticAnalysisLimits {
    recursion_depth: usize,
    pairwise_comparisons: u64,
}

impl SemanticAnalysisLimits {
    /// Creates explicit recursion-depth and pairwise-comparison limits.
    pub const fn new(recursion_depth: usize, pairwise_comparisons: u64) -> Self {
        Self {
            recursion_depth,
            pairwise_comparisons,
        }
    }

    /// Returns the maximum active semantic recursion depth.
    pub const fn recursion_depth(self) -> usize {
        self.recursion_depth
    }

    /// Returns the maximum pairwise semantic comparisons in one package-level fact.
    pub const fn pairwise_comparisons(self) -> u64 {
        self.pairwise_comparisons
    }
}

impl Default for SemanticAnalysisLimits {
    fn default() -> Self {
        Self::new(256, 100_000)
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticAnalysisLimits;

    #[test]
    fn default_semantic_limits_are_finite_and_nonzero() {
        let limits = SemanticAnalysisLimits::default();

        assert!(limits.recursion_depth() > 0);
        assert!(limits.pairwise_comparisons() > 0);
    }

    #[test]
    fn explicit_semantic_limits_preserve_zero_for_immediate_rejection() {
        let limits = SemanticAnalysisLimits::new(0, 0);

        assert_eq!(limits.recursion_depth(), 0);
        assert_eq!(limits.pairwise_comparisons(), 0);
    }
}
