use super::context::semantic_value_failure_context;
use super::failure::{
    DiagnosticEmissionFieldJson, count_field, count_u64_field, text_field,
};

pub(in crate::output::diagnostic::json) fn checker_failure_context(
    failure: bray_diagnostics::DiagnosticCheckerFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticCheckerFailure as Failure;

    let mut fields = vec![text_field("cause", failure.as_str())];

    match failure {
        Failure::MissingSource { source_id } => {
            fields.push(count_field("source", source_id.raw()));
        }
        Failure::SourceVersionMismatch {
            source_id,
            expected,
            actual,
        } => fields.extend([
            count_field("source", source_id.raw()),
            count_u64_field("expected_source_version", expected.raw()),
            count_u64_field("actual_source_version", actual.raw()),
        ]),
        Failure::InvalidSourceRange { span } => push_span(&mut fields, span),
        Failure::SemanticQueryUnavailable { symbol, query } => {
            push_symbol(&mut fields, symbol);
            fields.push(text_field("query_kind", query));
        }
        Failure::SemanticValue(failure) => return semantic_value_failure_context(failure),
        Failure::GenericSubstitution(failure) => {
            push_generic_substitution_failure(&mut fields, failure);
        }
        Failure::CompilerKnownRepresentationUnavailable(role) => {
            fields.push(text_field("representation_role", role));
        }
        Failure::InvalidExpressionTypeInput { expression }
        | Failure::InvalidBoundNode { node: expression } => {
            push_node(&mut fields, "node", expression);
        }
        Failure::IncompatibleInput {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => push_input_mismatch(
            &mut fields,
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        ),
        Failure::CheckedConstantTerms { owner, source } => {
            push_symbol(&mut fields, owner);
            push_span(&mut fields, source);
        }
        Failure::LiteralValue(failure) => push_literal_failure(&mut fields, failure),
        Failure::PatternInput(failure) => push_pattern_failure(&mut fields, failure),
        Failure::ConstantInput(failure) => push_constant_input_failure(&mut fields, failure),
        Failure::ConstantEvaluation(failure) => {
            push_constant_evaluation_failure(&mut fields, failure);
        }
        Failure::ConstantOperation(failure) => {
            push_constant_operation_failure(&mut fields, failure);
        }
        Failure::SelectionInputCapacityExceeded { count } => {
            fields.push(text_field("selection_input_count", count.to_string()));
        }
        Failure::SelectionInputOrdinalUnrepresentable { ordinal } => {
            fields.push(text_field("selection_input_ordinal", ordinal.to_string()));
        }
        Failure::ConstantArrayLengthCapacityExceeded { length } => {
            fields.push(text_field("array_length", length.to_string()));
        }
        Failure::SemanticSelection(failure) => {
            push_semantic_selection_failure(&mut fields, failure);
        }
        Failure::StorageFlow(failure) => push_storage_flow_failure(&mut fields, failure),
        Failure::StoragePlan(failure) => {
            fields.push(text_field("storage_plan_problem", storage_plan_failure(failure)));
        }
        Failure::InvalidStorageOperation {
            expression,
            access,
            status,
        } => {
            push_node(&mut fields, "expression", expression);
            fields.push(count_field("storage_access", access));
            fields.push(text_field("storage_status", status));
        }
        Failure::SemanticSnapshot(failure) => push_input_mismatch(
            &mut fields,
            failure.input(),
            failure.expected_unit(),
            failure.expected_kind(),
            failure.actual_unit(),
            failure.actual_kind(),
        ),
        Failure::InvalidUnitView(problem) => fields.push(text_field("unit_view_problem", problem)),
        Failure::SemanticValueUnavailable
        | Failure::AtomicRepresentationTypeUnavailable
        | Failure::AtomicRepresentationArgumentsUnavailable
        | Failure::AtomicInitializerArgumentUnavailable
        | Failure::AtomicInitializerResultUnavailable
        | Failure::UninitInitializerResultUnavailable
        | Failure::ImportedExecutableTemplateMismatch
        | Failure::InvalidSemanticSelectionInput
        | Failure::InvalidConstantEvaluationInput
        | Failure::InvalidStoragePlan
        | Failure::InvalidLiveness
        | Failure::InvalidRefinementInput
        | Failure::RefinementCapacityUnrepresentable
        | Failure::RefinementStorageUnavailable
        | Failure::InvalidBodySemantics
        | Failure::ExpressionTypeCapacityExceeded => {}
    }

    fields
}

pub(super) fn push_generic_substitution_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticGenericSubstitutionFailure,
) {
    use bray_diagnostics::DiagnosticGenericSubstitutionFailure as Failure;

    match failure {
        Failure::ArgumentCountMismatch {
            parameter_count,
            argument_count,
        } => fields.extend([
            text_field("substitution_problem", "argument_count_mismatch"),
            text_field("parameter_count", parameter_count.to_string()),
            text_field("argument_count", argument_count.to_string()),
        ]),
        Failure::ArgumentKindMismatch {
            ordinal,
            expected,
            actual,
        } => fields.extend([
            text_field("substitution_problem", "argument_kind_mismatch"),
            count_field("argument_ordinal", ordinal),
            text_field("expected_argument_kind", expected),
            text_field("actual_argument_kind", actual),
        ]),
        Failure::OrdinalOverflow => fields.push(text_field(
            "substitution_problem",
            "argument_ordinal_overflow",
        )),
    }
}

