mod authority;
mod check;
mod decision;
mod memory;
mod model;

pub use crate::storage::{closed_type_is_copyable, type_is_copyable, type_is_copyable_in_context};
pub(crate) use check::check_storage_flow_with_graph;
pub(crate) use model::StorageFlowInput;

pub(in crate::analysis) use model::{ReachedInputWrites, StorageFlowState};
