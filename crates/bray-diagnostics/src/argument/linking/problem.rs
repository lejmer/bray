use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact native optimization report problem argument.
    pub const fn link_optimization_report_problem(
        problem: DiagnosticLinkOptimizationReportProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::LinkOptimizationReportProblem,
            DiagnosticArgValue::LinkOptimizationReportProblem(problem),
        )
    }
}

/// Locale-neutral native optimization report contract violation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkOptimizationReportProblem {
    Missing,
    Inaccessible,
    Malformed,
    UnsupportedFormat,
    ToolchainMismatch,
    DriverMismatch,
    InconsistentCacheOutcomes,
    MissingResourceMeasurement,
}

impl DiagnosticLinkOptimizationReportProblem {
    /// Returns the stable machine key for this contract violation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Inaccessible => "inaccessible",
            Self::Malformed => "malformed",
            Self::UnsupportedFormat => "unsupported_format",
            Self::ToolchainMismatch => "toolchain_mismatch",
            Self::DriverMismatch => "driver_mismatch",
            Self::InconsistentCacheOutcomes => "inconsistent_cache_outcomes",
            Self::MissingResourceMeasurement => "missing_resource_measurement",
        }
    }
}
