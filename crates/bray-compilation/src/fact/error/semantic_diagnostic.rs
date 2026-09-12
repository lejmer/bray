use bray_diagnostics::{DiagnosticFailureField, DiagnosticSemanticQueryFailure};

use super::diagnostic_context::{
    constant_value_kind, count_field, identity_field, natural_field, push_source_span, push_symbol,
    text_field,
};
use super::semantic_context::semantic_query_context;
use crate::compilation::{
    SemanticDataKind, SemanticQueryError, SemanticQueryFailure, SemanticQueryViolation,
    SemanticSymbolCategory,
};

pub(crate) fn diagnostic_semantic_query_failure(
    error: &SemanticQueryError,
) -> DiagnosticSemanticQueryFailure {
    let (category, reason, mut context) = match error.cause() {
        SemanticQueryFailure::Contract(failure) => {
            let mut context = semantic_query_context(failure.context());
            let reason = push_contract_violation(&mut context, failure.violation());

            ("semantic_query_contract_violation", reason, context)
        }
        SemanticQueryFailure::CallableSignature { callable, cause } => {
            let mut context = Vec::new();
            push_optional_symbol(&mut context, "callable_kind", "callable", *callable);

            if let bray_symbols::CallableSignatureTemplateError::SemanticValue(cause) = cause {
                push_semantic_value_failure(&mut context, *cause);
            }

            (
                "semantic_query_callable_signature",
                callable_signature_reason(cause),
                context,
            )
        }
        SemanticQueryFailure::GenericSubstitution { owner, cause } => {
            let mut context = Vec::new();

            if let Some(owner) = owner {
                push_symbol(&mut context, "owner_kind", "owner", owner.symbol());
            }

            push_generic_substitution_failure(&mut context, cause);

            (
                "semantic_query_generic_substitution",
                generic_substitution_reason(cause),
                context,
            )
        }
        SemanticQueryFailure::BoundUnit { unit, cause } => {
            let mut context = vec![identity_field("unit", unit)];

            push_bound_unit_failure(&mut context, cause);

            (
                "semantic_query_bound_unit",
                bound_unit_reason(cause),
                context,
            )
        }
        SemanticQueryFailure::ImplementationMatch {
            implementation,
            cause,
        } => {
            let mut context = Vec::new();

            push_optional_symbol(
                &mut context,
                "implementation_kind",
                "implementation",
                implementation.map(bray_symbols::ImplementationSymbolId::into_any),
            );

            match cause {
                crate::compilation::ImplementationMatchError::InvalidSubstitution(cause) => {
                    push_generic_substitution_failure(&mut context, cause);
                }
                crate::compilation::ImplementationMatchError::SemanticValue(cause) => {
                    push_semantic_value_failure(&mut context, *cause);
                }
            }

            (
                "semantic_query_implementation",
                implementation_match_reason(cause),
                context,
            )
        }
        SemanticQueryFailure::ImplementationAmbiguity { requirement, cause } => (
            "semantic_query_implementation",
            implementation_ambiguity_reason(cause),
            vec![identity_field("requirement", requirement)],
        ),
        SemanticQueryFailure::ImplementationCandidateSet { requirement, cause } => (
            "semantic_query_implementation",
            implementation_candidate_set_reason(cause),
            vec![identity_field("requirement", requirement)],
        ),
        SemanticQueryFailure::ImplementationCoherenceEvidence { requirement, cause } => (
            "semantic_query_implementation",
            implementation_coherence_reason(cause),
            vec![identity_field("requirement", requirement)],
        ),
        SemanticQueryFailure::ImplementationCandidate {
            requirement,
            implementation,
            cause,
        } => {
            let mut context = vec![identity_field("requirement", requirement)];

            push_symbol(
                &mut context,
                "implementation_kind",
                "implementation",
                implementation.into_any(),
            );

            (
                "semantic_query_implementation",
                implementation_candidate_reason(cause),
                context,
            )
        }
        SemanticQueryFailure::ImplementationParticipation { domain, cause } => (
            "semantic_query_implementation",
            implementation_participation_reason(cause),
            vec![identity_field("domain", domain)],
        ),
        SemanticQueryFailure::CheckedConstantTerms { unit, cause } => {
            let bray_checker::CheckedConstantTermsBuildError::DuplicateOccurrence(occurrence) =
                cause;

            let mut context = vec![identity_field("occurrence", occurrence)];

            if let Some(unit) = unit {
                context.push(identity_field("unit", unit));
            }

            (
                "semantic_query_checked_constant_terms",
                "checked_constant_terms_duplicate_occurrence",
                context,
            )
        }
        SemanticQueryFailure::TypeAssociatedSurface { subject, cause } => {
            let mut context = Vec::new();

            push_symbol(&mut context, "subject_kind", "subject", subject.into_any());

            match cause {
                bray_symbols::TypeAssociatedSurfaceBuildError::DuplicateMember(member) => {
                    push_symbol(&mut context, "duplicate_kind", "duplicate", *member);
                }
                bray_symbols::TypeAssociatedSurfaceBuildError::DuplicateImplementation(
                    implementation,
                ) => {
                    push_symbol(
                        &mut context,
                        "duplicate_kind",
                        "duplicate",
                        (*implementation).into(),
                    );
                }
            }

            (
                "semantic_query_type_surface",
                type_surface_reason(cause),
                context,
            )
        }
        SemanticQueryFailure::PreparsedSyntax {
            symbol,
            source: _,
            cause,
        } => {
            let mut context = Vec::new();
            push_symbol(&mut context, "symbol_kind", "symbol", *symbol);
            push_preparsed_syntax_failure(&mut context, cause);

            (
                "semantic_query_preparsed_syntax",
                preparsed_syntax_reason(cause),
                context,
            )
        }
    };

    if let Some(source) = error.source() {
        push_source_span(
            &mut context,
            "source_id",
            "source_start",
            "source_end",
            source,
        );
    }

    DiagnosticSemanticQueryFailure::new(category, reason, context)
}

