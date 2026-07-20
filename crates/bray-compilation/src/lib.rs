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

pub use compilation::{Compilation, CompilationLoadError, PackageInterfaceExportError};
pub use compiler_known::{
    CompilerKnownCatalogCheckError, CompilerKnownCatalogCheckReport, check_compiler_known_catalog,
};
pub use fact::{
    CancellationToken, FactCycle, FactQueryError, ImportedSemanticFactKey, SymbolCompletionError,
    force_complete_symbol,
};
pub use request::{
    CompilationOptions, CompilationRequest, DependencyInterfaceInput, PackageInterfaceExportRequest,
};
pub use target::TargetAvailabilityFacts;
pub use worker::{WorkerBudget, WorkerBudgetError};
