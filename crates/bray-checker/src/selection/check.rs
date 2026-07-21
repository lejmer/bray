use bray_bound_tree::{CheckedExpressionTypes, SelectedCall, SelectedOperation, SelectionKind};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticSelectionKind, SeverityKind,
};

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::{
    CallableCandidate, CallableSelectionRequest, CandidateSelection, OperationSelectionRequest,
    SelectionFailure,
};

pub(crate) fn select_callable<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: CallableSelectionRequest,
) -> CheckerOutcome<CandidateSelection<SelectedCall>>
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
) -> CheckerOutcome<CandidateSelection<SelectedCall>>
where
    C: CheckerRequestContext + ?Sized,
{
    let result = super::call::select_candidates(request, types, input, candidates);

    callable_outcome(request, input.expression(), result)
}

fn callable_outcome<C>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    result: Result<Option<CandidateSelection<SelectedCall>>, crate::CheckerInfrastructureError>,
) -> CheckerOutcome<CandidateSelection<SelectedCall>>
where
    C: CheckerRequestContext + ?Sized,
{
    match result {
        Ok(Some(selection)) => complete(request, expression, SelectionKind::Callable, selection),
        Ok(None) => CheckerOutcome::Cancelled,
        Err(error) => CheckerOutcome::InfrastructureFailure(error),
    }
}

pub(crate) fn select_operation<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: OperationSelectionRequest,
) -> CheckerOutcome<CandidateSelection<SelectedOperation>>
where
    C: CheckerRequestContext + ?Sized,
{
    let expression = input.expression();
    let kind = input.kind();

    match super::operation::select(request, types, input) {
        Ok(Some(selection)) => complete(request, expression, kind, selection),
        Ok(None) => CheckerOutcome::Cancelled,
        Err(error) => CheckerOutcome::InfrastructureFailure(error),
    }
}

fn complete<C, T>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    kind: SelectionKind,
    selection: CandidateSelection<T>,
) -> CheckerOutcome<CandidateSelection<T>>
where
    C: CheckerRequestContext + ?Sized,
{
    let CandidateSelection::Failed(failure) = &selection else {
        return CheckerOutcome::without_diagnostics(selection);
    };

    let Some(diagnostic_kind) = (match failure {
        SelectionFailure::Unavailable => Some(DiagnosticKind::CheckingNoApplicableCandidate),
        SelectionFailure::Ambiguous(_) => Some(DiagnosticKind::CheckingAmbiguousCandidate),
        SelectionFailure::Inaccessible => Some(DiagnosticKind::CheckingInaccessibleCandidate),
        SelectionFailure::Incompatible => Some(DiagnosticKind::CheckingIncompatibleCandidate),
        SelectionFailure::Recovered => None,
    }) else {
        return CheckerOutcome::without_diagnostics(selection);
    };

    let span = match expression_span(request, expression) {
        Ok(span) => span,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let diagnostic = Diagnostic::new(diagnostic_id(0), diagnostic_kind, SeverityKind::Error)
        .with_primary_span(span)
        .with_arg(DiagnosticArg::selection_kind(diagnostic_selection_kind(
            kind,
        )));

    CheckerOutcome::complete(selection, DiagnosticBag::single(diagnostic))
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