fn push_contract_violation(
    context: &mut Vec<DiagnosticFailureField>,
    violation: &SemanticQueryViolation,
) -> &'static str {
    use SemanticQueryViolation as Violation;

    match violation {
        Violation::Missing(data) => {
            context.push(text_field("data", semantic_data_kind(*data)));

            "semantic_query_missing_data"
        }
        Violation::UnexpectedSymbolKind { expected, actual } => {
            context.push(text_field("expected", semantic_symbol_category(*expected)));

            context.push(text_field("actual", actual.as_str()));

            "semantic_query_unexpected_symbol_kind"
        }
        Violation::UnexpectedConstantValueKind(actual) => {
            context.push(text_field("actual", constant_value_kind(actual)));

            "semantic_query_unexpected_constant_value_kind"
        }
        Violation::UnexpectedSymbolOrigin(actual) => {
            context.push(text_field("actual", actual.as_str()));

            "semantic_query_unexpected_symbol_origin"
        }
        Violation::GenericParameterKindMismatch { expected, actual } => {
            context.push(text_field("expected", generic_parameter_kind(*expected)));
            context.push(text_field("actual", generic_parameter_kind(*actual)));

            "semantic_query_generic_parameter_kind_mismatch"
        }
        Violation::DuplicateSymbol(symbol) => {
            push_symbol(context, "symbol_kind", "symbol", *symbol);

            "semantic_query_duplicate_symbol"
        }
        Violation::SymbolKindMismatch { expected, actual } => {
            context.push(text_field("expected", expected.as_str()));
            context.push(text_field("actual", actual.as_str()));

            "semantic_query_symbol_kind_mismatch"
        }
        Violation::MissingBoundNode(node) => {
            context.push(identity_field("node", node));

            "semantic_query_missing_bound_node"
        }
        Violation::MissingCheckedTemplateNode(node) => {
            context.push(identity_field("node", node));

            "semantic_query_missing_checked_template_node"
        }
        Violation::UnexpectedBoundUnitRoot { expected, actual } => {
            context.push(text_field("expected", expected.as_str()));
            context.push(identity_field("actual", actual));

            "semantic_query_unexpected_bound_unit_root"
        }
        Violation::UnexpectedWalkOutcome(actual) => {
            match actual {
                bray_bound_tree::BoundWalkOutcome::Completed => {
                    context.push(text_field("actual", "completed"));
                }
                bray_bound_tree::BoundWalkOutcome::Stopped => {
                    context.push(text_field("actual", "stopped"));
                }
                bray_bound_tree::BoundWalkOutcome::MissingNode(node) => {
                    context.push(text_field("actual", "missing_node"));
                    context.push(identity_field("missing_node", node));
                }
            }

            "semantic_query_unexpected_walk_outcome"
        }
        Violation::TypeMismatch { expected, actual } => {
            context.push(identity_field("expected", expected));
            context.push(identity_field("actual", actual));

            "semantic_query_type_mismatch"
        }
        Violation::CountMismatch {
            data,
            expected,
            actual,
        } => {
            context.push(text_field("data", semantic_data_kind(*data)));
            context.push(natural_field("expected", *expected));
            context.push(natural_field("actual", *actual));

            "semantic_query_count_mismatch"
        }
        Violation::CapacityExceeded { data, value } => {
            context.push(text_field("data", semantic_data_kind(*data)));
            context.push(natural_field("value", *value));

            "semantic_query_capacity_exceeded"
        }
        Violation::CountOverflow { data, value } => {
            context.push(text_field("data", semantic_data_kind(*data)));
            context.push(count_field("value", *value));

            "semantic_query_count_overflow"
        }
        Violation::QueryStackMismatch { expected, actual } => {
            push_symbol(context, "expected_kind", "expected", *expected);

            push_optional_symbol(context, "actual_kind", "actual", *actual);

            "semantic_query_stack_mismatch"
        }
        Violation::PackageMismatch { expected, actual } => {
            context.push(text_field("expected", expected.as_str()));
            context.push(text_field("actual", actual.as_str()));

            "semantic_query_package_mismatch"
        }
        Violation::CompilerKnownOperationUnavailable {
            storage,
            target,
            role,
        } => {
            context.push(identity_field("storage", storage));
            context.push(identity_field("target", target));
            context.push(text_field("role", role.as_str()));

            "semantic_query_compiler_known_operation_unavailable"
        }
        Violation::ConstantExpectationMismatch { expected, actual } => {
            push_constant_expectation(context, "expected", *expected);
            push_constant_expectation(context, "actual", *actual);

            "semantic_query_constant_expectation_mismatch"
        }
        Violation::ImportedRecordKindMismatch { expected, actual } => {
            context.push(text_field("expected", expected.as_str()));
            context.push(text_field("actual", actual.as_str()));

            "semantic_query_imported_record_kind_mismatch"
        }
        Violation::UnexpectedOrder(data) => {
            context.push(text_field("data", semantic_data_kind(*data)));

            "semantic_query_unexpected_order"
        }
        Violation::Unsupported(data) => {
            context.push(text_field("data", semantic_data_kind(*data)));

            "semantic_query_unsupported_data"
        }
    }
}

