//! Reference runtime artifact packaging and conformance.

mod command;
mod smoke;

pub(crate) use command::{build_for_readiness, run, smoke_test_host};
