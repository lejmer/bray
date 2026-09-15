mod query;
mod static_initializer;
mod support;

pub(super) use support::{
    call_parameter_values, constant_callable_root, substitute_expression_types,
};
pub(in crate::compilation) use support::{
    collect_constant_references, collect_constant_references_from, constant_definition_id,
};
