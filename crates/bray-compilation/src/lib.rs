//! Compiler entry points, requests, and lazy compiler facts.

#![forbid(unsafe_code)]

mod compilation;
mod fact;
mod request;
mod target;
mod worker;

pub use compilation::{Compilation, CompilationLoadError};
pub use fact::{
    CancellationToken, CompilationFactKey, FactCycle, FactQueryError, SymbolCompletionError,
    SymbolFactKey, force_complete_symbol,
};
pub use request::{CompilationOptions, CompilationRequest};
pub use target::TargetAvailabilityFacts;
pub use worker::{WorkerBudget, WorkerBudgetError};
