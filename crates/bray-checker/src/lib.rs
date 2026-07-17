//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod outcome;
mod request;
mod service;

#[cfg(test)]
mod test_support;

pub use outcome::{CheckerOutcome, ControlFlowCheckResult};
pub use request::{
    ControlFactSelections, ControlFactSelectionsError, UnitCheckRequest, UnitCheckRequestError,
    UnitCheckRoot,
};
pub use service::{ControlFlowChecker, DefaultControlFlowChecker};
