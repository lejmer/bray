use bray_bound_tree::{
    BoundReferenceTarget, CheckedExpressionTypes, DeclaredValueTypeTerm, SelectedCall,
    SelectedOperation, SelectionKind,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticCallableArgumentRejection,
    DiagnosticConstructionInputRejection, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticReceiverCapability, DiagnosticReceiverMode,
    DiagnosticRejectedSelectionCandidate, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
    DiagnosticSelectionCandidate, DiagnosticSelectionCandidateIdentity,
    DiagnosticSelectionCandidateSignature, DiagnosticSelectionCandidates, DiagnosticSelectionKind,
    DiagnosticSelectionRejectionReason, DiagnosticSelectionRejections, SeverityKind,
};

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::{
    CallableCandidate, CallableSelectionRequest, CandidateSelection,
    IterationSourceSelectionRequest, OperationSelectionRequest, SelectionCallableArgumentRejection,
    SelectionCandidateKey, SelectionCandidateRejectionReason, SelectionCandidateSignature,
    SelectionConstructionInputRejection, SelectionFailure, SelectionFailureCandidate,
    SelectionRejectedCandidate,
};

pub(crate) fn select_callable<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: CallableSelectionRequest,
) -> CheckerOutcome<CandidateSelection<SelectedCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let expression = input.expression();

    callable_outcome(
        request,
        expression,
        super::call::select(request, types, input),
    )
}

pub(crate) fn select_callable_candidates<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: &CallableSelectionRequest,
    candidates: &[CallableCandidate],
) -> CheckerOutcome<CandidateSelection<SelectedCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let result = super::call::select_candidates(request, types, input, candidates);

    callable_outcome(request, input.expression(), result)
}

fn callable_outcome<C, Error>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    result: Result<Option<CandidateSelection<SelectedCall>>, Error>,
) -> CheckerOutcome<CandidateSelection<SelectedCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
    Error: Into<CheckerQueryError<C::UpstreamError>>,
{
    match result {
        Ok(Some(selection)) => complete(
            request,
            expression,
            DiagnosticSelectionKind::Callable,
            selection,
        ),
        Ok(None) => CheckerOutcome::Cancelled,
        Err(error) => query_outcome(error),
    }
}

fn query_outcome<T, Upstream>(
    error: impl Into<CheckerQueryError<Upstream>>,
) -> CheckerOutcome<T, Upstream> {
    match error.into() {
        CheckerQueryError::Cancelled => CheckerOutcome::Cancelled,
        CheckerQueryError::Infrastructure(error) => CheckerOutcome::InfrastructureFailure(error),
        CheckerQueryError::Upstream(error) => CheckerOutcome::UpstreamFailure(error),
    }
}

pub(crate) fn select_operation<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: OperationSelectionRequest,
) -> CheckerOutcome<CandidateSelection<SelectedOperation>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let expression = input.expression();
    let kind = input.kind();

    match super::operation::select(request, types, input) {
        Ok(Some(selection)) => complete(
            request,
            expression,
            diagnostic_selection_kind(kind),
            selection,
        ),
        Ok(None) => CheckerOutcome::Cancelled,
        Err(error) => query_outcome(error),
    }
}

pub(crate) fn select_iteration_source<C>(
    request: CheckerUnitView<'_, C>,
    input: &IterationSourceSelectionRequest,
) -> CheckerOutcome<CandidateSelection<bray_bound_tree::SelectedIterationSource>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    match super::iteration::select(input) {
        Ok(selection) => complete(
            request,
            input.expression(),
            DiagnosticSelectionKind::IterationSource,
            selection,
        ),
        Err(error) => query_outcome(error),
    }
}

