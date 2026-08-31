pub(super) fn format_english_checker_failure(
    failure: bray_diagnostics::DiagnosticCheckerFailure,
) -> String {
    use bray_diagnostics::DiagnosticCheckerFailure as Failure;

    let message = match failure {
        Failure::MissingSource { source_id } => {
            return format!(
                "an internal compiler error prevented Bray from finding source snapshot #{} required by this product",
                source_id.raw(),
            );
        }
        Failure::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        } => {
            return format!(
                "an internal compiler error expected revision {} of source snapshot #{} but found revision {} while compiling this product",
                expected.raw(),
                source_id.raw(),
                actual.raw(),
            );
        }
        Failure::InvalidSourceRange { span } => {
            return format!(
                "an internal compiler error retained bytes {} through {} outside source snapshot #{}",
                span.start().bytes(),
                span.end().bytes(),
                span.source_id().raw(),
            );
        }
        Failure::SemanticQueryUnavailable { symbol, query } => {
            return format!(
                "an internal compiler error could not obtain {} for {} declaration #{}",
                format_checker_query(query),
                format_checker_symbol_kind(symbol.kind()),
                symbol.ordinal(),
            );
        }
        Failure::SemanticValueUnavailable => {
            "an internal compiler error prevented Bray from retaining declaration information required by this product"
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
            return format!(
                "an internal compiler error prevented Bray from finding the language-defined `{}` type required for the selected target",
                format_compiler_known_representation(role),
            );
        }
        Failure::InvalidExpressionTypeInput { expression } => {
            return format!(
                "an internal compiler error associated expression #{} in source body #{} with the wrong source body while determining its type",
                expression.ordinal(),
                expression.unit(),
            );
        }
        Failure::InvalidSemanticSelectionInput => {
            "an internal compiler error prevented Bray from selecting the operation or call for an expression"
        }
        Failure::InvalidLiteralValueInput => {
            "an internal compiler error associated a literal with the wrong source body while determining its value"
        }
        Failure::InvalidConstantEvaluationInput => {
            "an internal compiler error supplied incompatible source information while evaluating a constant expression"
        }
        Failure::InvalidPatternCheckInput => {
            "an internal compiler error supplied incompatible source information while checking a pattern"
        }
        Failure::InvalidStoragePlan => {
            "an internal compiler error prevented Bray from arranging the local values used by a source body"
        }
        Failure::InvalidLiveness => {
            "an internal compiler error prevented Bray from determining how long a value remains usable"
        }
        Failure::InvalidRefinementInput => {
            "an internal compiler error prevented Bray from tracking what a condition or pattern proves about a value"
        }
        Failure::RefinementCapacityUnrepresentable => {
            "an internal compiler limit prevented Bray from retaining everything a condition or pattern proves about a value"
        }
        Failure::RefinementStorageUnavailable => {
            "the compiler could not allocate memory needed to track what a condition or pattern proves about a value"
        }
        Failure::StorageFlow(failure) => return format_storage_flow_failure(failure),
        Failure::InvalidStorageOperation {
            expression,
            access,
            status,
        } => {
            return format!(
                "an internal compiler error produced `{status}` while analyzing storage access #{access} for {} #{} in source body #{}",
                format_checker_node_kind(expression.kind()),
                expression.ordinal(),
                expression.unit(),
            );
        }
        Failure::InvalidBodySemantics => {
            "an internal compiler error found incompatible analysis results for a source body"
        }
        Failure::InvalidBoundNode { node } => {
            return format!(
                "an internal compiler error lost {} #{} required to compile source body #{}",
                format_checker_node_kind(node.kind()),
                node.ordinal(),
                node.unit(),
            );
        }
        Failure::ExpressionTypeCapacityExceeded => {
            "an internal compiler limit prevented Bray from determining every expression type in a source body"
        }
        Failure::InvalidUnitView("semantic_context_mismatch") => {
            "an internal compiler error associated analysis results with the wrong source declaration or body"
        }
        Failure::InvalidUnitView(reason) => {
            return format!(
                "an internal compiler error could not use a source declaration or body because its `{reason}` consistency check failed"
            );
        }
    };

    message.to_owned()
}