fn push_optional_symbol(
    context: &mut Vec<DiagnosticFailureField>,
    kind_name: &'static str,
    identity_name: &'static str,
    symbol: Option<bray_symbols::AnySymbolId>,
) {
    if let Some(symbol) = symbol {
        push_symbol(context, kind_name, identity_name, symbol);
    }
}

pub(crate) fn push_generic_substitution_failure(
    context: &mut Vec<DiagnosticFailureField>,
    cause: &bray_symbols::GenericSubstitutionShapeError,
) {
    use bray_symbols::GenericSubstitutionShapeError as Error;

    match cause {
        Error::ArgumentCountMismatch {
            parameter_count,
            argument_count,
        } => {
            context.push(natural_field("parameter_count", *parameter_count));
            context.push(natural_field("argument_count", *argument_count));
        }
        Error::ArgumentKindMismatch {
            ordinal,
            expected,
            actual,
        } => {
            context.push(count_field("ordinal", u64::from(*ordinal)));
            context.push(text_field("expected", generic_argument_kind(*expected)));
            context.push(text_field("actual", generic_argument_kind(*actual)));
        }
        Error::OrdinalOverflow => {}
    }
}

pub(crate) fn diagnostic_generic_substitution_failure(
    cause: bray_symbols::GenericSubstitutionShapeError,
) -> bray_diagnostics::DiagnosticGenericSubstitutionFailure {
    use bray_diagnostics::DiagnosticGenericSubstitutionFailure as Diagnostic;
    use bray_symbols::GenericSubstitutionShapeError as Error;

    match cause {
        Error::ArgumentCountMismatch {
            parameter_count,
            argument_count,
        } => Diagnostic::ArgumentCountMismatch {
            parameter_count,
            argument_count,
        },
        Error::ArgumentKindMismatch {
            ordinal,
            expected,
            actual,
        } => Diagnostic::ArgumentKindMismatch {
            ordinal,
            expected: generic_argument_kind(expected),
            actual: generic_argument_kind(actual),
        },
        Error::OrdinalOverflow => Diagnostic::OrdinalOverflow,
    }
}