fn complete<C, T>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    kind: DiagnosticSelectionKind,
    selection: CandidateSelection<T>,
) -> CheckerOutcome<CandidateSelection<T>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let CandidateSelection::Failed(failure) = &selection else {
        return CheckerOutcome::without_diagnostics(selection);
    };

    let Some(diagnostic_kind) = failure_diagnostic_kind(failure) else {
        return CheckerOutcome::without_diagnostics(selection);
    };

    let span = match expression_span(request, expression) {
        Ok(span) => span,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let mut diagnostic = Diagnostic::new(diagnostic_id(0), diagnostic_kind, SeverityKind::Error)
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::SelectionFailure,
            span,
        ))
        .with_arg(DiagnosticArg::selection_kind(kind));

    if let Some(candidates) = failure_candidates(failure) {
        let retained = candidates
            .iter()
            .take(DiagnosticSelectionCandidates::MAXIMUM)
            .map(|candidate| diagnostic_candidate(request, candidate))
            .collect::<Result<Vec<_>, _>>();

        let retained = match retained {
            Ok(candidates) => candidates,
            Err(error) => return query_outcome(error),
        };

        let diagnostic_candidates =
            DiagnosticSelectionCandidates::try_from_prefix(retained, candidates.len());

        let diagnostic_candidates = match diagnostic_candidates {
            Ok(candidates) => candidates,
            Err(_) => {
                return CheckerOutcome::InfrastructureFailure(
                    crate::CheckerInfrastructureError::SemanticValueUnavailable,
                );
            }
        };

        diagnostic =
            diagnostic.with_arg(DiagnosticArg::selection_candidates(diagnostic_candidates));

        let related = match selection_candidate_locations(
            request,
            &candidates[..candidates.len().min(DiagnosticSelectionCandidates::MAXIMUM)],
            span,
        ) {
            Ok(related) => related,
            Err(error) => return query_outcome(error),
        };

        for location in related {
            diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::SelectionCandidate,
                location,
            ));
        }
    }

    if let SelectionFailure::Incompatible(rejections) = failure {
        let retained = rejections
            .iter()
            .take(DiagnosticSelectionRejections::MAXIMUM)
            .map(|rejection| diagnostic_rejection(request, rejection))
            .collect::<Result<Vec<_>, _>>();

        let retained = match retained {
            Ok(rejections) => rejections,
            Err(error) => return query_outcome(error),
        };

        let diagnostic_rejections =
            match DiagnosticSelectionRejections::try_from_prefix(retained, rejections.len()) {
                Ok(rejections) => rejections,
                Err(_) => {
                    return CheckerOutcome::InfrastructureFailure(
                        crate::CheckerInfrastructureError::SemanticValueUnavailable,
                    );
                }
            };

        diagnostic =
            diagnostic.with_arg(DiagnosticArg::selection_rejections(diagnostic_rejections));

        let retained_candidates = rejections
            .iter()
            .take(DiagnosticSelectionRejections::MAXIMUM)
            .map(SelectionRejectedCandidate::candidate)
            .cloned()
            .collect::<Vec<_>>();

        let related = match selection_candidate_locations(request, &retained_candidates, span) {
            Ok(related) => related,
            Err(error) => return query_outcome(error),
        };

        for location in related {
            diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::SelectionCandidate,
                location,
            ));
        }
    }

    match failure {
        SelectionFailure::Ambiguous(_) => {
            diagnostic = diagnostic.with_note(DiagnosticNote::new(
                DiagnosticNoteKind::SelectionMustBeDisambiguated,
            ));
        }
        SelectionFailure::Inaccessible { .. } => {}
        SelectionFailure::Unavailable
        | SelectionFailure::Incompatible(_)
        | SelectionFailure::Recovered => {}
    }

    CheckerOutcome::complete(selection, DiagnosticBag::single(diagnostic))
}

fn diagnostic_rejection<C>(
    request: CheckerUnitView<'_, C>,
    rejection: &SelectionRejectedCandidate,
) -> Result<DiagnosticRejectedSelectionCandidate, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let candidate = diagnostic_candidate(request, rejection.candidate())?;
    let reason = diagnostic_rejection_reason(request, rejection.reason())?;

    Ok(DiagnosticRejectedSelectionCandidate::new(candidate, reason))
}

