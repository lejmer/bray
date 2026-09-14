pub(super) fn format_english_checker_failure(
    failure: bray_diagnostics::DiagnosticCheckerFailure,
) -> String {
    use bray_diagnostics::DiagnosticCheckerFailure as Failure;

    if let Some(message) = format_contextual_checker_failure(failure) {
        return message;
    }

    let (is_internal, message) = match failure {
        Failure::MissingSource { source_id } => {
            let _ = source_id;

            (true, "source text required by this product was unavailable")
        }
        Failure::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        } => {
            let _ = (source_id, expected, actual);

            (
                true,
                "the wrong source-text revision was used while compiling this product",
            )
        }
        Failure::InvalidSourceRange { span } => {
            let _ = span;

            (
                true,
                "a source range extended beyond the available source text",
            )
        }
        Failure::SemanticQueryUnavailable { symbol, query } => {
            return super::format_internal_compiler_error(format!(
                "could not obtain {} for the highlighted {} declaration",
                format_checker_query(query),
                format_checker_symbol_kind(symbol.kind()),
            ));
        }
        Failure::SemanticValueUnavailable => (
            true,
            "declaration information required by this product was unavailable",
        ),
        Failure::GenericSubstitution(failure) => {
            return super::format_internal_compiler_error(format_generic_substitution_failure(
                failure,
            ));
        }
        Failure::SemanticValue(failure) => {
            return super::artifact::format_english_semantic_value_failure(failure);
        }
        Failure::AtomicRepresentationTypeUnavailable => (
            false,
            "the selected atomic value has no available representation type",
        ),
        Failure::AtomicRepresentationArgumentsUnavailable => (
            false,
            "the selected atomic value has no available representation arguments",
        ),
        Failure::AtomicInitializerArgumentUnavailable => (
            false,
            "the atomic initializer argument has no compile-time value",
        ),
        Failure::AtomicInitializerResultUnavailable => (
            false,
            "the atomic initializer result cannot be represented as a compile-time value",
        ),
        Failure::UninitInitializerResultUnavailable => (
            false,
            "the uninitialized-storage initializer result cannot be represented as a compile-time value",
        ),
        Failure::ImportedExecutableTemplateMismatch => (
            false,
            "an imported native operation does not match its compiled definition",
        ),
        Failure::CompilerKnownRepresentationUnavailable(role) => {
            return super::format_internal_compiler_error(format!(
                "the language-defined `{}` type required for the selected target was unavailable",
                format_compiler_known_representation(role),
            ));
        }
        Failure::InvalidExpressionTypeInput { expression } => {
            let _ = expression;

            return super::format_internal_compiler_error(format!(
                "associated the highlighted expression with the wrong source body while determining its type",
            ));
        }
        Failure::IncompatibleInput {
            input,
            expected_kind,
            actual_kind,
            ..
        } => {
            return super::format_internal_compiler_error(format!(
                "associated {} for the highlighted {} with a different {}",
                format_checker_input(input),
                format_bound_unit_kind(actual_kind),
                format_bound_unit_kind(expected_kind),
            ));
        }
        Failure::CheckedConstantTerms { .. } => (
            true,
            "more than one checked value was retained for the same constant expression",
        ),
        Failure::LiteralValue(failure) => {
            return super::format_internal_compiler_error(format_literal_value_failure(failure));
        }
        Failure::PatternInput(failure) => {
            return super::format_internal_compiler_error(format_pattern_input_failure(failure));
        }
        Failure::ConstantInput(failure) => {
            return super::format_internal_compiler_error(format_constant_input_failure(failure));
        }
        Failure::ConstantEvaluation(failure) => {
            return super::format_internal_compiler_error(format_constant_evaluation_failure(
                failure,
            ));
        }
        Failure::ConstantOperation(failure) => {
            return super::format_internal_compiler_error(format_constant_operation_failure(
                failure,
            ));
        }
        Failure::SelectionInputCapacityExceeded { count } => {
            return super::format_internal_compiler_error(format!(
                "could not represent semantic-selection input position {count}",
            ));
        }
        Failure::SelectionInputOrdinalUnrepresentable { ordinal } => {
            return super::format_internal_compiler_error(format!(
                "could not represent semantic-selection callable parameter ordinal {ordinal}",
            ));
        }
        Failure::ConstantArrayLengthCapacityExceeded { length } => {
            return super::format_internal_compiler_error(format!(
                "could not represent constant array length {length}",
            ));
        }
        Failure::InvalidSemanticSelectionInput => (
            true,
            "the operation or call for an expression could not be selected",
        ),
        Failure::SemanticSelection(failure) => {
            return super::format_internal_compiler_error(format_semantic_selection_failure(
                failure,
            ));
        }
        Failure::InvalidConstantEvaluationInput => (
            true,
            "constant evaluation received incompatible source information",
        ),
        Failure::InvalidStoragePlan => (
            true,
            "the local values used by a source body could not be arranged",
        ),
        Failure::StoragePlan(failure) => {
            return super::format_internal_compiler_error(format_storage_plan_failure(failure));
        }
        Failure::InvalidLiveness => (true, "value lifetimes could not be determined"),
        Failure::InvalidAtomicOperationInput { .. }
        | Failure::SelectionDiagnosticCapacityExceeded { .. }
        | Failure::CallbackParameterOrdinalUnrepresentable { .. }
        | Failure::InvalidMemoryOperationInput { .. }
        | Failure::InvalidMemoryGenericArgument { .. }
        | Failure::InvalidCallbackSignatureInput { .. }
        | Failure::MemoryOperations(_)
        | Failure::Liveness(_) => unreachable!("contextual checker failure was handled above"),
        Failure::InvalidRefinementInput => (
            true,
            "condition and pattern implications could not be tracked",
        ),
        Failure::RefinementCapacityUnrepresentable => (
            false,
            "an internal compiler limit prevented Bray from retaining everything a condition or pattern proves about a value",
        ),
        Failure::RefinementStorageUnavailable => (
            false,
            "memory needed to track what a condition or pattern proves about a value was unavailable",
        ),
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
                "classified a local-value access by the highlighted {} as {}",
                format_checker_node_kind(expression.kind()),
                format_storage_operation_status(status),
            ));
        }
        Failure::InvalidBodySemantics => (
            true,
            "found incompatible analysis results for a source body",
        ),
        Failure::SemanticSnapshot(failure) => {
            return super::format_internal_compiler_error(format!(
                "associated {} for a {} with a different {}",
                format_checker_snapshot_input(failure.input()),
                format_bound_unit_kind(failure.expected_kind()),
                format_bound_unit_kind(failure.actual_kind()),
            ));
        }
        Failure::InvalidBoundNode { node } => {
            return super::format_internal_compiler_error(format!(
                "lost the highlighted {} required to compile its source body",
                format_checker_node_kind(node.kind()),
            ));
        }
        Failure::ExpressionTypeCapacityExceeded => (
            false,
            "an internal compiler limit prevented Bray from determining every expression type in a source body",
        ),
        Failure::InvalidUnitView("semantic_context_mismatch") => (
            true,
            "analysis results belonged to the wrong source declaration or body",
        ),
        Failure::InvalidUnitView(reason) => {
            return super::format_internal_compiler_error(format!(
                "could not use a source declaration or body because its `{reason}` consistency check failed"
            ));
        }
    };

    if is_internal {
        super::format_internal_compiler_error(message)
    } else {
        message.to_owned()
    }
}

