mod context;
mod error;
mod symbol;
mod value_type;
mod value_type_surface;

pub(super) use context::CompilationBinderFacts;
pub(in crate::compilation) use error::binder_fact_error;
pub(in crate::compilation) use symbol::{
    CompilationSymbolFacts, bind_declared_trusted_capabilities,
    bind_module_part_directives_for_selection, has_visible_generic_parameters,
    imported_declaration_template, imported_implementation, type_scope,
};
pub(in crate::compilation) use value_type::bind_declared_value_type_templates;
