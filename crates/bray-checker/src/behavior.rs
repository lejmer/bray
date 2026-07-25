use bray_bound_tree::{
    BodyBehaviorCall, BodyBehaviorContributions, BodyBehaviorPhase, BoundCallResult,
    BoundCallableTarget, BoundExpression, CheckedControlFlowFacts, CheckedSemanticSelections,
    ConstructionDefaultProvider, ConstructionTarget, ConversionTarget, IndexTarget, OperatorTarget,
    SelectedArgument, SelectedConstructionInput, SelectedConversion, SelectedOperation,
    SemanticSelection,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::CurrentRunCancellation;

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

pub(crate) fn collect_body_behavior<C>(
    request: CheckerUnitView<'_, C>,
    control_flow: &CheckedControlFlowFacts,
    selections: &CheckedSemanticSelections,
) -> CheckerOutcome<BodyBehaviorContributions>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if control_flow.unit() != request.unit().unit()
        || control_flow.kind() != request.unit().key().kind()
        || selections.unit() != request.unit().unit()
        || selections.kind() != request.unit().key().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    }

    let mut calls = Vec::new();
    let mut defaults = Vec::new();

    for entry in selections.entries() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        match entry.selection() {
            SemanticSelection::Call(call) => {
                let mut contribution =
                    BodyBehaviorCall::new(call.target(), BodyBehaviorPhase::Invocation);
                let async_anonymous = matches!(
                    (call.target(), call.resolution().result()),
                    (
                        BoundCallableTarget::Anonymous(_),
                        BoundCallResult::LazyFuture(_)
                    )
                );

                if matches!(call.target(), BoundCallableTarget::Anonymous(_))
                    && !async_anonymous
                    && let Some(unit) = anonymous_callable_unit(request, entry.expression())
                {
                    contribution = contribution.with_anonymous_unit(unit);
                }

                if !async_anonymous {
                    calls.push(contribution);
                }

                defaults.extend(
                    call.arguments()
                        .iter()
                        .filter_map(|argument| match argument {
                            SelectedArgument::Default { provider, .. } => {
                                Some(ConstructionDefaultProvider::CallableParameter(*provider))
                            }
                            SelectedArgument::Explicit { .. } => None,
                        }),
                );
            }
            SemanticSelection::Operation(operation) => {
                collect_operation_behavior(operation, &mut calls, &mut defaults);
            }
            SemanticSelection::Reference(_) => {}
        }
    }

    for (_, expression) in request.unit().tree().expressions() {
        let BoundExpression::Await(await_expression) = expression else {
            continue;
        };

        // TODO(BRA-266): Consume deferred behavior carried by arbitrary Future values.
        let Some(SemanticSelection::Call(call)) = selections.expression(await_expression.operand())
        else {
            continue;
        };

        if matches!(call.resolution().result(), BoundCallResult::LazyFuture(_)) {
            let mut contribution =
                BodyBehaviorCall::new(call.target(), BodyBehaviorPhase::DeferredExecution);

            if matches!(call.target(), BoundCallableTarget::Anonymous(_))
                && let Some(unit) = anonymous_callable_unit(request, await_expression.operand())
            {
                contribution = contribution.with_anonymous_unit(unit);
            }

            calls.push(contribution);
        }
    }

    let current_run_cancellation = if control_flow
        .completion()
        .contains(bray_bound_tree::ControlCompletionKind::Cancellation)
    {
        CurrentRunCancellation::MayEnter
    } else {
        CurrentRunCancellation::NotEntered
    };

    let is_recovered = control_flow.is_recovered()
        || request
            .unit()
            .tree()
            .expressions()
            .any(|(_, expression)| expression.is_recovered());

    CheckerOutcome::Complete(DiagnosticResult::new(
        BodyBehaviorContributions::new(
            request.unit().unit(),
            request.unit().key().kind(),
            calls,
            defaults,
            current_run_cancellation,
            is_recovered,
        ),
        DiagnosticBag::new(),
    ))
}

fn anonymous_callable_unit<C>(
    request: CheckerUnitView<'_, C>,
    call: bray_bound_tree::BoundExpressionId,
) -> Option<bray_bound_tree::BoundUnitKey>
where
    C: CheckerRequestContext + ?Sized,
{
    let BoundExpression::Call(call) = request.unit().view().expression(call)? else {
        return None;
    };

    let BoundExpression::AnonymousCallable(callable) =
        request.unit().view().expression(call.callee())?
    else {
        return None;
    };

    Some(callable.unit().clone())
}

