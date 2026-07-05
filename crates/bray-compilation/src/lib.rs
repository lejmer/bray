//! Compiler entry points, requests, and lazy compiler facts.

#![forbid(unsafe_code)]

mod compilation;
mod request;
mod worker;

pub use compilation::{Compilation, CompilationLoadError};
pub use request::{CompilationOptions, CompilationRequest};
pub use worker::{WorkerBudget, WorkerBudgetError};