pub(crate) fn push_semantic_value_failure(
    context: &mut Vec<DiagnosticFailureField>,
    cause: bray_symbols::SemanticValueStoreError,
) {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    match crate::fact::diagnostic_semantic_value_failure(cause) {
        Failure::ForeignId {
            expected_store,
            actual_store,
        } => {
            context.push(count_field("expected_store", expected_store));
            context.push(count_field("actual_store", actual_store));
        }
        Failure::UnknownId { kind } | Failure::CapacityExhausted { kind } => {
            context.push(text_field("value_kind", kind));
        }
        Failure::GenericOwnerMismatch {
            expected_kind,
            expected,
            actual_kind,
            actual,
        } => {
            context.push(text_field("expected_owner_kind", expected_kind));
            context.push(count_field("expected_owner", u64::from(expected)));
            context.push(text_field("actual_owner_kind", actual_kind));
            context.push(count_field("actual_owner", u64::from(actual)));
        }
        Failure::OpenSubstitution => {}
    }
}

fn push_bound_unit_failure(
    context: &mut Vec<DiagnosticFailureField>,
    cause: &bray_bound_tree::BoundUnitBuildError,
) {
    use bray_bound_tree::BoundUnitBuildError as Error;

    match cause {
        Error::MissingRoot { unit, kind } => {
            context.push(count_field("missing_unit", u64::from(unit.raw())));
            context.push(text_field("root_kind", kind.as_str()));
        }
        Error::AnonymousCallableRegionMismatch { expected, actual } => {
            context.push(count_field("expected", u64::from(expected.raw())));
            context.push(count_field("actual", u64::from(actual.raw())));
        }
        Error::MissingAnonymousCallable { callable } => {
            context.push(identity_field("callable", callable));
        }
        Error::InvalidNestedUnit { index } | Error::NonCanonicalNestedUnits { index } => {
            context.push(natural_field("index", *index));
        }
        Error::RootKindMismatch | Error::LocalSymbolRegionMismatch => {}
    }
}