fn push_constant_operation_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticCheckerConstantOperationFailure,
) {
    use bray_diagnostics::DiagnosticCheckerConstantOperationFailure as Failure;

    let problem = match failure {
        Failure::Invalid => "invalid",
        Failure::DivisionByZero => "division_by_zero",
        Failure::NotRepresentable => "not_representable",
        Failure::ResourceLimitExceeded { actual, maximum } => {
            fields.extend([
                count_u64_field("actual_resource_demand", actual),
                count_u64_field("maximum_resource_demand", maximum),
            ]);

            "resource_limit_exceeded"
        }
    };

    fields.push(text_field("constant_operation_problem", problem));
}

const fn storage_plan_failure(
    failure: bray_diagnostics::DiagnosticStoragePlanFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticStoragePlanFailure as Failure;

    match failure {
        Failure::ForeignUnit => "foreign_unit",
        Failure::CapacityExceeded => "capacity_exceeded",
        Failure::MissingIdentity => "missing_identity",
        Failure::MissingAccess => "missing_access",
        Failure::MissingBorrowCapability => "missing_borrow_capability",
        Failure::DuplicateBinding => "duplicate_binding",
        Failure::BindingIdentityMismatch => "binding_identity_mismatch",
    }
}

fn push_literal_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticLiteralValueFailure,
) {
    use bray_diagnostics::DiagnosticLiteralValueFailure as Failure;

    let (kind, node) = match failure {
        Failure::ForeignExpressionTypes => ("foreign_expression_types", None),
        Failure::InvalidLiteral(node) => ("invalid_literal", Some(node)),
        Failure::MissingExpressionType(node) => ("missing_expression_type", Some(node)),
        Failure::MissingLiteralValue(node) => ("missing_literal_value", Some(node)),
        Failure::ValueTypeMismatch(node) => ("value_type_mismatch", Some(node)),
        Failure::DuplicateExpression(node) => ("duplicate_expression", Some(node)),
    };

    fields.push(text_field("literal_failure", kind));

    if let Some(node) = node {
        push_node(fields, "expression", node);
    }
}

fn push_pattern_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticPatternInputFailure,
) {
    use bray_diagnostics::DiagnosticPatternInputFailure as Failure;

    let (kind, name, node) = match failure {
        Failure::ConflictingDeclaredPattern(node) => {
            ("conflicting_declared_pattern", "pattern", node)
        }
        Failure::ConflictingConstantPattern(node) => {
            ("conflicting_constant_pattern", "pattern", node)
        }
        Failure::ConflictingGuard(node) => ("conflicting_guard", "expression", node),
    };

    fields.push(text_field("pattern_input_failure", kind));
    push_node(fields, name, node);
}

fn push_constant_input_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticConstantInputFailure,
) {
    match failure {
        bray_diagnostics::DiagnosticConstantInputFailure::ConflictingReference(node) => {
            fields.push(text_field(
                "constant_input_failure",
                "conflicting_reference",
            ));

            push_node(fields, "expression", node);
        }
        bray_diagnostics::DiagnosticConstantInputFailure::ConflictingLocalTerm(local) => {
            fields.push(text_field(
                "constant_input_failure",
                "conflicting_local_term",
            ));

            push_local(fields, local);
        }
    }
}

