pub(super) fn format_english_checker_failure(
    failure: bray_diagnostics::DiagnosticCheckerFailure,
) -> String {
    use bray_diagnostics::DiagnosticCheckerFailure as Failure;

    let detail = match failure {
        Failure::MissingSource { source_id } => {
            let _ = source_id;

            "source text required by this product was unavailable"
        }
        Failure::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        } => {
            let _ = (source_id, expected, actual);

            "the source-text revision does not match this compilation"
        }
        Failure::InvalidSourceRange { span } => {
            let _ = span;

            "a source range lies outside the available source text"
        }
        Failure::SemanticQueryUnavailable { symbol, query } => {
            return super::format_internal_compiler_error(format!(
                "{} was unavailable for the highlighted {} declaration",
                format_checker_query(query),
                format_checker_symbol_kind(symbol.kind()),
            ));
        }
        Failure::SemanticValueUnavailable => {
            "declaration information required by this product was unavailable"
        }
        Failure::SemanticValue(failure) => {
            return super::artifact::format_english_semantic_value_failure(failure);
        }
        Failure::AtomicRepresentationTypeUnavailable => {
            "the selected atomic value has no available representation type"
        }
        Failure::AtomicRepresentationArgumentsUnavailable => {
            "the selected atomic value has no available representation arguments"
        }
        Failure::AtomicInitializerArgumentUnavailable => {
            "the atomic initializer argument has no compile-time value"
        }
        Failure::AtomicInitializerResultUnavailable => {
            "the atomic initializer result cannot be represented as a compile-time value"
        }
        Failure::UninitInitializerResultUnavailable => {
            "the uninitialized-storage initializer result cannot be represented as a compile-time value"
        }
        Failure::ImportedExecutableTemplateMismatch => {
            "an imported native operation does not match its compiled definition"
        }
        Failure::CompilerKnownRepresentationUnavailable(role) => {
            return super::format_internal_compiler_error(format!(
                "the language-defined `{}` type required for the selected target was unavailable",
                format_compiler_known_representation(role),
            ));
        }
        Failure::InvalidExpressionTypeInput { expression } => {
            let _ = expression;

            return super::format_internal_compiler_error(
                "the highlighted expression belongs to a different source body than its type",
            );
        }
        Failure::InvalidSemanticSelectionInput => {
            "the operation or call for an expression could not be selected"
        }
        Failure::InvalidLiteralValueInput => {
            "the literal belongs to a different source body than its value"
        }
        Failure::InvalidConstantEvaluationInput => {
            "constant evaluation received incompatible source information"
        }
        Failure::InvalidPatternCheckInput => {
            "pattern checking received incompatible source information"
        }
        Failure::InvalidStoragePlan => {
            "the local values used by a source body could not be arranged"
        }
        Failure::InvalidLiveness => "value lifetimes could not be determined",
        Failure::InvalidRefinementInput => {
            "condition and pattern implications could not be tracked"
        }
        Failure::RefinementCapacityUnrepresentable => {
            "condition and pattern implications exceed the supported internal capacity"
        }
        Failure::RefinementStorageUnavailable => {
            "memory for condition and pattern implications was unavailable"
        }
        Failure::StorageFlow(failure) => {
            return super::format_internal_compiler_error(format_storage_flow_failure(failure));
        }
        Failure::InvalidStorageOperation {
            expression,
            access,
            status,
        } => {
            let _ = access;

            return super::format_internal_compiler_error(format!(
                "a local-value access by the highlighted {} has status {}",
                format_checker_node_kind(expression.kind()),
                format_storage_operation_status(status),
            ));
        }
        Failure::InvalidBodySemantics => "analysis results for a source body are incompatible",
        Failure::InvalidBoundNode { node } => {
            return super::format_internal_compiler_error(format!(
                "the highlighted {} required to compile its source body was unavailable",
                format_checker_node_kind(node.kind()),
            ));
        }
        Failure::ExpressionTypeCapacityExceeded => {
            "the expression types in a source body exceed the supported internal capacity"
        }
        Failure::InvalidUnitView("semantic_context_mismatch") => {
            "analysis results belong to a different source declaration or body"
        }
        Failure::InvalidUnitView(reason) => {
            return super::format_internal_compiler_error(format!(
                "a source declaration or body failed its `{reason}` consistency check"
            ));
        }
    };

    super::format_internal_compiler_error(detail)
}