fn push_preparsed_syntax_failure(
    context: &mut Vec<DiagnosticFailureField>,
    cause: &bray_syntax::PreparsedSyntaxFragmentError,
) {
    use bray_syntax::PreparsedSyntaxFragmentError as Error;

    match cause {
        Error::ExpectedNodeKind(kind)
        | Error::ExpectedTokenKind(kind)
        | Error::TokenOutsideNode(kind)
        | Error::UnexpectedExit(kind)
        | Error::UnclosedNode(kind) => context.push(text_field("syntax_kind", kind.as_str())),
        Error::MismatchedExit { expected, actual } => {
            context.push(text_field("expected", expected.as_str()));
            context.push(text_field("actual", actual.as_str()));
        }
        Error::MultipleRoots
        | Error::MissingRoot
        | Error::TextTooLarge
        | Error::GeneratedSourceIdOutOfRange => {}
    }
}

const fn generic_parameter_kind(
    kind: bray_compiler_known::CatalogGenericParameterKind,
) -> &'static str {
    match kind {
        bray_compiler_known::CatalogGenericParameterKind::Type => "type",
        bray_compiler_known::CatalogGenericParameterKind::Const => "const",
    }
}

fn push_constant_expectation(
    context: &mut Vec<DiagnosticFailureField>,
    name: &'static str,
    expectation: bray_symbols::ConstantExpressionExpectedType,
) {
    let (kind_name, symbol_kind_name) = match name {
        "expected" => ("expected_kind", "expected_symbol_kind"),
        "actual" => ("actual_kind", "actual_symbol_kind"),
        _ => unreachable!("constant expectation field name must be closed"),
    };

    match expectation {
        bray_symbols::ConstantExpressionExpectedType::Resolved(ty) => {
            context.push(text_field(kind_name, "resolved"));
            context.push(identity_field(name, &ty));
        }
        bray_symbols::ConstantExpressionExpectedType::GenericParameter(parameter) => {
            context.push(text_field(kind_name, "generic_parameter"));
            push_symbol(context, symbol_kind_name, name, parameter.into());
        }
    }
}

pub(crate) const fn callable_signature_reason(
    cause: &bray_symbols::CallableSignatureTemplateError,
) -> &'static str {
    use bray_symbols::CallableSignatureTemplateError as Error;

    match cause {
        Error::SemanticValue(cause) => semantic_value_reason(*cause),
        Error::InvalidCallableType => "callable_signature_invalid_callable_type",
        Error::ParameterCountMismatch => "callable_signature_parameter_count_mismatch",
        Error::ParameterIdentityMismatch => "callable_signature_parameter_identity_mismatch",
    }
}

pub(crate) const fn generic_substitution_reason(
    cause: &bray_symbols::GenericSubstitutionShapeError,
) -> &'static str {
    use bray_symbols::GenericSubstitutionShapeError as Error;

    match cause {
        Error::ArgumentCountMismatch { .. } => "generic_substitution_argument_count_mismatch",
        Error::ArgumentKindMismatch { .. } => "generic_substitution_argument_kind_mismatch",
        Error::OrdinalOverflow => "generic_substitution_ordinal_overflow",
    }
}

const fn bound_unit_reason(cause: &bray_bound_tree::BoundUnitBuildError) -> &'static str {
    use bray_bound_tree::BoundUnitBuildError as Error;

    match cause {
        Error::RootKindMismatch => "bound_unit_root_kind_mismatch",
        Error::MissingRoot { .. } => "bound_unit_missing_root",
        Error::LocalSymbolRegionMismatch => "bound_unit_local_symbol_region_mismatch",
        Error::AnonymousCallableRegionMismatch { .. } => {
            "bound_unit_anonymous_callable_region_mismatch"
        }
        Error::MissingAnonymousCallable { .. } => "bound_unit_missing_anonymous_callable",
        Error::InvalidNestedUnit { .. } => "bound_unit_invalid_nested_unit",
        Error::NonCanonicalNestedUnits { .. } => "bound_unit_noncanonical_nested_units",
    }
}

const fn implementation_match_reason(
    cause: &crate::compilation::ImplementationMatchError,
) -> &'static str {
    use crate::compilation::ImplementationMatchError as Error;

    match cause {
        Error::InvalidSubstitution(error) => generic_substitution_reason(error),
        Error::SemanticValue(cause) => semantic_value_reason(*cause),
    }
}

