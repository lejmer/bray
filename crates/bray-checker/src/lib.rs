//! Semantic checking for bound Bray programs.

#![forbid(unsafe_code)]

mod analysis;
mod outcome;
mod request;
mod service;
mod storage;

#[cfg(test)]
mod test_support;

pub use outcome::{CheckerOutcome, ControlFlowCheckResult, StorageCheckResult};
pub use request::{UnitCheckRequest, UnitCheckRequestError, UnitCheckRoot};
pub use service::{
    ControlFlowChecker, DefaultControlFlowChecker, DefaultStorageChecker, StorageChecker,
};
pub use storage::StorageCheckError;
