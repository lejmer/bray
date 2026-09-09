use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use bray_bound_tree::{AnyBoundNodeId, BoundAssignmentOperator, BoundExpression};
use bray_symbols::{ConstantTermData, ConstantTermId, SemanticValueStoreError};

use crate::constant::shape::{observation_subject, storage_observation_subject};
use crate::contract::{MAX_CONDITION_STEPS, callable_input_count, rewrite_condition_with_budget};
use crate::{CheckerRequestContext, CheckerUnitView, ExecutionGuaranteeInput};

use super::super::id::AnalysisOperationId;
use super::super::model::{AnalysisOperationKind, ControlFlowGraph};
use super::flow::{DomainState, GuaranteeDomain};

pub(super) fn assignment_observations<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    input: &ExecutionGuaranteeInput,
    graph: &ControlFlowGraph,
) -> Result<
    BTreeMap<AnalysisOperationId, (ConstantTermId, bray_bound_tree::BoundExpressionId)>,
    SemanticValueStoreError,
> {
    let values = request.semantic_values();
    let mut assignments = BTreeMap::new();

    for operation in graph
        .blocks()
        .iter()
        .flat_map(|block| block.operations())
        .filter_map(|operation| graph.operation(*operation))
    {
        let AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) = operation.kind()
        else {
            continue;
        };

        let Some(BoundExpression::Assignment(assignment)) = request.view().expression(expression)
        else {
            continue;
        };

        let [target, value] = assignment.operands() else {
            continue;
        };

        if assignment.operator() != BoundAssignmentOperator::Assign || assignment.is_recovered() {
            continue;
        }

        let Some(target) = input.expression(*target) else {
            continue;
        };

        // Scalars and immediate call results have distinct evaluation-time observations.
        // Other assignments retain only closed values without mutable storage observations.
        if !input.is_scalar_observation(*value)
            && input.call(*value).is_none()
            && input
                .expression(*value)
                .map(|term| callable_input_count(values, [term]))
                .transpose()?
                .flatten()
                != Some(0)
        {
            continue;
        }

        let mut remaining = MAX_CONDITION_STEPS;

        if let Some(target) = storage_observation_subject(values, target, &mut remaining)? {
            assignments.insert(operation.id(), (target, *value));
        }
    }

    Ok(assignments)
}

pub(super) fn observation_accesses(
    values: &bray_symbols::SemanticValueStore,
    input: &ExecutionGuaranteeInput,
    storage: &bray_bound_tree::StoragePlan,
) -> Result<
    BTreeMap<ConstantTermId, BTreeSet<bray_bound_tree::StorageAccessId>>,
    SemanticValueStoreError,
> {
    let mut accesses = BTreeMap::<_, BTreeSet<_>>::new();

    for (identity, _) in storage.identity_entries() {
        if let Some(term) = input.storage_observation(identity)
            && let Some(access) = storage.root_access(identity)
        {
            accesses.entry(term).or_default().insert(access);
        }
    }

    for plan in storage.access_plans().iter().filter(|plan| {
        matches!(
            plan.point(),
            bray_bound_tree::BoundOperationPoint::Evaluation(_)
        )
    }) {
        let mut remaining = MAX_CONDITION_STEPS;

        if let Some(term) = input.expression(plan.expression())
            && let Some(subject) = storage_observation_subject(values, term, &mut remaining)?
        {
            accesses.entry(subject).or_default().insert(plan.access());
        }
    }

    Ok(accesses)
}

pub(super) fn local_initializers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    input: &ExecutionGuaranteeInput,
) -> BTreeMap<bray_bound_tree::BoundPatternId, (ConstantTermId, bray_bound_tree::BoundExpressionId)>
{
    let mut initializers = BTreeMap::new();

    for (_, block) in request.unit().tree().blocks() {
        for item in block.items() {
            let bray_bound_tree::BoundBlockItem::LocalBinding(binding) = item else {
                continue;
            };

            let Some(pattern) = request.view().pattern(binding.pattern()) else {
                continue;
            };

            let [symbol] = pattern.bindings() else {
                continue;
            };

            if pattern.kind() == bray_bound_tree::BoundPatternKind::Binding
                && !pattern.is_recovered()
                && let Some(term) = input.local_observation(*symbol)
            {
                initializers.insert(binding.pattern(), (term, binding.initializer()));
            }
        }
    }

    initializers
}