fn diagnostic_rejection_reason<C>(
    request: CheckerUnitView<'_, C>,
    reason: &SelectionCandidateRejectionReason,
) -> Result<DiagnosticSelectionRejectionReason, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let reason = match reason {
        SelectionCandidateRejectionReason::GenericArgumentCount { provided, maximum } => {
            DiagnosticSelectionRejectionReason::GenericArgumentCount {
                provided: *provided,
                maximum: *maximum,
            }
        }
        SelectionCandidateRejectionReason::ReceiverPresence { provided, required } => {
            DiagnosticSelectionRejectionReason::ReceiverPresence {
                provided: *provided,
                required: *required,
            }
        }
        SelectionCandidateRejectionReason::ReceiverType { provided, required } => {
            DiagnosticSelectionRejectionReason::ReceiverType {
                provided: crate::diagnostic::diagnostic_type(request.context(), *provided)?,
                required: crate::diagnostic::diagnostic_type(request.context(), *required)?,
            }
        }
        SelectionCandidateRejectionReason::ReceiverCapability { provided, required } => {
            DiagnosticSelectionRejectionReason::ReceiverCapability {
                provided: match provided {
                    super::ReceiverCapability::Shared => DiagnosticReceiverCapability::Shared,
                    super::ReceiverCapability::Mutable => DiagnosticReceiverCapability::Mutable,
                    super::ReceiverCapability::Owned => DiagnosticReceiverCapability::Owned,
                    super::ReceiverCapability::OwnedMutable => {
                        DiagnosticReceiverCapability::OwnedMutable
                    }
                },
                required: match required {
                    bray_symbols::ReceiverMode::Shared => DiagnosticReceiverMode::Shared,
                    bray_symbols::ReceiverMode::Mutable => DiagnosticReceiverMode::Mutable,
                    bray_symbols::ReceiverMode::Consuming => DiagnosticReceiverMode::Consuming,
                    bray_symbols::ReceiverMode::ConsumingMutable => {
                        DiagnosticReceiverMode::ConsumingMutable
                    }
                },
            }
        }
        SelectionCandidateRejectionReason::CallableArgument(reason) => {
            DiagnosticSelectionRejectionReason::CallableArgument(
                diagnostic_callable_argument_rejection(request, reason)?,
            )
        }
        SelectionCandidateRejectionReason::OperandTypes { provided } => {
            DiagnosticSelectionRejectionReason::OperandTypes {
                provided: provided
                    .iter()
                    .copied()
                    .map(|ty| crate::diagnostic::diagnostic_type(request.context(), ty))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_boxed_slice(),
            }
        }
        SelectionCandidateRejectionReason::ConstructionInput(reason) => {
            DiagnosticSelectionRejectionReason::ConstructionInput(
                diagnostic_construction_input_rejection(request, reason)?,
            )
        }
        SelectionCandidateRejectionReason::ExpressionForm => {
            DiagnosticSelectionRejectionReason::ExpressionForm
        }
        SelectionCandidateRejectionReason::RequiredImplementation => {
            DiagnosticSelectionRejectionReason::RequiredImplementation
        }
        SelectionCandidateRejectionReason::RequiredLanguageOperation => {
            DiagnosticSelectionRejectionReason::RequiredLanguageOperation
        }
    };

    Ok(reason)
}

fn diagnostic_callable_argument_rejection<C>(
    request: CheckerUnitView<'_, C>,
    reason: &SelectionCallableArgumentRejection,
) -> Result<DiagnosticCallableArgumentRejection, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let reason = match reason {
        SelectionCallableArgumentRejection::PositionalAfterNamed { ordinal } => {
            DiagnosticCallableArgumentRejection::PositionalAfterNamed { ordinal: *ordinal }
        }
        SelectionCallableArgumentRejection::UnknownName { provided, accepted } => {
            DiagnosticCallableArgumentRejection::UnknownName {
                provided: provided.clone(),
                accepted: accepted.to_vec().into_boxed_slice(),
            }
        }
        SelectionCallableArgumentRejection::PositionalUnavailable { ordinal } => {
            DiagnosticCallableArgumentRejection::PositionalUnavailable { ordinal: *ordinal }
        }
        SelectionCallableArgumentRejection::Duplicate { name, ordinal } => {
            DiagnosticCallableArgumentRejection::Duplicate {
                name: name.clone(),
                ordinal: *ordinal,
            }
        }
        SelectionCallableArgumentRejection::Type {
            name,
            ordinal,
            expected,
            actual,
        } => DiagnosticCallableArgumentRejection::Type {
            name: name.clone(),
            ordinal: *ordinal,
            expected: crate::diagnostic::diagnostic_type(request.context(), *expected)?,
            actual: crate::diagnostic::diagnostic_type(request.context(), *actual)?,
        },
        SelectionCallableArgumentRejection::Missing {
            name,
            ordinal,
            expected,
        } => DiagnosticCallableArgumentRejection::Missing {
            name: name.clone(),
            ordinal: *ordinal,
            expected: crate::diagnostic::diagnostic_type(request.context(), *expected)?,
        },
    };

    Ok(reason)
}