fn format_contextual_checker_failure(
    failure: bray_diagnostics::DiagnosticCheckerFailure,
) -> Option<String> {
    use bray_diagnostics::DiagnosticCheckerFailure as Failure;

    let detail = match failure {
        Failure::InvalidAtomicOperationInput {
            hook,
            argument_count,
        } => format!(
            "hook '{hook}' with {argument_count} generic arguments is outside the atomic operation catalog",
        ),
        Failure::SelectionDiagnosticCapacityExceeded { kind, count } => {
            format!("could not represent {kind} for {count} selection candidates")
        }
        Failure::CallbackParameterOrdinalUnrepresentable { ordinal } => {
            format!("could not represent callback parameter ordinal {ordinal}")
        }
        Failure::InvalidMemoryOperationInput { hook } => {
            format!("hook '{hook}' is outside the memory operation catalog")
        }
        Failure::InvalidMemoryGenericArgument { ordinal, actual } => {
            format!("memory operation generic argument {ordinal} was {actual} instead of type",)
        }
        Failure::InvalidCallbackSignatureInput { actual } => {
            format!("callback signature had {actual} type syntax instead of a callable")
        }
        Failure::MemoryOperations(failure) => match failure {
            bray_diagnostics::DiagnosticMemoryOperationsFailure::ForeignUnit => {
                "a checked memory operation belongs to another bound unit".to_owned()
            }
            bray_diagnostics::DiagnosticMemoryOperationsFailure::DuplicateExpression => {
                "more than one checked memory operation describes the same expression".to_owned()
            }
        },
        Failure::Liveness(failure) => match failure {
            bray_diagnostics::DiagnosticLivenessFailure::ForeignUnit => {
                "a liveness decision belongs to another bound unit".to_owned()
            }
            bray_diagnostics::DiagnosticLivenessFailure::UnsupportedSubject => {
                "a liveness decision retained an unsupported subject".to_owned()
            }
        },
        _ => return None,
    };

    Some(super::format_internal_compiler_error(detail))
}

