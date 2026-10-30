//! Reference runtime artifact packaging and conformance.

mod archive;
mod bootstrap;
mod command;
mod contract;
mod entry_smoke;
mod execution;
mod native_index;
mod native_link;
mod numeric;
mod observation_smoke;
mod partition;
mod reuse;
mod smoke;
mod synchronous;

pub use bootstrap::build as build_bootstrap;

pub(crate) use command::{build_for_readiness, run, smoke_test_host};
