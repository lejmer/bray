mod binding;
mod cache;
mod completion;
mod compute;
mod condition;
mod constant;
mod contract;
mod declaration_body;
mod directive;
mod environment;
mod guarantee;
mod imported;
mod initializer;
mod module_surface;
mod static_storage;
mod surface;
mod template;

#[cfg(test)]
mod test_support;

pub(in crate::compilation) use binding::binder_error;
pub(in crate::compilation) use cache::CompilationSymbolSemantics;
pub(in crate::compilation::binder) use condition::bind_callable_type_contract;
pub(in crate::compilation) use contract::{
    bind_declared_callable_phase_behaviors, bind_declared_execution_requirements,
    bind_declared_trusted_capabilities,
};
pub(in crate::compilation) use declaration_body::predicate_definition_key;
pub(in crate::compilation) use directive::bind_module_part_directives_for_selection;
pub(in crate::compilation) use environment::{
    generic_parameter_ids, has_visible_generic_parameters, type_binder, type_scope,
    visible_generic_const_parameters, visible_generic_parameters,
};
pub(in crate::compilation) use imported::{
    imported_declaration_template, imported_declaration_template_at, imported_implementation,
};