fn diagnostic_construction_input_rejection<C>(
    request: CheckerUnitView<'_, C>,
    reason: &SelectionConstructionInputRejection,
) -> Result<DiagnosticConstructionInputRejection, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let reason = match reason {
        SelectionConstructionInputRejection::PositionalAfterNamed => {
            DiagnosticConstructionInputRejection::PositionalAfterNamed
        }
        SelectionConstructionInputRejection::UnknownName { provided, accepted } => {
            DiagnosticConstructionInputRejection::UnknownName {
                provided: provided.clone(),
                accepted: accepted.to_vec().into_boxed_slice(),
            }
        }
        SelectionConstructionInputRejection::PositionalUnavailable { ordinal } => {
            DiagnosticConstructionInputRejection::PositionalUnavailable { ordinal: *ordinal }
        }
        SelectionConstructionInputRejection::Duplicate { name, ordinal } => {
            DiagnosticConstructionInputRejection::Duplicate {
                name: name.clone(),
                ordinal: *ordinal,
            }
        }
        SelectionConstructionInputRejection::Type {
            name,
            ordinal,
            expected,
            actual,
        } => DiagnosticConstructionInputRejection::Type {
            name: name.clone(),
            ordinal: *ordinal,
            expected: crate::diagnostic::diagnostic_type(request.context(), *expected)?,
            actual: crate::diagnostic::diagnostic_type(request.context(), *actual)?,
        },
        SelectionConstructionInputRejection::Missing {
            name,
            ordinal,
            expected,
        } => DiagnosticConstructionInputRejection::Missing {
            name: name.clone(),
            ordinal: *ordinal,
            expected: crate::diagnostic::diagnostic_type(request.context(), *expected)?,
        },
    };

    Ok(reason)
}

fn failure_candidates(failure: &SelectionFailure) -> Option<&[SelectionFailureCandidate]> {
    match failure {
        SelectionFailure::Ambiguous(candidates)
        | SelectionFailure::Inaccessible { candidates, .. } => Some(candidates),
        SelectionFailure::Unavailable
        | SelectionFailure::Incompatible(_)
        | SelectionFailure::Recovered => None,
    }
}

fn diagnostic_candidate<C>(
    request: CheckerUnitView<'_, C>,
    candidate: &SelectionFailureCandidate,
) -> Result<DiagnosticSelectionCandidate, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let identity = diagnostic_candidate_identity(request, candidate.key())?;

    let signature = match candidate.signature() {
        SelectionCandidateSignature::Callable {
            callable_type,
            result_type,
        } => {
            let callable = request
                .semantic_values()
                .type_data(*callable_type)
                .map_err(|error| crate::CheckerInfrastructureError::SemanticValueStore(error))?;

            let bray_symbols::TypeData::Callable(callable) = callable.as_ref() else {
                return Err(
                    crate::CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
                );
            };

            DiagnosticSelectionCandidateSignature::Callable {
                parameter_types: callable
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        crate::diagnostic::diagnostic_type(request.context(), parameter.ty())
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_boxed_slice(),
                result_type: crate::diagnostic::diagnostic_type(request.context(), *result_type)?,
            }
        }
        SelectionCandidateSignature::Operation {
            operand_types,
            result_type,
        } => DiagnosticSelectionCandidateSignature::Operation {
            operand_types: operand_types
                .iter()
                .copied()
                .map(|ty| crate::diagnostic::diagnostic_type(request.context(), ty))
                .collect::<Result<Vec<_>, _>>()?
                .into_boxed_slice(),
            result_type: result_type
                .map(|ty| crate::diagnostic::diagnostic_type(request.context(), ty))
                .transpose()?,
        },
        SelectionCandidateSignature::Iteration {
            source_type,
            cursor_type,
            element_type,
        } => DiagnosticSelectionCandidateSignature::Iteration {
            source_type: crate::diagnostic::diagnostic_type(request.context(), *source_type)?,
            cursor_type: crate::diagnostic::diagnostic_type(request.context(), *cursor_type)?,
            element_type: crate::diagnostic::diagnostic_type(request.context(), *element_type)?,
        },
    };

    Ok(DiagnosticSelectionCandidate::new(identity, signature))
}

