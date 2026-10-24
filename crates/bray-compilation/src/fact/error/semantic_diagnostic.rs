use bray_diagnostics::{DiagnosticFailureField, DiagnosticSemanticQueryFailure};

use super::diagnostic_context::{count_field, identity_field, natural_field, text_field};
use super::semantic_context::semantic_query_context;
use crate::compilation::{SemanticDataKind, SemanticQueryError, SemanticQueryFailure};

pub(crate) fn diagnostic_semantic_query_failure(
    error: &SemanticQueryError,
) -> DiagnosticSemanticQueryFailure {
    let (reason, context) = match error.cause() {
        SemanticQueryFailure::Contract(failure) => {
            let mut context = semantic_query_context(failure.context());

            context.push(text_field("data", semantic_data_kind(failure.data())));

            context.push(DiagnosticFailureField::new(
                "value",
                bray_diagnostics::DiagnosticFailureValue::Natural(failure.value().to_string()),
            ));

            ("semantic_query_capacity_exceeded", context)
        }
        SemanticQueryFailure::GenericSubstitution { owner, .. } => {
            let context = owner
                .iter()
                .map(|owner| identity_field("owner", owner))
                .collect();

            ("generic_substitution_ordinal_overflow", context)
        }
    };

    DiagnosticSemanticQueryFailure::new("semantic_query_capacity", reason, context)
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

pub(crate) fn push_semantic_value_failure(
    context: &mut Vec<DiagnosticFailureField>,
    cause: bray_symbols::SemanticValueStoreError,
) {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    match crate::fact::diagnostic_semantic_value_failure(cause) {
        Failure::CapacityExhausted { kind } => context.push(text_field("value_kind", kind)),
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

const fn generic_argument_kind(kind: bray_symbols::GenericArgumentKind) -> &'static str {
    kind.as_str()
}

const fn semantic_data_kind(kind: SemanticDataKind) -> &'static str {
    match kind {
        SemanticDataKind::BoundExpression => "bound_expression",
        SemanticDataKind::BoundUnit => "bound_unit",
        SemanticDataKind::CallableSignature => "callable_signature",
        SemanticDataKind::ConstantDefinition => "constant_definition",
        SemanticDataKind::ConstantTerm => "constant_term",
        SemanticDataKind::DeclarationRecord => "declaration_record",
        SemanticDataKind::ExecutionEvidence => "execution_evidence",
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
