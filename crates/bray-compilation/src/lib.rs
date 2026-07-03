//! Compiler entry points, compile requests, and pipeline orchestration.

#![forbid(unsafe_code)]

mod compilation;

pub use compilation::{
    Compilation, CompilationBuildError, CompilationOptions, CompilationRequest, WorkerBudget,
    WorkerBudgetError,
};