fn diagnostic_candidate_identity<C>(
    request: CheckerUnitView<'_, C>,
    key: &SelectionCandidateKey,
) -> Result<DiagnosticSelectionCandidateIdentity, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let identity = match key {
        SelectionCandidateKey::BuiltIn => DiagnosticSelectionCandidateIdentity::BuiltIn,
        SelectionCandidateKey::Symbol(key) => {
            let identity = bray_symbols::diagnostic_symbol_identity(key);

            match request.symbols().symbol_for_key(key) {
                Some(symbol) => match diagnostic_member_name(request, symbol)? {
                    Some(name) => {
                        DiagnosticSelectionCandidateIdentity::NamedDeclaration { identity, name }
                    }
                    None => DiagnosticSelectionCandidateIdentity::Declaration(identity),
                },
                None => DiagnosticSelectionCandidateIdentity::Declaration(identity),
            }
        }
        SelectionCandidateKey::Iteration { iterable, iterator } => {
            DiagnosticSelectionCandidateIdentity::Iteration {
                iterable: bray_symbols::diagnostic_symbol_identity(iterable),
                iterator: bray_symbols::diagnostic_symbol_identity(iterator),
            }
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Expression(_)) => {
            DiagnosticSelectionCandidateIdentity::ExpressionValue
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Pattern(_)) => {
            DiagnosticSelectionCandidateIdentity::PatternValue
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Value(
            BoundReferenceTarget::Local(_),
        )) => DiagnosticSelectionCandidateIdentity::LocalValue,
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Value(
            BoundReferenceTarget::Surface(symbol),
        )) => {
            let Some(key) = request.symbols().symbol_key(*symbol) else {
                return Err(crate::CheckerInfrastructureError::SemanticValueUnavailable.into());
            };

            let identity = bray_symbols::diagnostic_symbol_identity(key);

            match diagnostic_member_name(request, *symbol)? {
                Some(name) => {
                    DiagnosticSelectionCandidateIdentity::NamedSurfaceValue { identity, name }
                }
                None => DiagnosticSelectionCandidateIdentity::SurfaceValue(identity),
            }
        }
    };

    Ok(identity)
}

fn diagnostic_member_name<C>(
    request: CheckerUnitView<'_, C>,
    symbol: bray_symbols::AnySymbolId,
) -> Result<Option<String>, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    match request.member_name(symbol) {
        Ok(name) => Ok(name.map(|name| name.as_str().to_owned())),
        Err(crate::CheckerQueryError::Cancelled) => Ok(None),
        Err(crate::CheckerQueryError::Infrastructure(error)) => {
            Err(crate::CheckerQueryError::Infrastructure(error))
        }
        Err(crate::CheckerQueryError::Upstream(error)) => {
            Err(crate::CheckerQueryError::Upstream(error))
        }
    }
}

fn selection_candidate_locations<C>(
    request: CheckerUnitView<'_, C>,
    candidates: &[SelectionFailureCandidate],
    primary: bray_source::SourceSpan,
) -> Result<Vec<bray_source::SourceSpan>, crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut locations = std::collections::BTreeSet::new();

    for candidate in candidates {
        candidate_source_locations(request, candidate.key(), &mut locations)?;
    }

    locations.remove(&primary);

    Ok(locations.into_iter().collect())
}

