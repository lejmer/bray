mod context;
mod error;
mod symbol;
mod value_type;
mod value_type_surface;

pub(super) use context::CompilationBindingContext;
pub(in crate::compilation) use error::{
    BindingQueryResult, binding_error, binding_query_error, callable_signature_binding_error,
    semantic_contract_binding_error, semantic_query_binding_error,
    symbol_query_contract_binding_error,
};
pub(in crate::compilation) use symbol::{
    BodyAnalysis, CompilationSymbolSemantics, anonymous_result_dependencies,
    bind_callable_clause_unit, bind_declared_execution_requirements,
    bind_declared_trusted_capabilities, bind_module_part_directives_for_selection,
    callable_requirement_source, checked_callable_type_contracts, checked_source_expression,
    default_result_dependencies, expression_result_dependencies,
    extend_dependency_contract_with_statics, generic_parameter_ids, has_visible_generic_parameters,
    imported_declaration_template, imported_declaration_template_at, imported_implementation,
    source_body_semantics, static_dependencies_from_contract, static_dependency_root,
    static_result_dependencies, type_binder, type_scope, visible_generic_parameters,
};
pub(in crate::compilation) use value_type::bind_declared_value_type_templates;