impl GuaranteeDomain<'_> {
    pub(super) fn transfer_value_observations(
        &self,
        state: &mut DomainState,
        initializer: bray_bound_tree::BoundExpressionId,
        target: ConstantTermId,
    ) -> Result<(), SemanticValueStoreError> {
        let Some(source) = self.input.expression(initializer) else {
            return Ok(());
        };

        let mut remaining = MAX_CONDITION_STEPS;

        let Some(source) = observation_subject(self.values, source, &mut remaining)? else {
            return Ok(());
        };

        let mut transferred = Vec::new();

        for (subject, value) in &state.observations {
            let mut copied = false;
            let mut remaining = MAX_CONDITION_STEPS;

            let subject =
                rewrite_condition_with_budget(self.values, *subject, &mut remaining, |term, _| {
                    if term == source {
                        copied = true;

                        Ok(ControlFlow::Break(Some(target)))
                    } else {
                        Ok(ControlFlow::Continue(()))
                    }
                })?;

            if let Some(subject) = subject
                && copied
            {
                transferred.push((subject, *value));
            }
        }

        state.observations.extend(transferred);

        Ok(())
    }

    pub(super) fn current_expression(
        &self,
        state: &DomainState,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        let mut remaining = MAX_CONDITION_STEPS;

        self.current_expression_with_budget(state, expression, &mut remaining)
    }

    fn current_expression_with_budget(
        &self,
        state: &DomainState,
        expression: bray_bound_tree::BoundExpressionId,
        remaining: &mut usize,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        match state.evaluated_values.get(&expression).copied() {
            Some(value) => Ok(Some(value)),
            None => {
                match self.view.expression(expression) {
                    Some(BoundExpression::Structured(structured))
                        if structured.kind()
                            == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
                            && !structured.is_recovered() =>
                    {
                        if let [operand] = structured.operands() {
                            return self.current_expression_with_budget(state, *operand, remaining);
                        }
                    }
                    Some(BoundExpression::Structured(structured))
                        if matches!(
                            structured.kind(),
                            bray_bound_tree::BoundStructuredExpressionKind::PatternTest
                                | bray_bound_tree::BoundStructuredExpressionKind::PatternBinding
                        ) && !structured.is_recovered() =>
                    {
                        if let ([operand], [pattern]) =
                            (structured.operands(), structured.patterns())
                            && let Some(subject) =
                                self.current_expression_with_budget(state, *operand, remaining)?
                        {
                            return self.pattern_condition(*pattern, subject, remaining);
                        }
                    }
                    Some(BoundExpression::Unary(unary))
                        if matches!(
                            self.selections.expression(expression),
                            Some(bray_bound_tree::SemanticSelection::Operation(
                                bray_bound_tree::SelectedOperation::Operator {
                                    target: bray_bound_tree::OperatorTarget::BuiltIn(
                                        bray_bound_tree::BoundOperator::LogicalNot
                                    ),
                                    ..
                                }
                            ))
                        ) =>
                    {
                        if let [operand] = unary.operands()
                            && let Some(operand) =
                                self.current_expression_with_budget(state, *operand, remaining)?
                        {
                            return self
                                .values
                                .intern_constant_term(ConstantTermData::Unary {
                                    operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                                    operand,
                                })
                                .map(Some);
                        }
                    }
                    _ => {}
                }

                if let Some(bray_bound_tree::SemanticSelection::Operation(
                    bray_bound_tree::SelectedOperation::Construction(construction),
                )) = self.selections.expression(expression)
                    && let Some(value) =
                        self.current_construction(state, construction, remaining)?
                {
                    return Ok(Some(value));
                }

                match self.input.expression(expression) {
                    Some(term) => self.current_observation(state, term, None, None),
                    None => Ok(None),
                }
            }
        }
    }

    fn current_construction(
        &self,
        state: &DomainState,
        construction: &bray_bound_tree::SelectedConstruction,
        remaining: &mut usize,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        if matches!(
            construction.target(),
            bray_bound_tree::ConstructionTarget::TypeForm { .. }
        ) || construction.inputs().len() > *remaining
        {
            return Ok(None);
        }

        let mut fields = Vec::with_capacity(construction.inputs().len());

        for input in construction.inputs() {
            let bray_bound_tree::SelectedConstructionInput::Explicit {
                expression, input, ..
            } = input
            else {
                return Ok(None);
            };

            let Some(value) = self.current_expression_with_budget(state, *expression, remaining)?
            else {
                return Ok(None);
            };

            fields.push((*input, value));
        }

        match crate::constant::shape::constructed_term(construction.target(), fields) {
            Some(term) => self.values.intern_constant_term(term).map(Some),
            None => Ok(None),
        }
    }

    pub(super) fn current_observation(
        &self,
        state: &DomainState,
        condition: ConstantTermId,
        returned: Option<ConstantTermId>,
        preserved: Option<ConstantTermId>,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        let mut remaining = MAX_CONDITION_STEPS;

        rewrite_condition_with_budget(self.values, condition, &mut remaining, |term, data| {
            if preserved == Some(term) {
                return Ok(ControlFlow::Break(Some(term)));
            }

            if let Some(value) = state.observations.get(&term) {
                return Ok(ControlFlow::Break(Some(*value)));
            }

            if matches!(data, ConstantTermData::Projection(_)) && !state.observations.is_empty() {
                let mut remaining = MAX_CONDITION_STEPS;

                if let Some(subject) = observation_subject(self.values, term, &mut remaining)?
                    && let Some(value) = state.observations.get(&subject)
                {
                    return Ok(ControlFlow::Break(Some(*value)));
                }
            }

            Ok(match data {
                ConstantTermData::CallableArgument(ordinal)
                    if *ordinal == self.input.result_ordinal() =>
                {
                    ControlFlow::Break(returned)
                }
                ConstantTermData::CallableArgument(ordinal) if !state.entry_observations_valid => {
                    let unknown = state
                        .invalidation
                        .and_then(|operation| self.input.unknown_observation(operation, *ordinal))
                        .map(|ordinal| {
                            self.values
                                .intern_constant_term(ConstantTermData::CallableArgument(ordinal))
                        })
                        .transpose()?;

                    ControlFlow::Break(unknown)
                }
                _ => ControlFlow::Continue(()),
            })
        })
    }
}