fn format_storage_flow_failure(failure: bray_diagnostics::DiagnosticStorageFlowFailure) -> String {
    use bray_diagnostics::DiagnosticStorageFlowFailure as Failure;

    match failure {
        Failure::IncompatibleInput {
            input,
            expected_kind,
            actual_kind,
            ..
        } => format!(
            "{} for the highlighted {} belongs to a different {}",
            format_storage_flow_input(input),
            format_bound_unit_kind(actual_kind),
            format_bound_unit_kind(expected_kind),
        ),
        Failure::FlowConstruction(reason) => format!(
            "the ownership result for this source body could not be retained because {}",
            format_storage_flow_construction(reason),
        ),
        Failure::ForeignDependencyContract => {
            "a selected call or iteration dependency belongs to another source body".to_owned()
        }
        Failure::DependencyContractsConstruction(reason) => format!(
            "this source body's value dependencies could not be retained because {}",
            format_dependency_contract_construction(reason),
        ),
        Failure::AsyncConstruction(reason) => format!(
            "this source body's asynchronous behavior could not be retained because {}",
            format_async_construction(reason),
        ),
        Failure::MissingAwaitDependencyContract { expression } => format!(
            "the values required by the highlighted {} were not determined",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::MissingDependencyContract { expression, .. } => format!(
            "the highlighted {} refers to a value dependency that does not exist in its source body",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::CallableParameterCountMismatch {
            callable,
            signature_parameters,
            type_parameters,
        } => format!(
            "the highlighted {} declaration has {signature_parameters} declared parameters but {type_parameters} parameter modes",
            format_checker_symbol_kind(callable.kind()),
        ),
        Failure::CallableTypeNotCallable { callable } => format!(
            "the highlighted {} declaration has a non-callable type",
            format_checker_symbol_kind(callable.kind()),
        ),
        Failure::MissingBorrowCapability { unit, borrow } => {
            let _ = (unit, borrow);

            "a borrow operation required to check whether a value escapes the highlighted source body was unavailable".to_owned()
        }
        Failure::MissingExitOrigin { exit } => format!(
            "the source location for the highlighted {} was unavailable",
            format_checker_node_kind(exit.kind()),
        ),
        Failure::MissingBlock { block } => format!(
            "the highlighted {} targeted by a control-flow transfer was unavailable",
            format_checker_node_kind(block.kind()),
        ),
        Failure::MissingStorageAccess { unit, access } => {
            let _ = (unit, access);

            "a local-value access required by the highlighted source body was unavailable"
                .to_owned()
        }
        Failure::MissingStorageIdentity { unit, identity } => {
            let _ = (unit, identity);

            "a local value required by the highlighted source body was unavailable".to_owned()
        }
        Failure::MissingStorageSymbolName { symbol } => format!(
            "the declared name of the highlighted {} declaration was unavailable while describing a local-value access",
            format_checker_symbol_kind(symbol.kind()),
        ),
        Failure::UnbalancedScopes {
            open_scope: Some(scope),
        } => format!(
            "the highlighted {} remained open after examining its source body's lexical scopes",
            format_checker_node_kind(scope.kind()),
        ),
        Failure::UnbalancedScopes { open_scope: None } => {
            "block boundaries are inconsistent in this source body".to_owned()
        }
        Failure::MissingPattern { pattern } => format!(
            "the highlighted {} was unavailable while examining its source body's lexical scopes",
            format_checker_node_kind(pattern.kind()),
        ),
    }
}

fn format_storage_flow_input(input: &str) -> &str {
    match input {
        "expression_types" => "expression types",
        "semantic_selections" => "selected calls and operations",
        "storage_plan" => "local-value layout",
        "liveness" => "value lifetimes",
        "refinements" => "known conditions and matched patterns",
        "memory_operations" => "memory operations",
        "storage_flow" => "ownership results",
        "dependency_contracts" => "value dependencies",
        _ => "ownership information",
    }
}

fn format_bound_unit_kind(kind: &str) -> &str {
    match kind {
        "callable_body" => "callable body",
        "anonymous_callable" => "anonymous callable",
        "runtime_default" => "runtime default",
        "constant_template" => "constant definition",
        "embedded_constant" => "embedded constant",
        "predicate_definition" => "predicate definition",
        "constraint" => "constraint",
        "contract_clause" => "contract clause",
        "target_gate" => "target condition",
        _ => "source body",
    }
}

fn format_storage_flow_construction(reason: &str) -> &str {
    match reason {
        "foreign_unit" => "one recorded operation belongs to another source body",
        "duplicate_suspension" => "one await or yield expression has two saved states",
        "duplicate_memory_operation" => "one expression has two recorded memory operations",
        _ => "one ownership operation could not be represented",
    }
}

fn format_dependency_contract_construction(reason: &str) -> &str {
    match reason {
        "foreign_storage_plan" => "its local-value layout belongs to another source body",
        "invalid_expression" => "a dependency refers to a missing expression",
        "invalid_access" => "a dependency refers to a missing value access",
        "invalid_borrow" => "a dependency refers to a missing borrow",
        "foreign_contract" => "a dependency refers to another source body",
        "contract_capacity_exceeded" => {
            "the number of distinct dependencies exceeds the supported limit"
        }
        _ => "one value dependency could not be represented",
    }
}

fn format_async_construction(reason: &str) -> &str {
    match reason {
        "foreign_unit" => "one recorded operation belongs to another source body",
        _ => "one asynchronous operation could not be represented",
    }
}

fn format_checker_query(query: &str) -> &str {
    match query {
        "members" => "its member declarations",
        "imports" => "its imported declarations",
        "directives" => "its declaration directives",
        "generic_parameters" => "its generic parameters",
        "generic_declaration_template" => "its generic declaration",
        "generic_constraints" => "its checked generic constraints",
        "callable_signature" => "its callable signature",
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

fn format_checker_symbol_kind(kind: &str) -> &str {
    match kind {
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

fn format_checker_node_kind(kind: &str) -> &str {
    match kind {
        "callable_body" => "callable body",
        "expression" => "expression",
        "await_expression" => "await expression",
        "block" => "block",
        "pattern" => "pattern",
        _ => "source construct",
    }
}

fn format_compiler_known_representation(role: &str) -> &str {
    match role {
        "ScalarBool" => "bool",
        "ScalarChar" => "char",
        "ScalarI8" => "i8",
        "ScalarI16" => "i16",
        "ScalarI32" => "i32",
        "ScalarI64" => "i64",
        "ScalarI128" => "i128",
        "ScalarU8" => "u8",
        "ScalarU16" => "u16",
        "ScalarU32" => "u32",
        "ScalarU64" => "u64",
        "ScalarU128" => "u128",
        "ScalarIsize" => "isize",
        "ScalarUsize" => "usize",
        "ScalarR16" => "r16",
        "ScalarR32" => "r32",
        "ScalarR64" => "r64",
        "ScalarR128" => "r128",
        "ScalarC32" => "c32",
        "ScalarC64" => "c64",
        "ScalarC128" => "c128",
        "ScalarC256" => "c256",
        _ => "language-defined representation",
    }
}

fn format_storage_operation_status(status: &str) -> &str {
    match status {
        "unreachable" => "unreachable",
        "valid" => "valid",
        "recovered" => "recovered from an earlier source error",
        "uninitialized" => "an access to an uninitialized value",
        "moved" => "an access after ownership moved",
        "conflicting_borrow" => "a conflicting borrow",
        "missing_mutation_authority" => "a mutation without mutable access",
        "missing_ownership" => "an operation without ownership",
        "inactive_projection" => "an access through an inactive union variant",
        "not_copyable" => "an implicit copy of a non-copyable value",
        _ => "an unrecognized state",
    }
}
