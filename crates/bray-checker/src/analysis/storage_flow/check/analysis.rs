mod access;
mod core;
mod effects;
mod escape;
mod memory;
mod observations;
mod references;
mod snapshot;

pub(crate) use core::{StorageFlowCollector, check_storage_flow_with_graph};

pub(in crate::analysis) use snapshot::ReplacementState;
