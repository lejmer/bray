//! Versioned compiler profile reports and presentation-neutral analysis.

#![forbid(unsafe_code)]

mod analysis;
mod model;
#[cfg(test)]
mod test_support;
mod validation;

pub use analysis::{
    CompilationProfileComparison, CompilationProfileComparisonError,
    CompilationProfileMetricChange, CompilationProfileOperationChange,
    CompilationProfileQueryChange, CompilationProfileQueryTotals, CompilationProfileSummary,
};
pub use model::{
    COMPILATION_PROFILE_SCHEMA_REVISION, CompilationProfileAggregation, CompilationProfileCategory,
    CompilationProfileConfiguration, CompilationProfileContext,
    CompilationProfileDescriptorCatalog, CompilationProfileDurationDistribution,
    CompilationProfileEvent, CompilationProfileMetric, CompilationProfileMetricDescriptor,
    CompilationProfileMode, CompilationProfileOperationDescriptor,
    CompilationProfileOperationStatistics, CompilationProfileOutcome,
    CompilationProfileQueryDescriptor, CompilationProfileQueryStatistics, CompilationProfileReport,
    CompilationProfileRuntimeArtifact, CompilationProfileSchedulerStatistics,
    CompilationProfileSubject, CompilationProfileSubjectKind, CompilationProfileTimeBreakdown,
    CompilationProfileUnit,
};
pub use validation::{CompilationProfileDescriptorKind, CompilationProfileValidationError};
