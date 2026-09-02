//! Compiler entry points, requests, and demand-driven compiler queries.

#![forbid(unsafe_code)]

mod compilation;
mod compiler_known;
mod fact;
mod profile;
mod request;
mod target;
mod worker;

#[cfg(test)]
mod test_support;

pub use compilation::{
    AsyncAnalysisView, BuildConfiguration, CodegenPreparationError, Compilation,
    CompilationLoadError, CompletionCandidate, DependencyContractsView, EmissionCodegenError,
    EmissionCodegenErrorKind, ExpressionTypesView, ForeignQueryError, ForeignQueryErrorKind,
    LinkedProductEmissionError, LiteralValuesView, LivenessView, NativeProductPlan,
    NativeProductPlanningError, PackageInterfaceExportError, ProductEmissionError,
    ProductEmissionErrorKind, ProductEmissionInputs, ProductQueryError, ProductQueryErrorKind,
    ProductSourceGraph, RefinementsView, SemanticAvailability, SemanticQueryError,
    SemanticQueryErrorKind, SemanticSelectionsView, StorageFlowView, TestDiscovery,
};
pub use compiler_known::{
    CompilerKnownCatalogCheckError, CompilerKnownCatalogCheckReport, check_compiler_known_catalog,
};
pub use fact::{
    CancellationToken, FactCycle, FactQueryError, FactRuntimeError, FactRuntimeErrorKind,
    ImportedExecutableTemplateMismatch, ImportedQueryFailure, ImportedSemanticRecordKey,
    LocatedLoweringFailure, QueryPriority, SymbolCompletionError,
};
pub use profile::{
    CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileConfiguration,
    CompilationProfileContext, CompilationProfileDescriptorCatalog,
    CompilationProfileDurationDistribution, CompilationProfileEvent, CompilationProfileMetric,
    CompilationProfileMetricDescriptor, CompilationProfileMode,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileOutcome, CompilationProfileQueryDescriptor,
    CompilationProfileQueryStatistics, CompilationProfileReport,
    CompilationProfileSchedulerStatistics, CompilationProfileSubject,
    CompilationProfileSubjectKind, CompilationProfileTimeBreakdown, CompilationProfileUnit,
};
pub use request::{
    CompilationOptions, CompilationRequest, DependencyInterfaceInput,
    PackageInterfaceExportRequest, PackageSourceAuthority, SemanticAnalysisLimits,
};
pub use target::{SelectedTarget, SelectedTargetContext};
pub use worker::{WorkerBudget, WorkerBudgetError};
