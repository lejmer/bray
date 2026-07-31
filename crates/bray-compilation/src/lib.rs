//! Compiler entry points, requests, and lazy compiler facts.

#![forbid(unsafe_code)]

mod compilation;
mod compiler_known;
mod fact;
mod request;
mod target;
mod worker;

#[cfg(test)]
mod test_support;

pub use compilation::{
    CodegenFactError, Compilation, CompilationLoadError, EmissionCodegenError,
    EmissionCodegenErrorKind, PackageInterfaceExportError, ProductEmissionError,
    NativeProductFactError, NativeProductFacts, ProductEmissionErrorKind,
    CompletionCandidate, ProductEmissionInputs, ProductSourceGraph, SemanticAvailability,
};
pub use compiler_known::{
    CompilerKnownCatalogCheckError, CompilerKnownCatalogCheckReport, check_compiler_known_catalog,
};
pub use fact::{
    CancellationToken, FactCycle, FactQueryError, ImportedSemanticFactKey, QueryPriority,
    SymbolCompletionError,
};
pub use request::{
    CompilationOptions, CompilationRequest, DependencyInterfaceInput,
    PackageInterfaceExportRequest, SemanticAnalysisLimits,
};
pub use target::{SelectedTarget, SelectedTargetContext};
pub use worker::{WorkerBudget, WorkerBudgetError};