pub(super) fn format_generic_substitution_failure(
    failure: bray_diagnostics::DiagnosticGenericSubstitutionFailure,
) -> String {
    use bray_diagnostics::DiagnosticGenericSubstitutionFailure as Failure;

    match failure {
        Failure::ArgumentCountMismatch {
            parameter_count,
            argument_count,
        } => {
            format!("received {argument_count} generic arguments for {parameter_count} parameters",)
        }
        Failure::ArgumentKindMismatch {
            ordinal,
            expected,
            actual,
        } => format!(
            "expected a {expected} generic argument at position {ordinal}, but received {actual}",
        ),
        Failure::OrdinalOverflow => {
            "could not represent every generic argument position".to_owned()
        }
    }
}

fn format_constant_operation_failure(
    failure: bray_diagnostics::DiagnosticCheckerConstantOperationFailure,
) -> String {
    use bray_diagnostics::DiagnosticCheckerConstantOperationFailure as Failure;

    match failure {
        Failure::Invalid => {
            "received incompatible constant values for equality comparison".to_owned()
        }
        Failure::DivisionByZero => {
            "encountered division by zero while comparing constants".to_owned()
        }
        Failure::NotRepresentable => {
            "could not represent the result of a constant comparison".to_owned()
        }
        Failure::ResourceLimitExceeded { actual, maximum } => format!(
            "required {actual} units of constant-evaluation work, exceeding the limit of {maximum}",
        ),
    }
}

fn format_storage_plan_failure(
    failure: bray_diagnostics::DiagnosticStoragePlanFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticStoragePlanFailure as Failure;

    match failure {
        Failure::ForeignUnit => "associated planned storage with a different source body",
        Failure::CapacityExceeded => "could not represent every planned storage record",
        Failure::MissingIdentity => "referenced a planned storage identity that does not exist",
        Failure::MissingAccess => "referenced a planned storage access that does not exist",
        Failure::MissingBorrowCapability => {
            "referenced a planned borrow capability that does not exist"
        }
        Failure::DuplicateBinding => "bound one source value to storage more than once",
        Failure::BindingIdentityMismatch => {
            "bound one source value to an incompatible storage identity"
        }
    }
}