fn candidate_source_locations<C>(
    request: CheckerUnitView<'_, C>,
    key: &SelectionCandidateKey,
    locations: &mut std::collections::BTreeSet<bray_source::SourceSpan>,
) -> Result<(), crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    match key {
        SelectionCandidateKey::BuiltIn => {}
        SelectionCandidateKey::Symbol(key) => {
            add_symbol_key_location(request, key, locations)?;
        }
        SelectionCandidateKey::Iteration { iterable, iterator } => {
            add_symbol_key_location(request, iterable, locations)?;
            add_symbol_key_location(request, iterator, locations)?;
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Expression(expression)) => {
            let Some(expression) = request.view().expression(*expression) else {
                return Err(
                    crate::CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
                );
            };

            locations.insert(request.source(expression.origin().source_anchor())?.span());
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Pattern(pattern)) => {
            let Some(pattern) = request.view().pattern(*pattern) else {
                return Err(
                    crate::CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
                );
            };

            locations.insert(request.source(pattern.origin().source_anchor())?.span());
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Value(
            BoundReferenceTarget::Local(symbol),
        )) => {
            let Some(anchor) = request.unit().local_symbols().syntax_anchor(*symbol) else {
                return Err(
                    crate::CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
                );
            };

            locations.insert(request.source_syntax(anchor)?.span());
        }
        SelectionCandidateKey::Value(DeclaredValueTypeTerm::Value(
            BoundReferenceTarget::Surface(symbol),
        )) => {
            if let Some(anchor) = request.symbols().declaration_syntax_anchor(*symbol) {
                locations.insert(request.source_syntax(anchor)?.span());
            }
        }
    }

    Ok(())
}

fn add_symbol_key_location<C>(
    request: CheckerUnitView<'_, C>,
    key: &bray_symbols::SymbolKey,
    locations: &mut std::collections::BTreeSet<bray_source::SourceSpan>,
) -> Result<(), crate::CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(symbol) = request.symbols().symbol_for_key(key) else {
        return Ok(());
    };

    let Some(anchor) = request.symbols().declaration_syntax_anchor(symbol) else {
        return Ok(());
    };

    locations.insert(request.source_syntax(anchor)?.span());

    Ok(())
}

const fn failure_diagnostic_kind(failure: &SelectionFailure) -> Option<DiagnosticKind> {
    match failure {
        SelectionFailure::Unavailable => Some(DiagnosticKind::CheckingNoApplicableCandidate),
        SelectionFailure::Ambiguous(_) => Some(DiagnosticKind::CheckingAmbiguousCandidate),
        SelectionFailure::Inaccessible { .. } => None,
        SelectionFailure::Incompatible(_) => Some(DiagnosticKind::CheckingIncompatibleCandidate),
        SelectionFailure::Recovered => None,
    }
}

const fn diagnostic_selection_kind(kind: SelectionKind) -> DiagnosticSelectionKind {
    match kind {
        SelectionKind::Callable => DiagnosticSelectionKind::Callable,
        SelectionKind::Member => DiagnosticSelectionKind::Member,
        SelectionKind::Operator => DiagnosticSelectionKind::Operator,
        SelectionKind::Index => DiagnosticSelectionKind::Index,
        SelectionKind::Construction => DiagnosticSelectionKind::Construction,
        SelectionKind::Conversion => DiagnosticSelectionKind::Conversion,
        SelectionKind::Implementation => DiagnosticSelectionKind::Implementation,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use super::failure_diagnostic_kind;
    use crate::selection::{SelectionFailure, SelectionInaccessibility};

    #[test]
    fn selection_failures_map_to_exact_structured_diagnostics() {
        let cases = [
            (
                SelectionFailure::Unavailable,
                Some(DiagnosticKind::CheckingNoApplicableCandidate),
            ),
            (
                SelectionFailure::Ambiguous([].into()),
                Some(DiagnosticKind::CheckingAmbiguousCandidate),
            ),
            (
                SelectionFailure::Inaccessible {
                    candidates: [].into(),
                    reason: SelectionInaccessibility::NotVisibleFromRequestingContext,
                },
                None,
            ),
            (
                SelectionFailure::Incompatible([].into()),
                Some(DiagnosticKind::CheckingIncompatibleCandidate),
            ),
            (SelectionFailure::Recovered, None),
        ];

        for (failure, expected) in cases {
            assert_eq!(failure_diagnostic_kind(&failure), expected);
        }
    }
}
