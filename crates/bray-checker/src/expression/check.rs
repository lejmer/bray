use bray_bound_tree::{CheckedSemanticSelections, DeclaredValueTypeTemplates};

use super::candidate::{converge, final_selections, prepare_calls};
use super::declared::{PreparedDeclaredTypes, defer_return_operands, prepare_declared_types};
use crate::type_check::{
    ExpressionTypeSession, SessionProgress, finish_expression_types_with_deferred,
};
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView,
    ExpressionCandidateSet,
};

pub(crate) fn check_expression_semantics<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    candidate_sets: &[ExpressionCandidateSet],
) -> CheckerOutcome<(
    bray_bound_tree::CheckedExpressionTypes,
    CheckedSemanticSelections,
)>
where
    C: CheckerRequestContext + ?Sized,
{
    if declared_types.unit() != request.view().unit()
        || declared_types.kind() != request.view().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    }

    let declared = match prepare_declared_types(request, declared_types) {
        Ok(SessionProgress::Complete(declared)) => declared,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let PreparedDeclaredTypes {
        input,
        deferred,
        unsupported_callable_result,
    } = declared;

    let mut prepared = match prepare_calls(request, candidate_sets) {
        Ok(prepared) => prepared,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    prepared.defer(deferred);

    let mut session = match ExpressionTypeSession::begin(request) {
        Ok(SessionProgress::Complete(session)) => session,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    if unsupported_callable_result {
        defer_return_operands(request, session.expressions(), prepared.deferred_mut());
    }

    if let Err(error) = session.apply_input(&input) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    match converge(request, &prepared, &mut session) {
        Ok(SessionProgress::Complete(())) => {}
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    match session.apply_literal_defaults() {
        SessionProgress::Complete(()) => {}
        SessionProgress::Cancelled => return CheckerOutcome::Cancelled,
    }

    match converge(request, &prepared, &mut session) {
        Ok(SessionProgress::Complete(())) => {}
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    let type_result =
        match finish_expression_types_with_deferred(request, session, prepared.deferred()) {
            CheckerOutcome::Complete(result) => result,
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
        };

    let (types, type_diagnostics) = type_result.into_parts();

    let (entries, selection_diagnostics) = match final_selections(request, &types, &prepared) {
        Ok(Some(result)) => result,
        Ok(None) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let selections = match CheckedSemanticSelections::try_new(request.unit(), &types, entries) {
        Ok(selections) => selections,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            );
        }
    };

    CheckerOutcome::complete(
        (types, selections),
        type_diagnostics.merged(&selection_diagnostics),
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundExpressionId, BoundUnit, BoundUnitId, DeclaredValueTypeTemplates};

    use super::check_expression_semantics;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression,
    };
    use crate::{CheckerInfrastructureError, CheckerUnitView, ExpressionCandidateSet};

    #[test]
    fn foreign_candidate_expressions_fail_without_partial_results() {
        let (unit, _) = literal_unit(BoundUnitId::new(91));

        let (_, foreign) = literal_unit(BoundUnitId::new(92));

        let declared =
            DeclaredValueTypeTemplates::new(unit.unit(), unit.key().kind(), [], [], None);

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = match CheckerUnitView::new(&unit, &semantic_context, &context) {
            Ok(request) => request,
            Err(error) => panic!("test checker unit view must be valid: {error:?}"),
        };

        let outcome = check_expression_semantics(
            request,
            &declared,
            &[ExpressionCandidateSet::NotApplicable(foreign)],
        );

        assert_eq!(
            outcome.infrastructure_failure(),
            Some(CheckerInfrastructureError::InvalidSemanticSelectionInput)
        );

        assert_eq!(outcome.result(), None);
    }

    fn literal_unit(unit: BoundUnitId) -> (BoundUnit, BoundExpressionId) {
        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            vec![push_expression(
                tree,
                integer_literal_expression(origin, None),
            )]
        });

        let [expression] = expressions.as_slice() else {
            panic!("literal unit must contain one expression");
        };

        (unit, *expression)
    }
}
