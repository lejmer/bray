mod access;
mod cleanup;
mod core;
mod deferred;
mod effects;
mod escape;
mod memory;
mod observations;
mod references;
mod reports;
mod snapshot;
mod terminal;
mod transfers;

pub(crate) use core::{StorageFlowCollector, check_storage_flow_with_graph};

pub(in crate::analysis) use snapshot::ReplacementState;