fn format_semantic_selection_failure(
    failure: bray_diagnostics::DiagnosticSemanticSelectionFailure,
) -> String {
    use bray_diagnostics::DiagnosticSemanticSelectionFailure as Failure;

    match failure {
        Failure::ForeignExpressionTypes {
            expected_kind,
            actual_kind,
            ..
        } => format!(
            "associated expression types for a {} with a different {}",
            format_bound_unit_kind(expected_kind),
            format_bound_unit_kind(actual_kind),
        ),
        Failure::InvalidExpression(expression) => format!(
            "selected semantics for a {} outside its source body",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::DuplicateExpression(expression) => format!(
            "selected more than one meaning for the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::SelectionKindMismatch(expression) => format!(
            "selected the wrong operation category for the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::ResultTypeMismatch(expression) => format!(
            "selected a result type that disagrees with the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::OperandTypeMismatch(expression) => format!(
            "selected an operand type that disagrees with the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::SubjectTypeMismatch(expression) => format!(
            "selected an implementation for the wrong subject type at the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
    }
}

fn format_checker_snapshot_input(input: &str) -> &'static str {
    match input {
        "selections" => "selected expression semantics",
        "literals" => "literal values",
        "refinements" => "flow-sensitive refinements",
        "storage_flow" => "storage-flow results",
        "dependencies" => "dependency contracts",
        "asynchronous" => "asynchronous-behavior results",
        "behavior" => "body-behavior results",
        _ => "semantic results",
    }
}

fn format_literal_value_failure(
    failure: bray_diagnostics::DiagnosticLiteralValueFailure,
) -> String {
    use bray_diagnostics::DiagnosticLiteralValueFailure as Failure;

    match failure {
        Failure::ForeignExpressionTypes => {
            "associated literal types with the wrong source body".to_owned()
        }
        Failure::InvalidLiteral(expression) => format!(
            "treated the highlighted {} as a literal",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::MissingExpressionType(expression) => format!(
            "lost the type of the highlighted {} while determining its literal value",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::MissingLiteralValue(expression) => format!(
            "lost the checked value of the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::ValueTypeMismatch(expression) => format!(
            "retained a value with the wrong type for the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::DuplicateExpression(expression) => format!(
            "retained two values for the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
    }
}

fn format_pattern_input_failure(
    failure: bray_diagnostics::DiagnosticPatternInputFailure,
) -> String {
    use bray_diagnostics::DiagnosticPatternInputFailure as Failure;

    match failure {
        Failure::ConflictingDeclaredPattern(pattern) => format!(
            "retained conflicting declared types for the highlighted {}",
            format_checker_node_kind(pattern.kind()),
        ),
        Failure::ConflictingConstantPattern(pattern) => format!(
            "retained conflicting constant values for the highlighted {}",
            format_checker_node_kind(pattern.kind()),
        ),
        Failure::ConflictingGuard(expression) => format!(
            "retained conflicting constant values for the highlighted {} guard",
            format_checker_node_kind(expression.kind()),
        ),
    }
}

fn format_constant_input_failure(
    failure: bray_diagnostics::DiagnosticConstantInputFailure,
) -> String {
    use bray_diagnostics::DiagnosticConstantInputFailure as Failure;

    match failure {
        Failure::ConflictingReference(expression) => format!(
            "retained conflicting resolutions for the highlighted {}",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::ConflictingLocalTerm(_) => {
            "retained conflicting values for a local constant".to_owned()
        }
    }
}

fn format_constant_evaluation_failure(
    failure: bray_diagnostics::DiagnosticConstantEvaluationFailure,
) -> String {
    use bray_diagnostics::DiagnosticConstantEvaluationFailure as Failure;

    match failure {
        Failure::InvalidExpressionRoot(node)
        | Failure::InvalidBlockRoot(node)
        | Failure::MissingExpression(node)
        | Failure::MissingBlock(node)
        | Failure::MissingPattern(node) => format!(
            "lost the highlighted {} while evaluating a constant",
            format_checker_node_kind(node.kind()),
        ),
        Failure::MissingExpressionType(node) | Failure::MissingBlockResultType(node) => format!(
            "lost the type of the highlighted {} while evaluating a constant",
            format_checker_node_kind(node.kind()),
        ),
        Failure::MissingPatternInput(pattern) => format!(
            "did not check the highlighted {} before evaluating it as a constant",
            format_checker_node_kind(pattern.kind()),
        ),
        Failure::MissingPatternBinding(_) => {
            "lost a checked pattern binding while evaluating a constant".to_owned()
        }
        Failure::UnexpectedPropagation { .. } => {
            "left an unresolved value in a completed constant evaluation".to_owned()
        }
    }
}

fn format_checker_input(input: &str) -> &str {
    match input {
        "async_analysis" => "asynchronous analysis",
        "control_flow" => "control-flow analysis",
        "declared_value_types" => "declared value types",
        "expression_semantics" => "expression semantics",
        "expression_types" => "expression types",
        "literal_values" => "literal values",
        "memory_operations" => "memory operations",
        "patterns" => "pattern results",
        "semantic_selections" => "selected calls and operations",
        "storage_plan" => "local-value layout",
        _ => "semantic information",
    }
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
            "associated {} for the highlighted {} with a different {}",
            format_storage_flow_input(input),
            format_bound_unit_kind(actual_kind),
            format_bound_unit_kind(expected_kind),
        ),
        Failure::FlowConstruction(reason) => format!(
            "could not retain the ownership result for this source body because {}",
            format_storage_flow_construction(reason),
        ),
        Failure::UnresolvedDependencyWitness { expression } => format!(
            "could not determine which values the highlighted {} retains through its trait implementation",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::ForeignDependencyContract => {
            "selected a call or iteration dependency belonging to another source body".to_owned()
        }
        Failure::DependencyContractsConstruction(reason) => format!(
            "could not retain this source body's value dependencies because {}",
            format_dependency_contract_construction(reason),
        ),
        Failure::AsyncConstruction(reason) => format!(
            "could not retain this source body's asynchronous behavior because {}",
            format_async_construction(reason),
        ),
        Failure::MissingAwaitDependencyContract { expression } => format!(
            "did not determine which values the highlighted {} requires",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::MissingDependencyContract { expression, .. } => format!(
            "associated the highlighted {} with a value dependency that does not exist in its source body",
            format_checker_node_kind(expression.kind()),
        ),
        Failure::CallableParameterCountMismatch {
            callable,
            signature_parameters,
            type_parameters,
        } => format!(
            "retained {signature_parameters} declared parameters but {type_parameters} parameter modes for the highlighted {} declaration",
            format_checker_symbol_kind(callable.kind()),
        ),
        Failure::CallableTypeNotCallable { callable } => format!(
            "retained a non-callable type for the highlighted {} declaration",
            format_checker_symbol_kind(callable.kind()),
        ),
        Failure::MissingBorrowCapability { unit, borrow } => {
            let _ = (unit, borrow);

            "lost a borrow operation while checking whether a value escapes the highlighted source body".to_owned()
        }
        Failure::MissingExitOrigin { exit } => format!(
            "lost the source location for the highlighted {}",
            format_checker_node_kind(exit.kind()),
        ),
        Failure::MissingBlock { block } => format!(
            "lost the highlighted {} targeted by a control-flow transfer",
            format_checker_node_kind(block.kind()),
        ),
        Failure::MissingStorageAccess { unit, access } => {
            let _ = (unit, access);

            "lost a local-value access required by the highlighted source body".to_owned()
        }
        Failure::MissingStorageIdentity { unit, identity } => {
            let _ = (unit, identity);

            "lost a local value required by the highlighted source body".to_owned()
        }
        Failure::MissingStorageSymbolName { symbol } => format!(
            "lost the declared name of the highlighted {} declaration while describing a local-value access",
            format_checker_symbol_kind(symbol.kind()),
        ),
        Failure::UnbalancedScopes {
            open_scope: Some(scope),
        } => format!(
            "left the highlighted {} open while examining its source body's lexical scopes",
            format_checker_node_kind(scope.kind()),
        ),
        Failure::UnbalancedScopes { open_scope: None } => {
            "encountered inconsistent block boundaries while examining this source body".to_owned()
        }
        Failure::MissingPattern { pattern } => format!(
            "lost the highlighted {} while examining its source body's lexical scopes",
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
