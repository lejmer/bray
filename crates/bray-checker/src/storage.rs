mod copyability;
mod expression;
pub(crate) mod observation;
mod pattern;
mod plan;
mod projection;
mod protocol;
mod scope;

pub(crate) use bray_bound_tree::StorageScopeOwners;

pub(crate) use plan::plan_storage;
pub use projection::projected_storage_type;
pub(crate) use projection::{
    dependency_projection, dependency_projections, projected_value_type, reference_path,
    resolve_storage_projections, storage_projections,
};
pub(crate) use protocol::selected_storage_protocol_call;
pub(crate) use scope::{
    local_initialization_destinations, pattern_binding_sources, storage_scope_owners,
    value_transfer_bindings,
};

pub use copyability::{closed_type_is_copyable, type_is_copyable, type_is_copyable_in_context};
