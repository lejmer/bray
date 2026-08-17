mod binding;
mod cache;
mod completion;
mod compute;
mod constant;
mod contract;
mod declaration_body;
mod directive;
mod environment;
mod imported;
mod module_surface;
mod surface;
mod static_storage;
mod template;

#[cfg(test)]
mod test_support;

pub(in crate::compilation) use cache::CompilationSymbolSemantics;
pub(in crate::compilation) use contract::bind_declared_trusted_capabilities;
pub(in crate::compilation) use directive::bind_module_part_directives_for_selection;
pub(in crate::compilation) use environment::{
    has_visible_generic_parameters, type_binder, type_scope, visible_generic_const_parameters,
};
pub(in crate::compilation) use imported::{
    imported_declaration_template, imported_declaration_template_at, imported_implementation,
};