fn collect_operation_behavior(
    operation: &SelectedOperation,
    calls: &mut Vec<BodyBehaviorCall>,
    defaults: &mut Vec<ConstructionDefaultProvider>,
) {
    match operation {
        SelectedOperation::Operator {
            target: OperatorTarget::Trait { fulfillment, .. },
            ..
        }
        | SelectedOperation::Index {
            target: IndexTarget::Custom { fulfillment, .. },
            ..
        } => calls.push(invocation(*fulfillment)),
        SelectedOperation::Construction(construction) => {
            if let ConstructionTarget::TypeForm(callable) = construction.target() {
                calls.push(invocation(callable));
            }

            defaults.extend(
                construction
                    .inputs()
                    .iter()
                    .filter_map(|input| match input {
                        SelectedConstructionInput::Default { provider, .. } => Some(*provider),
                        SelectedConstructionInput::Explicit { .. } => None,
                    }),
            );
        }
        SelectedOperation::Conversion(conversion) => {
            collect_conversion_behavior(conversion, calls);
        }
        SelectedOperation::Member(_)
        | SelectedOperation::Operator { .. }
        | SelectedOperation::Index { .. }
        | SelectedOperation::Implementation(_) => {}
    }
}

fn collect_conversion_behavior(conversion: &SelectedConversion, calls: &mut Vec<BodyBehaviorCall>) {
    match conversion.target() {
        ConversionTarget::Trait { fulfillment, .. } => {
            calls.push(invocation(*fulfillment));
        }
        ConversionTarget::Composite(conversions) => {
            for conversion in conversions.iter() {
                collect_conversion_behavior(conversion, calls);
            }
        }
        ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
    }
}

fn invocation(callable: bray_symbols::CallableInstanceData) -> BodyBehaviorCall {
    BodyBehaviorCall::new(
        BoundCallableTarget::Declaration(callable),
        BodyBehaviorPhase::Invocation,
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundAwaitExpression, BoundCallExpression, BoundCallResult, BoundCallableTarget,
        BoundExpression, BoundResolvedCall, BoundUnitId, CheckedControlFlowFacts,
        CheckedSemanticSelections, ExpressionTypeResult, ExpressionTypeStatus, SemanticSelection,
        SemanticSelectionEntry,
    };
    use bray_symbols::{CallableAbi, CurrentRunCancellation, TypeData};

    use crate::service::BodyBehaviorCollector;
    use crate::test_support::{
        TestCheckerContext, callable_entry, checked_expression_types, error_type, expression_unit,
        push_expression, semantic_values,
    };
    use crate::{CheckerUnitView, DefaultBodyBehaviorCollector};

    #[test]
    fn direct_awaits_retain_invocation_and_deferred_callable_phases() {
        let unit_id = BoundUnitId::new(50);

        let (unit, expressions) = expression_unit(unit_id, |tree, origin| {
            let callee = push_expression(
                tree,
                BoundExpression::Error(bray_bound_tree::BoundErrorExpression::new(
                    origin,
                    error_type(),
                )),
            );

            let call = push_expression(
                tree,
                BoundExpression::Call(BoundCallExpression::pending(origin, callee, [], [])),
            );

            let await_expression = push_expression(
                tree,
                BoundExpression::Await(BoundAwaitExpression::pending(origin, call, false)),
            );

            vec![callee, call, await_expression]
        });

        let callable_type = semantic_values()
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test callable type must intern: {error:?}"));

        let resolution = BoundResolvedCall::new(
            BoundCallableTarget::Indirect(callable_type),
            [],
            BoundCallResult::LazyFuture(bray_bound_tree::BoundFutureConstruction::new(
                error_type(),
                error_type(),
            )),
        );

        let call = bray_bound_tree::SelectedCall::new(resolution, CallableAbi::Bray, [], []);

        let result = ExpressionTypeResult::new(error_type(), ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        let selections = CheckedSemanticSelections::try_new(
            &unit,
            &types,
            [SemanticSelectionEntry::new(
                expressions[1],
                SemanticSelection::Call(call),
            )],
        )
        .unwrap_or_else(|error| panic!("test selections must validate: {error:?}"));

        let control_flow = CheckedControlFlowFacts::new(
            unit_id,
            unit.key().kind(),
            bray_bound_tree::ControlCompletion::from_kinds([
                bray_bound_tree::ControlCompletionKind::Cancellation,
            ]),
        );

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let result =
            DefaultBodyBehaviorCollector.collect_body_behavior(request, &control_flow, &selections);

        let crate::CheckerOutcome::Complete(result) = result else {
            panic!("behavior collection must complete");
        };

        assert_eq!(result.value().calls().len(), 2);
        assert_eq!(
            result.value().current_run_cancellation(),
            CurrentRunCancellation::MayEnter
        );
    }
}
