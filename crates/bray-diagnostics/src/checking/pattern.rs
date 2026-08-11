use crate::DiagnosticType;

/// One source-semantic value category omitted by a match.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPatternMissingCase {
    /// The absent case of a nullable value.
    NullableAbsent,
    /// At least one present value of a nullable value.
    NullablePresent,
    /// One boolean value.
    Boolean(bool),
    /// One named union variant.
    UnionVariant(String),
    /// Values outside the explicitly listed finite cases require a catch-all pattern.
    RemainingValues,
}

impl DiagnosticPatternMissingCase {
    /// Returns the stable machine key for the missing-case category.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NullableAbsent => "nullable_absent",
            Self::NullablePresent => "nullable_present",
            Self::Boolean(_) => "boolean",
            Self::UnionVariant(_) => "union_variant",
            Self::RemainingValues => "remaining_values",
        }
    }
}

/// Exact subject and bounded missing-value categories for a non-exhaustive match.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticPatternCoverage {
    subject_type: DiagnosticType,
    missing: Box<[DiagnosticPatternMissingCase]>,
    omitted_count: u64,
}

impl DiagnosticPatternCoverage {
    /// Creates coverage context from checker-validated, deterministic missing cases.
    pub fn new(
        subject_type: DiagnosticType,
        missing: impl IntoIterator<Item = DiagnosticPatternMissingCase>,
        omitted_count: u64,
    ) -> Self {
        Self {
            subject_type,
            missing: missing.into_iter().collect(),
            omitted_count,
        }
    }

    /// Returns the matched subject type.
    pub const fn subject_type(&self) -> &DiagnosticType {
        &self.subject_type
    }

    /// Returns the deterministic missing value categories.
    pub fn missing(&self) -> &[DiagnosticPatternMissingCase] {
        &self.missing
    }

    /// Returns the number of additional finite-domain cases omitted from the bounded list.
    pub const fn omitted_count(&self) -> u64 {
        self.omitted_count
    }
}

/// Exact reason that a match arm or pattern alternative cannot be selected.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPatternUnreachability {
    /// An earlier pattern already covers every value accepted here.
    CoveredByEarlierPattern,
    /// The arm guard is the compile-time constant `false`.
    GuardAlwaysFalse,
}

impl DiagnosticPatternUnreachability {
    /// Returns the stable machine key for the unreachability reason.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CoveredByEarlierPattern => "covered_by_earlier_pattern",
            Self::GuardAlwaysFalse => "guard_always_false",
        }
    }
}
