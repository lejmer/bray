use bray_bound_tree::{
    BodyBehaviorCall, BodyBehaviorContributions, BodyBehaviorPhase, BoundCallResult,
    BoundCallableTarget, CheckedAsync, CheckedControlFlow, CheckedSemanticSelections,
    ConstructionDefaultProvider, ConstructionTarget, ConversionTarget, IndexTarget, OperatorTarget,
    SelectedArgument, SelectedConstructionInput, SelectedConversion, SelectedOperation,
    SemanticSelection,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::CurrentRunCancellation;

use crate::unit::semantic_input_failure;
use crate::{CheckerInputKind, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

pub(crate) fn collect_body_behavior<C>(
    request: CheckerUnitView<'_, C>,
    control_flow: &CheckedControlFlow,
    selections: &CheckedSemanticSelections,
    async_analysis: &CheckedAsync,
) -> CheckerOutcome<BodyBehaviorContributions>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if let Some(error) = semantic_input_failure(
        request,
        [
            (
                CheckerInputKind::ControlFlow,
                (control_flow.unit(), control_flow.kind()),
            ),
            (
                CheckerInputKind::SemanticSelections,
                (selections.unit(), selections.kind()),
            ),
            (
                CheckerInputKind::AsyncAnalysis,
                (async_analysis.unit(), async_analysis.kind()),
            ),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let mut calls = Vec::new();
    let mut defaults = Vec::new();
    let mut enters_current_run_cancellation = false;

    for entry in selections.entries() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let source = request
            .view()
            .expression(entry.expression())
            .map(|expression| expression.origin().source_anchor());

        match entry.selection() {
            SemanticSelection::Call(call) => {
                enters_current_run_cancellation |= matches!(
                    call.implementation_hook(),
                    Some(ImplementationHook::CurrentRunCancellationPropagation)
                );

                let mut contribution =
                    BodyBehaviorCall::new(call.target(), BodyBehaviorPhase::Invocation);

                if let Some(source) = source {
                    contribution = contribution.with_source(source);
                }

                let async_anonymous = matches!(
                    (call.target(), call.resolution().result()),
                    (
                        BoundCallableTarget::Anonymous(_),
                        BoundCallResult::LazyFuture(_)
                    )
                );

                if matches!(call.target(), BoundCallableTarget::Anonymous(_))
                    && !async_anonymous
                    && let Some(unit) = request.anonymous_callable_unit(entry.expression())
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
                collect_operation_behavior(operation, source, &mut calls, &mut defaults);
            }
            SemanticSelection::Iteration(selection) => {
                calls.extend([selection.iterate(), selection.next()].map(|target| {
                    let contribution = BodyBehaviorCall::new(
                        BoundCallableTarget::Declaration(target),
                        BodyBehaviorPhase::Invocation,
                    );

                    match source {
                        Some(source) => contribution.with_source(source),
                        None => contribution,
                    }
                }));
            }
            SemanticSelection::Reference(_)
            | SemanticSelection::CallableReference(_)
            | SemanticSelection::StaticReference(_)
            | SemanticSelection::Predicate(_)
            | SemanticSelection::Propagation(_) => {}
        }
    }

    for suspension in async_analysis.suspensions() {
        calls.extend(suspension.deferred_calls().iter().cloned());
    }

    let current_run_cancellation = if enters_current_run_cancellation
        || control_flow
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

/// Collects invocation dependencies and reports whether their witnesses are already resolved.
pub(crate) fn collect_operation_behavior(
    operation: &SelectedOperation,
    source: Option<bray_bound_tree::BoundSourceAnchor>,
    calls: &mut Vec<BodyBehaviorCall>,
    defaults: &mut Vec<ConstructionDefaultProvider>,
) -> bool {
    if let Some(target) = operation.operator_target() {
        match target {
            OperatorTarget::Trait { fulfillment, .. } => {
                calls.push(invocation(fulfillment, source));
            }
            OperatorTarget::TraitConstraint { member, .. } => {
                calls.push(invocation(member, source));
            }
            OperatorTarget::BuiltIn(_) => {}
        }

        return !matches!(target, OperatorTarget::TraitConstraint { .. });
    }

    match operation {
        SelectedOperation::Index {
            target: IndexTarget::Custom { fulfillment, .. },
            ..
        } => calls.push(invocation(*fulfillment, source)),
        SelectedOperation::Index {
            target: IndexTarget::TraitConstraint { member, .. },
            ..
        } => {
            calls.push(invocation(*member, source));

            return false;
        }
        SelectedOperation::Construction(construction) => {
            if let ConstructionTarget::TypeForm { callable, .. } = construction.target() {
                calls.push(invocation(callable, source));
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
            return collect_conversion_behavior(conversion, source, calls);
        }
        SelectedOperation::Member(_)
        | SelectedOperation::Operator { .. }
        | SelectedOperation::CompoundAssignment(_)
        | SelectedOperation::Index { .. }
        | SelectedOperation::Implementation(_) => {}
    }

    true
}

/// Collects conversion invocations and reports whether their witnesses are already resolved.
pub(crate) fn collect_conversion_behavior(
    conversion: &SelectedConversion,
    source: Option<bray_bound_tree::BoundSourceAnchor>,
    calls: &mut Vec<BodyBehaviorCall>,
) -> bool {
    let mut resolved = true;
    let mut pending = vec![conversion];

    while let Some(conversion) = pending.pop() {
        match conversion.target() {
            ConversionTarget::Trait { fulfillment, .. } => {
                calls.push(invocation(*fulfillment, source))
            }
            ConversionTarget::TraitConstraint { member, .. } => {
                calls.push(invocation(*member, source));
                resolved = false;
            }
            ConversionTarget::Composite(conversions) => pending.extend(conversions.iter().rev()),
            ConversionTarget::Identity
            | ConversionTarget::NullablePresent
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion => {}
        }
    }

    resolved
}

fn invocation(
    callable: bray_symbols::CallableInstanceData,
    source: Option<bray_bound_tree::BoundSourceAnchor>,
) -> BodyBehaviorCall {
    let contribution = BodyBehaviorCall::new(
        BoundCallableTarget::Declaration(callable),
        BodyBehaviorPhase::Invocation,
    );

    match source {
        Some(source) => contribution.with_source(source),
        None => contribution,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        AsyncSuspensionPoint, BodyBehaviorCall, BodyBehaviorPhase, BoundAwaitExpression,
        BoundCallExpression, BoundCallResult, BoundCallableTarget, BoundExpression,
        BoundResolvedCall, BoundUnitId, CheckedAsync, CheckedControlFlow,
        CheckedSemanticSelections, ExpressionTypeResult, ExpressionTypeStatus, SemanticSelection,
        SemanticSelectionEntry,
    };
    use bray_symbols::{CallableAbi, CurrentRunCancellation, TypeData};

    use crate::service::BodyBehaviorCollector;
    use crate::test_support::{
        TestCheckerContext, callable_entry, checked_expression_types,
        empty_callable_phase_behaviors, error_type, expression_unit, push_expression,
        semantic_values,
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

        let call = bray_bound_tree::SelectedCall::new(
            resolution,
            CallableAbi::Bray,
            empty_callable_phase_behaviors(),
            None,
            [],
            [],
        );

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

        let control_flow = CheckedControlFlow::new(
            unit_id,
            unit.key().kind(),
            bray_bound_tree::ControlCompletion::from_kinds([
                bray_bound_tree::ControlCompletionKind::Cancellation,
            ]),
        );

        let async_analysis = CheckedAsync::try_new(
            unit_id,
            unit.key().kind(),
            [],
            [AsyncSuspensionPoint::new(
                expressions[2],
                bray_bound_tree::AsyncSuspensionKind::Await {
                    operand: expressions[1],
                },
                None,
                [BodyBehaviorCall::new(
                    BoundCallableTarget::Indirect(callable_type),
                    BodyBehaviorPhase::DeferredExecution,
                )],
                [],
                false,
            )],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test async analysis must validate: {error:?}"));

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let result = DefaultBodyBehaviorCollector.collect_body_behavior(
            request,
            &control_flow,
            &selections,
            &async_analysis,
        );

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
