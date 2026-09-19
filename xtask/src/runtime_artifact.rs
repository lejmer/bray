//! Reference runtime artifact packaging and conformance.

mod archive;
mod bootstrap;
mod command;
mod contract;
mod native_link;
mod observation_smoke;
mod partition;
mod reuse;
mod smoke;

pub use bootstrap::build as build_bootstrap;

pub(crate) use command::{build_for_readiness, run, smoke_test_host};