const fn semantic_value_reason(cause: bray_symbols::SemanticValueStoreError) -> &'static str {
    use bray_symbols::SemanticValueStoreError as Error;

    match cause {
        Error::ForeignId { .. } => "semantic_value_foreign_id",
        Error::UnknownId { .. } => "semantic_value_unknown_id",
        Error::CapacityExhausted { .. } => "semantic_value_capacity_exhausted",
        Error::GenericOwnerMismatch { .. } => "semantic_value_generic_owner_mismatch",
        Error::OpenSubstitution => "semantic_value_open_substitution",
    }
}

const fn implementation_ambiguity_reason(
    cause: &bray_symbols::ImplementationAmbiguityError,
) -> &'static str {
    use bray_symbols::ImplementationAmbiguityError as Error;

    match cause {
        Error::TooFewCandidates => "implementation_ambiguity_too_few_candidates",
        Error::ConflictingCandidateKey => "implementation_ambiguity_conflicting_candidate_key",
    }
}

const fn implementation_candidate_set_reason(
    cause: &bray_symbols::ImplementationCandidateSetError,
) -> &'static str {
    use bray_symbols::ImplementationCandidateSetError as Error;

    match cause {
        Error::MismatchedCoherenceKey => "implementation_candidate_set_mismatched_coherence_key",
        Error::ConflictingCandidateKey => "implementation_candidate_set_conflicting_candidate_key",
    }
}

const fn implementation_coherence_reason(
    cause: &bray_symbols::ImplementationCoherenceEvidenceError,
) -> &'static str {
    use bray_symbols::ImplementationCoherenceEvidenceError as Error;

    match cause {
        Error::NoImplementations => "implementation_coherence_no_implementations",
        Error::ConflictingParticipantKey => "implementation_coherence_conflicting_participant_key",
    }
}

const fn implementation_candidate_reason(
    cause: &bray_symbols::ImplementationCandidateError,
) -> &'static str {
    use bray_symbols::ImplementationCandidateError as Error;

    match cause {
        Error::MissingCoherenceParticipant => {
            "implementation_candidate_missing_coherence_participant"
        }
        Error::ConflictingConstraintOrdinal => {
            "implementation_candidate_conflicting_constraint_ordinal"
        }
        Error::ConflictingTargetProperty => "implementation_candidate_conflicting_target_property",
    }
}

const fn implementation_participation_reason(
    cause: &bray_symbols::ImplementationParticipationSetError,
) -> &'static str {
    use bray_symbols::ImplementationParticipationSetError as Error;

    match cause {
        Error::DuplicateKey => "implementation_participation_duplicate_key",
        Error::DuplicateImplementation => "implementation_participation_duplicate_implementation",
    }
}

const fn type_surface_reason(
    cause: &bray_symbols::TypeAssociatedSurfaceBuildError,
) -> &'static str {
    use bray_symbols::TypeAssociatedSurfaceBuildError as Error;

    match cause {
        Error::DuplicateMember(_) => "type_surface_duplicate_member",
        Error::DuplicateImplementation(_) => "type_surface_duplicate_implementation",
    }
}

const fn preparsed_syntax_reason(
    cause: &bray_syntax::PreparsedSyntaxFragmentError,
) -> &'static str {
    use bray_syntax::PreparsedSyntaxFragmentError as Error;

    match cause {
        Error::ExpectedNodeKind(_) => "preparsed_syntax_expected_node_kind",
        Error::ExpectedTokenKind(_) => "preparsed_syntax_expected_token_kind",
        Error::TokenOutsideNode(_) => "preparsed_syntax_token_outside_node",
        Error::UnexpectedExit(_) => "preparsed_syntax_unexpected_exit",
        Error::MismatchedExit { .. } => "preparsed_syntax_mismatched_exit",
        Error::MultipleRoots => "preparsed_syntax_multiple_roots",
        Error::UnclosedNode(_) => "preparsed_syntax_unclosed_node",
        Error::MissingRoot => "preparsed_syntax_missing_root",
        Error::TextTooLarge => "preparsed_syntax_text_too_large",
        Error::GeneratedSourceIdOutOfRange => "preparsed_syntax_generated_source_id_out_of_range",
    }
}

