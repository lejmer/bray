use super::super::super::model::{AnalysisEdge, AnalysisRefinement};
use super::super::state::ExecutionState;
use super::model::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, Refinement, RefinementKind, SelectedPropagation,
    SelectedPropagationBoundary, SemanticSelection, StorageProjection,
};
use std::sync::Arc;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn assume_patterns<'a>(
        &self,
        state: &mut ExecutionState,
        refinements: impl Iterator<Item = &'a Refinement>,
    ) -> bool {
        let mut changed = false;

        for refinement in refinements {
            let RefinementKind::Pattern {
                subject,
                predicate,
                access,
                value,
                ..
            } = refinement.kind()
            else {
                continue;
            };

            let ty = self
                .storage
                .access(access)
                .expect("checked pattern retains its subject")
                .reached_type();

            let Some((operator, expected)) = crate::execution_guarantees::pattern_value(
                self.request.semantic_values(),
                ty,
                predicate,
            ) else {
                continue;
            };

            let condition = ExecutionCondition::pattern(
                self.storage_value(state, subject, access),
                operator,
                expected,
            );

            if condition
                .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                .is_some_and(|known| known != value)
            {
                return false;
            }

            condition.assume(value, &mut state.assumptions);

            changed = true;
        }

        if changed {
            state.refine_witnesses();
        }

        true
    }
    pub(super) fn branch_condition(
        &self,
        state: &ExecutionState,
        edge: &AnalysisEdge,
    ) -> Option<(ExecutionCondition, bool)> {
        match edge.refinement()? {
            AnalysisRefinement::InvocationPresence(invocation, present) => {
                let access = self.storage.invocation_result(invocation)
                    .expect("protocol presence has its actual result");
                let ty = self.storage.access(access)
                    .expect("protocol result retains its type").reached_type();
                let (_, absent) = crate::execution_guarantees::pattern_value(
                    self.request.semantic_values(), ty, bray_bound_tree::PatternPredicate::NullableAbsent)
                    .expect("next result is nullable");
                Some((ExecutionCondition::pattern(self.invocation_value(state, invocation),
                    bray_bound_tree::BoundOperator::Equal, absent), !present))
            }
            AnalysisRefinement::InvocationBoolean(invocation, value) => Some((
                self.invocation_value(state, invocation)
                    .component(bray_symbols::ConstantProjectionKind::NullableValue), value)),
            AnalysisRefinement::PatternOutcome {
                subject,
                pattern,
                value,
            } => {
                let (operator, expected) = self.pattern_conditions.get(&pattern)?;

                Some((
                    ExecutionCondition::pattern(
                        self.value(state, subject),
                        *operator,
                        expected.clone(),
                    ),
                    value,
                ))
            }
            AnalysisRefinement::PropagationOutcome {
                expression,
                succeeded,
            } => {
                let bray_bound_tree::SemanticSelection::Propagation(selection) = self
                    .semantics
                    .selections()
                    .expression(expression)
                    .expect("checked propagation retains its selection")
                else {
                    unreachable!("propagation edge requires a propagation selection")
                };

                let Some(BoundExpression::Structured(operation)) =
                    self.request.view().expression(expression)
                else {
                    unreachable!("propagation edge requires a structured expression")
                };

                let subject = *operation
                    .operands()
                    .first()
                    .expect("propagation retains its operand");

                match selection.success() {
                    Some((variant, _)) => Some((
                        self.value(state, subject).matches_variant(variant),
                        succeeded,
                    )),
                    None => {
                        let (_, absent) = crate::execution_guarantees::pattern_value(
                            self.request.semantic_values(),
                            self.semantics
                                .types()
                                .expression(subject)
                                .expect("nullable propagation retains its operand type")
                                .ty(),
                            bray_bound_tree::PatternPredicate::NullableAbsent,
                        )
                        .expect("nullable propagation has an absence value");

                        Some((
                            ExecutionCondition::pattern(
                                self.value(state, subject),
                                bray_bound_tree::BoundOperator::Equal,
                                absent,
                            ),
                            !succeeded,
                        ))
                    }
                }
            }
            _ => None,
        }
    }
    pub(super) fn propagate_result(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        let Some(SemanticSelection::Propagation(selection)) =
            self.semantics.selections().expression(expression)
        else {
            unreachable!("propagation edge retains its checked selection");
        };

        let value = match selection {
            SelectedPropagation::CurrentRun { .. } => return,
            SelectedPropagation::Nullable { result_type, .. } => {
                crate::execution_guarantees::pattern_value(
                    self.request.semantic_values(),
                    *result_type,
                    bray_bound_tree::PatternPredicate::NullableAbsent,
                )
                .expect("nullable propagation has an absence value")
                .1
            }
            SelectedPropagation::Result {
                error_conversion,
                error: (variant, field),
                ..
            } => {
                let Some(BoundExpression::Structured(operation)) =
                    self.request.view().expression(expression)
                else {
                    unreachable!("propagation retains its structured expression");
                };

                let subject = *operation
                    .operands()
                    .first()
                    .expect("propagation retains its operand");

                let projection = StorageProjection::ActiveUnionPayloadField {
                    variant: *variant,
                    field: *field,
                };

                let payload = match error_conversion.target() {
                    bray_bound_tree::ConversionTarget::Identity
                    | bray_bound_tree::ConversionTarget::CallableContract => {
                        self.value(state, subject).project(projection)
                    }
                    bray_bound_tree::ConversionTarget::NullablePresent => {
                        self.value(state, subject).project(projection).present()
                    }
                    // A value-changing conversion owns a new result. Its selected tag is known,
                    // but the source payload's properties do not automatically hold afterward.
                    _ => ExecutionCondition::Expression(expression).project(projection),
                };

                ExecutionCondition::Constructed(
                    Arc::new(ExecutionCondition::Tagged(
                        *variant,
                        Arc::new(ExecutionCondition::Expression(expression)),
                    )),
                    vec![(projection, payload)].into(),
                )
            }
        };

        match selection
            .boundary()
            .expect("ordinary propagation retains its result boundary")
        {
            SelectedPropagationBoundary::Callable => state.result = value,
            SelectedPropagationBoundary::YieldRegion(_) => {
                let target = self
                    .yield_targets
                    .get(&expression)
                    .expect("propagation retains its selected yield target");

                state.pending_results.insert(*target, value);
            }
        }
    }
}
