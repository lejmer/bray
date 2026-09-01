use bray_diagnostics::DiagnosticType;

use super::super::{DiagnosticProblemJson, DiagnosticTypeJson};
use super::identity::{
    DiagnosticArrayLengthJson, DiagnosticInterfaceSymbolIdentityJson, DiagnosticProblemFieldJson,
    DiagnosticProblemFieldValueJson,
};

pub(in crate::output::diagnostic::json) fn problem(
    reason: &'static str,
    context: impl IntoIterator<Item = DiagnosticProblemFieldJson>,
) -> DiagnosticProblemJson {
    DiagnosticProblemJson {
        reason,
        context: context.into_iter().collect(),
    }
}

fn problem_count(name: &'static str, value: u32) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Count(u64::from(value)),
    }
}

pub(in crate::output::diagnostic::json) fn problem_count_u64(
    name: &'static str,
    value: u64,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Count(value),
    }
}

pub(in crate::output::diagnostic::json) fn problem_text(
    name: &'static str,
    value: impl Into<String>,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Text(value.into()),
    }
}

pub(in crate::output::diagnostic::json) fn problem_type(
    name: &'static str,
    value: &DiagnosticType,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Type(DiagnosticTypeJson::from_type(value)),
    }
}

pub(in crate::output::diagnostic::json) fn problem_types(
    name: &'static str,
    values: &[DiagnosticType],
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Types(
            values.iter().map(DiagnosticTypeJson::from_type).collect(),
        ),
    }
}

pub(in crate::output::diagnostic::json) fn problem_array_length(
    name: &'static str,
    value: bray_diagnostics::DiagnosticArrayLength,
) -> DiagnosticProblemFieldJson {
    let value = match value {
        bray_diagnostics::DiagnosticArrayLength::Exact(value) => {
            DiagnosticArrayLengthJson::Exact(value)
        }
        bray_diagnostics::DiagnosticArrayLength::Symbolic => DiagnosticArrayLengthJson::Symbolic,
    };

    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::ArrayLength(value),
    }
}

fn problem_symbol_identity(
    name: &'static str,
    value: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::SymbolIdentity(
            DiagnosticInterfaceSymbolIdentityJson::from_identity(value),
        ),
    }
}

