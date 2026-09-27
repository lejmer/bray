mod callable_demand;
mod core;
mod validation;

pub use callable_demand::{
    DemandedCallableInstance, demanded_callable_instance_for_call, demanded_callable_instances,
    demanded_callable_instances_for_mir,
};
pub use core::CodegenMappings;
pub use validation::{
    demanded_debug_sources, demanded_runtime_references, demanded_runtime_references_for_mir,
    frame_creation_runtime_role, mapped_runtime_references, mapped_symbol_runtime_roles,
};