fn format_storage_flow_failure(
    failure: bray_diagnostics::DiagnosticStorageFlowFailure,
) -> String {
    use bray_diagnostics::DiagnosticStorageFlowFailure as Failure;

    match failure {
        Failure::IncompatibleInput {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => format!(
            "an internal compiler error associated {} for source body #{} ({}) with source body #{} ({})",
            format_storage_flow_input(input),
            actual_unit,
            format_bound_unit_kind(actual_kind),
            expected_unit,
            format_bound_unit_kind(expected_kind),
        ),
        Failure::FlowConstruction(reason) => format!(
            "an internal compiler error could not retain the ownership result for this source body because {}",
            format_storage_flow_construction(reason),
        ),
        Failure::ForeignDependencyContract => {
            "an internal compiler error selected a call or iteration dependency belonging to another source body".to_owned()
        }
        Failure::DependencyContractsConstruction(reason) => format!(
            "an internal compiler error could not retain this source body's value dependencies because {}",
            format_dependency_contract_construction(reason),
        ),
        Failure::AsyncConstruction(reason) => format!(
            "an internal compiler error could not retain this source body's asynchronous behavior because {}",
            format_async_construction(reason),
        ),
        Failure::MissingAwaitDependencyContract { expression } => format!(
            "an internal compiler error did not determine which values await expression #{} in source body #{} requires",
            expression.ordinal(),
            expression.unit(),
        ),
        Failure::MissingDependencyContract {
            expression,
            contract_unit,
            contract,
        } => format!(
            "an internal compiler error associated await expression #{} in source body #{} with missing dependency #{} from source body #{}",
            expression.ordinal(),
            expression.unit(),
            contract,
            contract_unit,
        ),
        Failure::CallableParameterCountMismatch {
            callable,
            signature_parameters,
            type_parameters,
        } => format!(
            "an internal compiler error retained {signature_parameters} declared parameters but {type_parameters} parameter modes for {} declaration #{}",
            format_checker_symbol_kind(callable.kind()),
            callable.ordinal(),
        ),
        Failure::CallableTypeNotCallable { callable } => format!(
            "an internal compiler error retained a non-callable type for {} declaration #{}",
            format_checker_symbol_kind(callable.kind()),
            callable.ordinal(),
        ),
        Failure::MissingBorrowCapability { unit, borrow } => format!(
            "an internal compiler error lost borrow #{} while checking whether a value escapes source body #{}",
            borrow, unit,
        ),
        Failure::MissingExitOrigin { exit } => format!(
            "an internal compiler error lost the source location for {} #{} in source body #{}",
            format_checker_node_kind(exit.kind()),
            exit.ordinal(),
            exit.unit(),
        ),
        Failure::MissingBlock { block } => format!(
            "an internal compiler error lost block #{} targeted by a control-flow transfer in source body #{}",
            block.ordinal(),
            block.unit(),
        ),
        Failure::MissingStorageAccess { unit, access } => format!(
            "an internal compiler error lost value access #{} required by source body #{}",
            access, unit,
        ),
        Failure::MissingStorageIdentity { unit, identity } => format!(
            "an internal compiler error lost local value #{} required by source body #{}",
            identity, unit,
        ),
        Failure::MissingStorageSymbolName { symbol } => format!(
            "an internal compiler error lost the declared name of {} declaration #{} while describing an invalid value access",
            format_checker_symbol_kind(symbol.kind()),
            symbol.ordinal(),
        ),
        Failure::UnbalancedScopes { open_scope: Some(scope) } => format!(
            "an internal compiler error left block #{} open while examining the lexical scopes of source body #{}",
            scope.ordinal(),
            scope.unit(),
        ),
        Failure::UnbalancedScopes { open_scope: None } => {
            "an internal compiler error encountered inconsistent block boundaries while examining this source body".to_owned()
        }
        Failure::MissingPattern { pattern } => format!(
            "an internal compiler error lost pattern #{} while examining the lexical scopes of source body #{}",
            pattern.ordinal(),
            pattern.unit(),
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
        input => input,
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
        kind => kind,
    }
}

fn format_storage_flow_construction(reason: &str) -> &str {
    match reason {
        "foreign_unit" => "one recorded operation belongs to another source body",
        "duplicate_suspension" => "one await or yield expression has two saved states",
        "duplicate_memory_operation" => "one expression has two recorded memory operations",
        reason => reason,
    }
}

fn format_dependency_contract_construction(reason: &str) -> &str {
    match reason {
        "foreign_storage_plan" => "its local-value layout belongs to another source body",
        "invalid_expression" => "a dependency refers to a missing expression",
        "invalid_access" => "a dependency refers to a missing value access",
        "invalid_borrow" => "a dependency refers to a missing borrow",
        "foreign_contract" => "a dependency refers to another source body",
        "contract_capacity_exceeded" => "the number of distinct dependencies exceeds the compiler's supported limit",
        reason => reason,
    }
}

fn format_async_construction(reason: &str) -> &str {
    match reason {
        "foreign_unit" => "one recorded operation belongs to another source body",
        reason => reason,
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
        query => query,
    }
}

fn format_checker_symbol_kind(kind: &str) -> &str {
    match kind {
        "callable_overload" | "implementation_overload" => "overload",
        "callable_parameter" => "parameter",
        "inherent_implementation" | "named_trait_implementation"
        | "unnamed_trait_implementation" => "implementation",
        "struct_field" => "struct field",
        "trait_callable_fulfillment" | "trait_callable_member" | "type_callable_member" => {
            "callable member"
        }
        "trait_constant_fulfillment" | "trait_constant_member" => "constant member",
        "trait_type_valued_fulfillment" | "trait_type_valued_member" => "type member",
        "union_payload_field" => "union payload field",
        "union_variant" => "union variant",
        kind => kind,
    }
}

fn format_checker_node_kind(kind: &str) -> &str {
    match kind {
        "callable_body" => "callable body",
        kind => kind,
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
        role => role,
    }
}