const fn generic_argument_kind(kind: bray_symbols::GenericArgumentKind) -> &'static str {
    kind.as_str()
}

const fn semantic_symbol_category(category: SemanticSymbolCategory) -> &'static str {
    match category {
        SemanticSymbolCategory::Callable => "callable",
        SemanticSymbolCategory::CallableParameter => "callable_parameter",
        SemanticSymbolCategory::GenericConstParameter => "generic_const_parameter",
        SemanticSymbolCategory::GenericOwner => "generic_owner",
        SemanticSymbolCategory::GenericParameter => "generic_parameter",
        SemanticSymbolCategory::Implementation => "implementation",
        SemanticSymbolCategory::TraitImplementation => "trait_implementation",
        SemanticSymbolCategory::NamedType => "named_type",
        SemanticSymbolCategory::PredicateDefinition => "predicate_definition",
        SemanticSymbolCategory::RuntimeDefaultProvider => "runtime_default_provider",
        SemanticSymbolCategory::RuntimeDefaultSubject => "runtime_default_subject",
        SemanticSymbolCategory::StructField => "struct_field",
        SemanticSymbolCategory::UnionPayloadField => "union_payload_field",
        SemanticSymbolCategory::TrustedCapability => "trusted_capability",
    }
}

const fn semantic_data_kind(kind: SemanticDataKind) -> &'static str {
    match kind {
        SemanticDataKind::BoundExpression => "bound_expression",
        SemanticDataKind::BoundUnit => "bound_unit",
        SemanticDataKind::CallableSignature => "callable_signature",
        SemanticDataKind::ConstantDefinition => "constant_definition",
        SemanticDataKind::ConstantTerm => "constant_term",
        SemanticDataKind::DeclarationRecord => "declaration_record",
        SemanticDataKind::DependencyContract => "dependency_contract",
        SemanticDataKind::GenericConstraint => "generic_constraint",
        SemanticDataKind::GenericSubstitution => "generic_substitution",
        SemanticDataKind::Implementation => "implementation",
        SemanticDataKind::ImplementationCandidate => "implementation_candidate",
        SemanticDataKind::ImplementationComparison => "implementation_comparison",
        SemanticDataKind::ImplementationUsing => "implementation_using",
        SemanticDataKind::ImportedTemplate => "imported_template",
        SemanticDataKind::IterationProtocol => "iteration_protocol",
        SemanticDataKind::IterationSource => "iteration_source",
        SemanticDataKind::LiteralValue => "literal_value",
        SemanticDataKind::MemberName => "member_name",
        SemanticDataKind::OperationSelection => "operation_selection",
        SemanticDataKind::OverloadComparison => "overload_comparison",
        SemanticDataKind::RuntimeDefault => "runtime_default",
        SemanticDataKind::SourceAnchor => "source_anchor",
        SemanticDataKind::SourceSnapshot => "source_snapshot",
        SemanticDataKind::Symbol => "symbol",
        SemanticDataKind::SymbolKey => "symbol_key",
        SemanticDataKind::SymbolQuery(_) => "symbol_query",
        SemanticDataKind::Syntax => "syntax",
        SemanticDataKind::ContainingModule => "containing_module",
        SemanticDataKind::TraitApplication => "trait_application",
        SemanticDataKind::Type => "type",
        SemanticDataKind::LifecycleMember => "lifecycle_member",
        SemanticDataKind::TypeSurface => "type_surface",
        SemanticDataKind::Diagnostic => "diagnostic",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;
    use bray_symbols::{
        CallableSignatureTemplateError, GenericSubstitutionShapeError, SemanticValueKind,
        SemanticValueStoreError,
    };

    use super::diagnostic_semantic_query_failure;
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::CompilationFactKey;

    #[test]
    fn semantic_query_diagnostics_preserve_exact_reason_and_typed_counts() {
        let error = SemanticQueryFailure::contract(
            SemanticQueryContext::Fact(CompilationFactKey::SyntaxTree),
            SemanticQueryViolation::CountMismatch {
                data: SemanticDataKind::Type,
                expected: 3,
                actual: 4,
            },
        )
        .into();

        let diagnostic = diagnostic_semantic_query_failure(&error);

        assert_eq!(diagnostic.category(), "semantic_query_contract_violation");
        assert_eq!(diagnostic.reason(), "semantic_query_count_mismatch");

        assert!(
            diagnostic
                .context()
                .contains(&bray_diagnostics::DiagnosticFailureField::new(
                    "expected",
                    DiagnosticFailureValue::Natural("3".to_owned()),
                ))
        );

        assert!(
            diagnostic
                .context()
                .contains(&bray_diagnostics::DiagnosticFailureField::new(
                    "actual",
                    DiagnosticFailureValue::Natural("4".to_owned()),
                ))
        );

        assert!(
            !diagnostic
                .context()
                .iter()
                .any(|field| field.name() == "cause")
        );
    }

    #[test]
    fn semantic_context_preserves_compiler_known_representation_role() {
        let error = SemanticQueryFailure::contract(
            SemanticQueryContext::CompilerKnownRepresentation(
                bray_compiler_known::RepresentationRole::RawPointer,
            ),
            SemanticQueryViolation::Missing(SemanticDataKind::Type),
        )
        .into();

        let diagnostic = diagnostic_semantic_query_failure(&error);

        let names = diagnostic
            .context()
            .iter()
            .map(|field| field.name())
            .collect::<Vec<_>>();

        assert_eq!(names, ["context_kind", "representation_role", "data"]);

        assert_eq!(
            diagnostic.context()[1].value(),
            &DiagnosticFailureValue::Text("RawPointer".to_owned())
        );
    }

    #[test]
    fn nested_semantic_query_failures_preserve_leaf_reasons_and_payloads() {
        let callable = SemanticQueryFailure::CallableSignature {
            callable: None,
            cause: CallableSignatureTemplateError::SemanticValue(
                SemanticValueStoreError::UnknownId {
                    kind: SemanticValueKind::Type,
                },
            ),
        }
        .into();

        let callable = diagnostic_semantic_query_failure(&callable);

        assert_eq!(callable.reason(), "semantic_value_unknown_id");

        assert!(
            callable
                .context()
                .contains(&bray_diagnostics::DiagnosticFailureField::new(
                    "value_kind",
                    DiagnosticFailureValue::Text("type".to_owned()),
                ))
        );

        let implementation = SemanticQueryFailure::ImplementationMatch {
            implementation: None,
            cause: crate::compilation::ImplementationMatchError::InvalidSubstitution(
                GenericSubstitutionShapeError::ArgumentCountMismatch {
                    parameter_count: 2,
                    argument_count: 3,
                },
            ),
        }
        .into();

        let implementation = diagnostic_semantic_query_failure(&implementation);

        assert_eq!(
            implementation.reason(),
            "generic_substitution_argument_count_mismatch"
        );

        assert!(
            implementation
                .context()
                .contains(&bray_diagnostics::DiagnosticFailureField::new(
                    "parameter_count",
                    DiagnosticFailureValue::Natural("2".to_owned()),
                ))
        );

        assert!(
            implementation
                .context()
                .contains(&bray_diagnostics::DiagnosticFailureField::new(
                    "argument_count",
                    DiagnosticFailureValue::Natural("3".to_owned()),
                ))
        );
    }
}
