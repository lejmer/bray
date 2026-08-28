//! Reference runtime artifact packaging and conformance.

mod command;
mod native_link;
mod partition;
mod reuse;
mod smoke;

pub(crate) use command::{build_for_readiness, run, smoke_test_host};