fn push_constant_evaluation_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticConstantEvaluationFailure,
) {
    use bray_diagnostics::DiagnosticConstantEvaluationFailure as Failure;

    let (kind, name, node) = match failure {
        Failure::InvalidExpressionRoot(node) => ("invalid_expression_root", "expression", node),
        Failure::InvalidBlockRoot(node) => ("invalid_block_root", "block", node),
        Failure::MissingExpressionType(node) => ("missing_expression_type", "expression", node),
        Failure::MissingBlockResultType(node) => ("missing_block_result_type", "block", node),
        Failure::MissingExpression(node) => ("missing_expression", "expression", node),
        Failure::MissingBlock(node) => ("missing_block", "block", node),
        Failure::MissingPatternInput(node) => ("missing_pattern_input", "pattern", node),
        Failure::MissingPattern(node) => ("missing_pattern", "pattern", node),
        Failure::MissingPatternBinding(local) => {
            fields.push(text_field(
                "constant_evaluation_failure",
                "missing_pattern_binding",
            ));

            push_local(fields, local);
            return;
        }
        Failure::UnexpectedPropagation { store, slot } => {
            fields.push(text_field(
                "constant_evaluation_failure",
                "unexpected_propagation",
            ));

            fields.push(count_u64_field("semantic_store", store));
            fields.push(count_field("constant_term", slot));
            return;
        }
    };

    fields.push(text_field("constant_evaluation_failure", kind));
    push_node(fields, name, node);
}

