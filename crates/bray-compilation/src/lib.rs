//! Compiler entry points, requests, and lazy compiler facts.

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
    BuildConfiguration, CodegenFactError, Compilation, CompilationLoadError, CompletionCandidate,
    EmissionCodegenError, EmissionCodegenErrorKind, NativeProductFactError, NativeProductFacts,
    PackageInterfaceExportError, ProductEmissionError, ProductEmissionErrorKind,
    ProductEmissionInputs, ProductSourceGraph, SemanticAvailability, TestDiscovery,
};
pub use compiler_known::{
    CompilerKnownCatalogCheckError, CompilerKnownCatalogCheckReport, check_compiler_known_catalog,
};
pub use fact::{
    CancellationToken, FactCycle, FactQueryError, ImportedSemanticFactKey, QueryPriority,
    SymbolCompletionError,
};
pub use profile::{
    CompilationProfileAggregation, CompilationProfileCategory, CompilationProfileConfiguration,
    CompilationProfileContext, CompilationProfileDescriptorCatalog, CompilationProfileEvent,
    CompilationProfileMetric, CompilationProfileMetricDescriptor, CompilationProfileMode,
    CompilationProfileOperationDescriptor, CompilationProfileOperationStatistics,
    CompilationProfileOutcome, CompilationProfileQueryDescriptor,
    CompilationProfileQueryStatistics, CompilationProfileReport, CompilationProfileSubject,
    CompilationProfileSubjectKind, CompilationProfileTimeBreakdown, CompilationProfileUnit,
};
pub use request::{
    CompilationOptions, CompilationRequest, DependencyInterfaceInput,
    PackageInterfaceExportRequest, PackageSourceAuthority, SemanticAnalysisLimits,
};
pub use target::{SelectedTarget, SelectedTargetContext};
pub use worker::{WorkerBudget, WorkerBudgetError};