pub(in crate::output::diagnostic::json) fn interface_symbol_graph_problem_json(
    value: &bray_diagnostics::DiagnosticInterfaceSymbolGraphProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceSymbolGraphProblem as Problem;

    match value {
        Problem::DuplicateInterface(interface) => problem(
            "duplicate_interface",
            [problem_count("interface", *interface)],
        ),
        Problem::DuplicatePackage(package) => {
            problem("duplicate_package", [problem_text("package", package)])
        }
        Problem::SymbolCapacityExceeded { actual, maximum } => problem(
            "symbol_capacity_exceeded",
            [
                DiagnosticProblemFieldJson {
                    name: "actual",
                    value: DiagnosticProblemFieldValueJson::Count(*actual),
                },
                DiagnosticProblemFieldJson {
                    name: "maximum",
                    value: DiagnosticProblemFieldValueJson::Count(*maximum),
                },
            ],
        ),
        Problem::DuplicateExternalIdentity(identity) => problem(
            "duplicate_external_identity",
            [problem_symbol_identity("identity", identity)],
        ),
        Problem::RelationshipSymbolOutOfBounds { interface, symbol } => problem(
            "relationship_symbol_out_of_bounds",
            [
                problem_count("interface", *interface),
                problem_count("symbol", *symbol),
            ],
        ),
        Problem::InvalidRelationshipKinds {
            relationship,
            owner,
            member,
        } => problem(
            "invalid_relationship_kinds",
            [
                problem_text("relationship", relationship.as_str()),
                problem_text("owner_kind", owner.as_str()),
                problem_text("member_kind", member.as_str()),
            ],
        ),
        Problem::RelationshipContainmentMismatch { owner, member } => problem(
            "relationship_containment_mismatch",
            [
                problem_count("owner", *owner),
                problem_count("member", *member),
            ],
        ),
        Problem::NonCanonicalRelationshipOrdinal {
            relationship,
            owner,
            expected,
            actual,
        } => problem(
            "non_canonical_relationship_ordinal",
            [
                problem_text("relationship", relationship.as_str()),
                problem_count("owner", *owner),
                DiagnosticProblemFieldJson {
                    name: "expected",
                    value: DiagnosticProblemFieldValueJson::Count(*expected),
                },
                problem_count("actual", *actual),
            ],
        ),
        Problem::MissingContainment { symbol, kind } => problem(
            "missing_containment",
            [
                problem_count("symbol", *symbol),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::DuplicateContainment { symbol, kind } => problem(
            "duplicate_containment",
            [
                problem_count("symbol", *symbol),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::LookupOwnerOutOfBounds { interface, owner } => problem(
            "lookup_owner_out_of_bounds",
            [
                problem_count("interface", *interface),
                problem_count("owner", *owner),
            ],
        ),
        Problem::InvalidLookupOwner { owner, kind } => problem(
            "invalid_lookup_owner",
            [
                problem_count("owner", *owner),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::MissingLookupTarget(identity) => problem(
            "missing_lookup_target",
            [problem_symbol_identity("identity", identity)],
        ),
        Problem::DuplicateLookupName { owner, name } => problem(
            "duplicate_lookup_name",
            [problem_count("owner", *owner), problem_text("name", name)],
        ),
        Problem::UnsupportedSymbolKind(kind) => problem(
            "unsupported_symbol_kind",
            [problem_text("symbol_kind", kind.as_str())],
        ),
        Problem::InvalidRecordRelationships { symbol, kind } => problem(
            "invalid_record_relationships",
            [
                problem_count("symbol", *symbol),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
    }
}

pub(in crate::output::diagnostic::json) fn interface_semantic_problem_json(
    value: &bray_diagnostics::DiagnosticInterfaceSemanticProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceSemanticProblem as Problem;

    match value {
        Problem::UnresolvedSymbol(reference) => {
            interface_symbol_reference_problem("unresolved_symbol", reference)
        }
        Problem::InvalidSymbolKind(reference) => {
            interface_symbol_reference_problem("invalid_symbol_kind", reference)
        }
        Problem::UnresolvedValueGraph => problem("unresolved_value_graph", []),
        Problem::SemanticContent(value) => semantic_content_problem_json(value),
        Problem::InvalidTemplate(value) => checked_template_problem_json(value),
        Problem::InvalidSupportEntity(entity) => problem(
            "invalid_support_entity",
            [problem_count("support_entity", *entity)],
        ),
    }
}

fn interface_symbol_reference_problem(
    reason: &'static str,
    reference: &bray_diagnostics::DiagnosticInterfaceSymbolReference,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceSymbolReference as Reference;

    let context = match reference {
        Reference::Local(symbol) => vec![
            problem_text("reference_kind", "local"),
            problem_count("symbol", *symbol),
        ],
        Reference::Dependency {
            dependency,
            identity,
        } => vec![
            problem_text("reference_kind", "dependency"),
            problem_count("dependency", *dependency),
            problem_symbol_identity("identity", identity),
        ],
        Reference::CompilerKnown(identity) => vec![
            problem_text("reference_kind", "compiler_known"),
            problem_symbol_identity("identity", identity),
        ],
    };

    problem(reason, context)
}

fn semantic_content_problem_json(
    value: &bray_diagnostics::DiagnosticSemanticContentProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticSemanticContentProblem as Problem;

    match value {
        Problem::ForeignId { expected, actual } => problem(
            "semantic_content_foreign_id",
            [
                DiagnosticProblemFieldJson {
                    name: "expected_content_set",
                    value: DiagnosticProblemFieldValueJson::Count(*expected),
                },
                DiagnosticProblemFieldJson {
                    name: "actual_content_set",
                    value: DiagnosticProblemFieldValueJson::Count(*actual),
                },
            ],
        ),
        Problem::UnknownId { value_kind } => problem(
            "semantic_content_unknown_id",
            [problem_text("value_kind", value_kind.as_str())],
        ),
        Problem::CapacityExhausted { value_kind } => problem(
            "semantic_content_capacity_exhausted",
            [problem_text("value_kind", value_kind.as_str())],
        ),
        Problem::GenericOwnerMismatch {
            expected_kind,
            expected,
            actual_kind,
            actual,
        } => problem(
            "semantic_content_generic_owner_mismatch",
            [
                problem_text("expected_owner_kind", expected_kind.as_str()),
                problem_count("expected_owner", *expected),
                problem_text("actual_owner_kind", actual_kind.as_str()),
                problem_count("actual_owner", *actual),
            ],
        ),
        Problem::OpenSubstitution => problem("semantic_content_open_substitution", []),
    }
}

fn checked_template_problem_json(
    value: &bray_diagnostics::DiagnosticCheckedTemplateProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticCheckedTemplateProblem as Problem;

    match value {
        Problem::CapacityExceeded => problem("template_capacity_exceeded", []),
        Problem::RecoveredTemplate => problem("template_recovered_content", []),
        Problem::MissingInput(input) => {
            problem("template_missing_input", [problem_count("input", *input)])
        }
        Problem::DuplicateInput { first, duplicate } => problem(
            "template_duplicate_input",
            [
                problem_count("first", *first),
                problem_count("duplicate", *duplicate),
            ],
        ),
        Problem::InputTypeMismatch {
            node,
            input,
            expected_type,
            actual_type,
        } => problem(
            "template_input_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("input", *input),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
        Problem::MissingNode(node) => {
            problem("template_missing_node", [problem_count("node", *node)])
        }
        Problem::ForwardNodeReference { node, referenced } => problem(
            "template_forward_node_reference",
            [
                problem_count("node", *node),
                problem_count("referenced", *referenced),
            ],
        ),
        Problem::MissingTemporary(temporary) => problem(
            "template_missing_temporary",
            [problem_count("temporary", *temporary)],
        ),
        Problem::UninitializedTemporary { node, temporary } => problem(
            "template_uninitialized_temporary",
            [
                problem_count("node", *node),
                problem_count("temporary", *temporary),
            ],
        ),
        Problem::TemporaryInitializerTypeMismatch {
            initializer,
            expected_type,
            actual_type,
        } => problem(
            "template_temporary_initializer_type_mismatch",
            [
                problem_count("initializer", *initializer),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
        Problem::TemporaryTypeMismatch {
            node,
            temporary,
            expected_type,
            actual_type,
        } => problem(
            "template_temporary_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("temporary", *temporary),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
        Problem::ConversionTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => type_mismatch_problem(
            "template_conversion_type_mismatch",
            "node",
            *node,
            *expected_type,
            *actual_type,
        ),
        Problem::ConditionalBranchTypeMismatch {
            node,
            when_true_type,
            when_false_type,
        } => problem(
            "template_conditional_branch_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("when_true_type", *when_true_type),
                problem_count("when_false_type", *when_false_type),
            ],
        ),
        Problem::ConditionalResultTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => type_mismatch_problem(
            "template_conditional_result_type_mismatch",
            "node",
            *node,
            *expected_type,
            *actual_type,
        ),
        Problem::ShortCircuitOperandTypeMismatch {
            node,
            left_type,
            right_type,
        } => problem(
            "template_short_circuit_operand_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("left_type", *left_type),
                problem_count("right_type", *right_type),
            ],
        ),
        Problem::ShortCircuitResultTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => type_mismatch_problem(
            "template_short_circuit_result_type_mismatch",
            "node",
            *node,
            *expected_type,
            *actual_type,
        ),
        Problem::ArrayElementTypeMismatch {
            node,
            element,
            expected_type,
            actual_type,
        } => problem(
            "template_array_element_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("element", *element),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
    }
}

fn type_mismatch_problem(
    reason: &'static str,
    subject_name: &'static str,
    subject: u32,
    expected: u32,
    actual: u32,
) -> DiagnosticProblemJson {
    problem(
        reason,
        [
            problem_count(subject_name, subject),
            problem_count("expected_type", expected),
            problem_count("actual_type", actual),
        ],
    )
}