fn push_semantic_selection_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticSemanticSelectionFailure,
) {
    use bray_diagnostics::DiagnosticSemanticSelectionFailure as Failure;

    let (kind, node) = match failure {
        Failure::ForeignExpressionTypes {
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => {
            fields.push(text_field(
                "semantic_selection_failure",
                "foreign_expression_types",
            ));

            push_input_mismatch(
                fields,
                "expression_types",
                expected_unit,
                expected_kind,
                actual_unit,
                actual_kind,
            );

            return;
        }
        Failure::InvalidExpression(node) => ("invalid_expression", node),
        Failure::DuplicateExpression(node) => ("duplicate_expression", node),
        Failure::SelectionKindMismatch(node) => ("selection_kind_mismatch", node),
        Failure::ResultTypeMismatch(node) => ("result_type_mismatch", node),
        Failure::OperandTypeMismatch(node) => ("operand_type_mismatch", node),
        Failure::SubjectTypeMismatch(node) => ("subject_type_mismatch", node),
    };

    fields.push(text_field("semantic_selection_failure", kind));
    push_node(fields, "expression", node);
}

fn push_storage_flow_failure(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    failure: bray_diagnostics::DiagnosticStorageFlowFailure,
) {
    use bray_diagnostics::DiagnosticStorageFlowFailure as Failure;

    match failure {
        Failure::IncompatibleInput {
            input,
            expected_unit,
            expected_kind,
            actual_unit,
            actual_kind,
        } => {
            fields.push(text_field("storage_flow_failure", "incompatible_input"));

            push_input_mismatch(
                fields,
                input,
                expected_unit,
                expected_kind,
                actual_unit,
                actual_kind,
            );
        }
        Failure::FlowConstruction(problem) => {
            push_storage_problem(fields, "flow_construction", problem);
        }
        Failure::ForeignDependencyContract => {
            fields.push(text_field(
                "storage_flow_failure",
                "foreign_dependency_contract",
            ));
        }
        Failure::DependencyContractsConstruction(problem) => {
            push_storage_problem(fields, "dependency_contracts_construction", problem);
        }
        Failure::AsyncConstruction(problem) => {
            push_storage_problem(fields, "async_construction", problem);
        }
        Failure::MissingAwaitDependencyContract { expression } => {
            fields.push(text_field(
                "storage_flow_failure",
                "missing_await_dependency_contract",
            ));

            push_node(fields, "expression", expression);
        }
        Failure::MissingDependencyContract {
            expression,
            contract_unit,
            contract,
        } => {
            fields.push(text_field(
                "storage_flow_failure",
                "missing_dependency_contract",
            ));

            push_node(fields, "expression", expression);
            fields.push(count_field("contract_unit", contract_unit));
            fields.push(count_field("dependency_contract", contract));
        }
        Failure::CallableParameterCountMismatch {
            callable,
            signature_parameters,
            type_parameters,
        } => {
            fields.push(text_field(
                "storage_flow_failure",
                "callable_parameter_count_mismatch",
            ));

            push_symbol(fields, callable);

            fields.push(count_u64_field(
                "signature_parameters",
                count_from_usize(signature_parameters),
            ));

            fields.push(count_u64_field(
                "type_parameters",
                count_from_usize(type_parameters),
            ));
        }
        Failure::CallableTypeNotCallable { callable } => {
            fields.push(text_field(
                "storage_flow_failure",
                "callable_type_not_callable",
            ));

            push_symbol(fields, callable);
        }
        Failure::MissingBorrowCapability { unit, borrow } => {
            push_storage_identity(fields, "missing_borrow_capability", unit, "borrow", borrow);
        }
        Failure::MissingExitOrigin { exit } => {
            fields.push(text_field("storage_flow_failure", "missing_exit_origin"));
            push_node(fields, "exit", exit);
        }
        Failure::MissingBlock { block } => {
            fields.push(text_field("storage_flow_failure", "missing_block"));
            push_node(fields, "block", block);
        }
        Failure::MissingStorageAccess { unit, access } => {
            push_storage_identity(fields, "missing_storage_access", unit, "storage_access", access);
        }
        Failure::MissingStorageIdentity { unit, identity } => push_storage_identity(
            fields,
            "missing_storage_identity",
            unit,
            "storage_identity",
            identity,
        ),
        Failure::MissingStorageSymbolName { symbol } => {
            fields.push(text_field(
                "storage_flow_failure",
                "missing_storage_symbol_name",
            ));

            push_symbol(fields, symbol);
        }
        Failure::UnbalancedScopes { open_scope } => {
            fields.push(text_field("storage_flow_failure", "unbalanced_scopes"));

            if let Some(open_scope) = open_scope {
                push_node(fields, "open_scope", open_scope);
            }
        }
        Failure::MissingPattern { pattern } => {
            fields.push(text_field("storage_flow_failure", "missing_pattern"));
            push_node(fields, "pattern", pattern);
        }
    }
}

fn push_storage_problem(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    kind: &'static str,
    problem: &'static str,
) {
    fields.push(text_field("storage_flow_failure", kind));
    fields.push(text_field("storage_flow_problem", problem));
}

fn push_storage_identity(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    kind: &'static str,
    unit: u32,
    name: &'static str,
    ordinal: u32,
) {
    fields.push(text_field("storage_flow_failure", kind));
    fields.push(count_field("bound_unit", unit));
    fields.push(count_field(name, ordinal));
}

fn push_input_mismatch(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    input: &'static str,
    expected_unit: u32,
    expected_kind: &'static str,
    actual_unit: u32,
    actual_kind: &'static str,
) {
    fields.extend([
        text_field("input", input),
        count_field("expected_unit", expected_unit),
        text_field("expected_unit_kind", expected_kind),
        count_field("actual_unit", actual_unit),
        text_field("actual_unit_kind", actual_kind),
    ]);
}

fn push_span(fields: &mut Vec<DiagnosticEmissionFieldJson>, span: bray_source::SourceSpan) {
    fields.extend([
        count_field("source", span.source_id().raw()),
        count_field("source_start", span.start().bytes()),
        count_field("source_end", span.end().bytes()),
    ]);
}

fn push_symbol(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    symbol: bray_diagnostics::DiagnosticCheckerSymbol,
) {
    fields.push(text_field("symbol_kind", symbol.kind()));
    fields.push(count_field("symbol", symbol.ordinal()));
}

fn push_node(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    name: &'static str,
    node: bray_diagnostics::DiagnosticCheckerNode,
) {
    fields.push(text_field("node_kind", node.kind()));
    fields.push(count_field("bound_unit", node.unit()));
    fields.push(count_field(name, node.ordinal()));
}

fn push_local(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    local: bray_diagnostics::DiagnosticCheckerLocal,
) {
    fields.push(text_field("local_kind", local.kind()));
    fields.push(count_field("local_region", local.region()));
    fields.push(count_field("local", local.ordinal()));
}

fn count_from_usize(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::checker_failure_context;

    #[test]
    fn checker_failures_serialize_typed_payloads() {
        let fields = checker_failure_context(
            bray_diagnostics::DiagnosticCheckerFailure::SourceVersionMismatch {
                source_id: bray_source::SourceId::new(3),
                expected: bray_source::SourceVersion::new(5),
                actual: bray_source::SourceVersion::new(8),
            },
        );

        let json = serde_json::to_value(fields)
            .unwrap_or_else(|error| panic!("checker context should serialize: {error:?}"));

        assert_eq!(json[1]["name"], "source");
        assert_eq!(json[1]["value"]["value"], 3);
        assert_eq!(json[2]["name"], "expected_source_version");
        assert_eq!(json[2]["value"]["value"], 5);
        assert_eq!(json[3]["name"], "actual_source_version");
        assert_eq!(json[3]["value"]["value"], 8);
    }

    #[test]
    fn nested_checker_failures_retain_leaf_category_and_identity() {
        let fields = checker_failure_context(
            bray_diagnostics::DiagnosticCheckerFailure::PatternInput(
                bray_diagnostics::DiagnosticPatternInputFailure::ConflictingGuard(
                    bray_diagnostics::DiagnosticCheckerNode::new("expression", 13, 21),
                ),
            ),
        );

        let json = serde_json::to_value(fields)
            .unwrap_or_else(|error| panic!("checker context should serialize: {error:?}"));

        assert_eq!(json[1]["value"]["value"], "conflicting_guard");
        assert_eq!(json[3]["value"]["value"], 13);
        assert_eq!(json[4]["value"]["value"], 21);
    }
}
