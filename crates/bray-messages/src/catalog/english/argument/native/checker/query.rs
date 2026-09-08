pub(super) fn format_checker_query(query: &str) -> &str {
    match query {
        "members" => "its member declarations",
        "imports" => "its imported declarations",
        "directives" => "its declaration directives",
        "generic_parameters" => "its generic parameters",
        "generic_declaration_template" => "its generic declaration",
        "generic_constraints" => "its checked generic constraints",
        "callable_signature" => "its callable signature",
        "callable_conditions" => "its callable conditions",
        "callable_contracts" => "its callable contracts",
        "callable_contract_template" => "its callable contract declaration",
        "predicate_signature_template" => "its predicate signature",
        "callable_contract_type" => "its callable contract type",
        "constant_declared_type" => "its declared constant type",
        "constant_definition" => "its checked constant definition",
        "static_instance_template" => "its static value definition",
        "callable_parameter_default" => "its checked parameter default",
        "unevaluated_default_template" => "its default value expression",
        "struct_field_type" => "its field type",
        "type_member_value" => "its type member value",
        "struct_field_default" => "its checked field default",
        "union_payload_field_type" => "its payload field type",
        "union_payload_field_default" => "its checked payload field default",
        "predicate_definition" => "its checked predicate definition",
        "union_variant_payload" => "its variant payload",
        "implementation_subject" => "its implementation subject",
        "implemented_trait_application" => "its implemented trait",
        "implementation_head_template" => "its implementation declaration",
        "implementation_coherence" => "its implementation coherence information",
        "overload_arms" => "its resolved overload alternatives",
        "overload_signature_template" => "its overload signatures",
        _ => "required declaration information",
    }
}

pub(super) fn format_checker_symbol_kind(kind: &str) -> &str {
    match kind {
        "function" => "function",
        "predicate" | "trait_predicate_member" | "trait_predicate_fulfillment" => "predicate",
        "callable_contract" => "callable contract",
        "anonymous_callable" => "anonymous callable",
        "constructor" => "constructor",
        "finalizer" | "trait_finalizer_requirement" => "finalizer",
        "destructor" | "trait_destructor_requirement" => "destructor",
        "scope_enter" | "trait_scope_enter_requirement" | "trait_scope_enter_fulfillment" => {
            "`enter` lifecycle"
        }
        "scope_exit" | "trait_scope_exit_requirement" | "trait_scope_exit_fulfillment" => {
            "`exit` lifecycle"
        }
        "callable_overload" | "implementation_overload" => "overload",
        "callable_parameter" => "parameter",
        "module" => "module",
        "inherent_implementation"
        | "named_trait_implementation"
        | "unnamed_trait_implementation" => "implementation",
        "struct_field" => "struct field",
        "trait_callable_fulfillment" | "trait_callable_member" | "type_callable_member" => {
            "callable member"
        }
        "trait_constant_fulfillment" | "trait_constant_member" => "constant member",
        "trait_type_valued_fulfillment" | "trait_type_valued_member" => "type member",
        "union_payload_field" => "union payload field",
        "union_variant" => "union variant",
        _ => "source",
    }
}

#[cfg(test)]
mod tests {
    use super::super::format_english_checker_failure;
    use bray_diagnostics::{DiagnosticCheckerFailure, DiagnosticCheckerSymbol};

    #[test]
    fn callable_condition_query_failures_identify_the_source_declaration() {
        for (kind, description) in [
            ("function", "function"),
            ("predicate", "predicate"),
            ("trait_predicate_member", "predicate"),
            ("trait_predicate_fulfillment", "predicate"),
            ("callable_contract", "callable contract"),
            ("anonymous_callable", "anonymous callable"),
            ("constructor", "constructor"),
            ("finalizer", "finalizer"),
            ("trait_finalizer_requirement", "finalizer"),
            ("destructor", "destructor"),
            ("trait_destructor_requirement", "destructor"),
            ("scope_enter", "`enter` lifecycle"),
            ("trait_scope_enter_requirement", "`enter` lifecycle"),
            ("trait_scope_enter_fulfillment", "`enter` lifecycle"),
            ("scope_exit", "`exit` lifecycle"),
            ("trait_scope_exit_requirement", "`exit` lifecycle"),
            ("trait_scope_exit_fulfillment", "`exit` lifecycle"),
        ] {
            let message = format_english_checker_failure(
                DiagnosticCheckerFailure::SemanticQueryUnavailable {
                    symbol: DiagnosticCheckerSymbol::new(kind, 17),
                    query: "callable_conditions",
                },
            );

            assert!(message.contains("internal compiler error"), "{message}");
            assert!(message.contains("callable conditions"), "{message}");

            assert!(
                message.contains(&format!("highlighted {description} declaration")),
                "{message}"
            );

            assert!(!message.contains("callable_conditions"), "{message}");
            assert!(!message.contains("17"), "{message}");
        }
    }
}
