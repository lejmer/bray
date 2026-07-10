//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod cancellation;
mod outcome;
mod request;
mod service;

pub use cancellation::CheckerCancellation;
pub use outcome::{CheckerOutcome, UnitCheckConclusions};
pub use request::UnitCheckRequest;
pub use service::UnitChecker;
