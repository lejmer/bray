use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpressionId, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, ExpressionTypeResult, walk_bound_unit_view,
};
use bray_symbols::TypeId;

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::canonical::CanonicalTypes;
use super::constraints::{
    add_expectations, add_intrinsic_constraints, add_relationship_constraints, block_expectations,
};
use super::inference::{InferenceTypeId, TypeConflict, TypeInferenceContext};
use super::propagation::propagate_dynamic_constraints;
use super::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};

pub(super) enum SessionProgress<T> {
    Complete(T),
    Cancelled,
}

pub(super) struct FinishedExpressionTypes {
    pub(super) results: Vec<(BoundExpressionId, ExpressionTypeResult)>,
    pub(super) conflicts: Vec<TypeConflict>,
    pub(super) unresolved: Vec<BoundExpressionId>,
}

/// Mutable checker-local state shared with operation selection before publication.
pub(crate) struct ExpressionTypeSession<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: UnitCheckRequest<'view, C>,
    expressions: Vec<BoundExpressionId>,
    variables: BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: BTreeMap<BoundBlockId, InferenceTypeId>,
    block_owners: BTreeMap<BoundBlockId, BoundExpressionId>,
    types: CanonicalTypes,
    inference: TypeInferenceContext,
}

impl<'view, C> ExpressionTypeSession<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(crate) fn begin(
        request: UnitCheckRequest<'view, C>,
    ) -> Result<SessionProgress<Self>, CheckerInfrastructureError> {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(nodes) = collect_nodes(request)? else {
            return Ok(SessionProgress::Cancelled);
        };

        let types = CanonicalTypes::new(request)?;
        let mut inference = TypeInferenceContext::new(types.error, types.never);
        let mut variables = BTreeMap::new();
        let mut block_variables = BTreeMap::new();

        initialize_expression_variables(
            request,
            &nodes.expressions,
            &types,
            &mut variables,
            &mut inference,
        )?;

        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        initialize_block_variables(request, &nodes.blocks, &mut block_variables, &mut inference)?;

        let block_owners = collect_block_owners(request, &nodes.expressions)?;

        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        if !add_relationship_constraints(
            request,
            &nodes.expressions,
            &variables,
            &block_variables,
            &types,
            &mut inference,
        ) {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(local_expectations) = block_expectations(request, &nodes.blocks) else {
            return Ok(SessionProgress::Cancelled);
        };

        if add_expectations(request, local_expectations, &variables, &mut inference)?.is_none() {
            return Ok(SessionProgress::Cancelled);
        }

        Ok(SessionProgress::Complete(Self {
            request,
            expressions: nodes.expressions,
            variables,
            block_variables,
            block_owners,
            types,
            inference,
        }))
    }

    pub(crate) fn apply_input(
        &mut self,
        input: &ExpressionTypeInput,
    ) -> Result<(), CheckerInfrastructureError> {
        validate_input(self.request, input)?;

        for evidence in input.evidence() {
            self.apply_evidence(*evidence)?;
        }

        for expectation in input.expectations() {
            self.add_expectation(expectation.expression(), expectation.ty())?;
        }

        if let Some(result_type) = input.callable_result_type() {
            self.add_return_expectations(result_type)?;
        }

        Ok(())
    }

    pub(crate) fn add_evidence(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        self.validate_expression_and_type(expression, ty)?;

        let Some(variable) = self.variables.get(&expression).copied() else {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        };

        self.inference.add_evidence(variable, ty, expression);

        Ok(())
    }

    fn apply_evidence(
        &mut self,
        evidence: ExpressionTypeEvidence,
    ) -> Result<(), CheckerInfrastructureError> {
        if self
            .expression_type(evidence.expression())
            .is_some_and(|current| current.ty() == evidence.ty() && !current.is_recovered())
        {
            return Ok(());
        }

        self.add_evidence(evidence.expression(), evidence.ty())
    }

    pub(crate) fn add_expectation(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        self.validate_expression_and_type(expression, ty)?;

        match add_expectations(
            self.request,
            [ExpressionTypeExpectation::new(expression, ty)],
            &self.variables,
            &mut self.inference,
        )? {
            Some(()) => Ok(()),
            None => Ok(()),
        }
    }

    pub(crate) fn expression_type(
        &mut self,
        expression: BoundExpressionId,
    ) -> Option<ExpressionTypeResult> {
        let variable = self.variables.get(&expression).copied()?;

        self.inference.result(variable)
    }

    pub(crate) fn propagate(&mut self) -> Result<SessionProgress<()>, CheckerInfrastructureError> {
        loop {
            if self.request.is_cancelled() {
                return Ok(SessionProgress::Cancelled);
            }

            match propagate_dynamic_constraints(
                self.request,
                &self.expressions,
                &self.variables,
                &self.block_variables,
                &self.block_owners,
                &self.types,
                &mut self.inference,
            )? {
                Some(true) => {}
                Some(false) => return Ok(SessionProgress::Complete(())),
                None => return Ok(SessionProgress::Cancelled),
            }
        }
    }

    pub(super) fn finish(self) -> FinishedExpressionTypes {
        let ordered_variables = self
            .expressions
            .iter()
            .filter_map(|expression| {
                self.variables
                    .get(expression)
                    .copied()
                    .map(|variable| (*expression, variable))
            })
            .collect::<Vec<_>>();

        let (results, conflicts, unresolved) = self.inference.finish(&ordered_variables);

        FinishedExpressionTypes {
            results,
            conflicts,
            unresolved,
        }
    }

    fn validate_expression_and_type(
        &self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        if !self.variables.contains_key(&expression) {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        }

        self.request
            .semantic_values()
            .type_data(ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        Ok(())
    }

    fn add_return_expectations(
        &mut self,
        result_type: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        for index in 0..self.expressions.len() {
            if self.request.is_cancelled() {
                return Ok(());
            }

            let expression = self.expressions[index];

            let Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) =
                self.request.view().expression(expression)
            else {
                continue;
            };

            if transfer.kind() != bray_bound_tree::BoundControlTransferKind::Return {
                continue;
            }

            match transfer.operand() {
                Some(operand) => self.add_expectation(operand, result_type)?,
                None if result_type != self.types.unit => {
                    let Some(variable) = self.variables.get(&expression).copied() else {
                        continue;
                    };

                    self.inference.add_directional_conflict(
                        expression,
                        result_type,
                        self.types.unit,
                    );
                    self.inference.mark_recovered(variable);
                }
                None => {}
            }
        }

        Ok(())
    }
}

struct CollectedNodes {
    expressions: Vec<BoundExpressionId>,
    blocks: Vec<BoundBlockId>,
}

fn collect_nodes<C>(
    request: UnitCheckRequest<'_, C>,
) -> Result<Option<CollectedNodes>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let root = match request.root() {
        crate::UnitCheckRoot::CallableBody(body) => AnyBoundNodeId::from(body),
        crate::UnitCheckRoot::Expression(expression) => AnyBoundNodeId::from(expression),
        crate::UnitCheckRoot::ExpressionSequence(block) => AnyBoundNodeId::from(block),
    };

    let mut expressions = BTreeSet::new();
    let mut blocks = BTreeSet::new();
    let outcome = walk_bound_unit_view(request.view(), root, |event| {
        if request.is_cancelled() {
            return BoundWalkControl::Stop;
        }

        if let BoundWalkEvent::Enter(node) = event {
            match node {
                AnyBoundNodeId::Expression(id) => {
                    expressions.insert(id);
                }
                AnyBoundNodeId::Block(id) => {
                    blocks.insert(id);
                }
                AnyBoundNodeId::Pattern(_) | AnyBoundNodeId::CallableBody(_) => {}
            }
        }

        BoundWalkControl::Continue
    });

    match outcome {
        BoundWalkOutcome::Completed => {}
        BoundWalkOutcome::Stopped => return Ok(None),
        BoundWalkOutcome::MissingNode(node) => {
            return Err(CheckerInfrastructureError::InvalidBoundNode { node });
        }
    }

    Ok(Some(CollectedNodes {
        expressions: expressions.into_iter().collect(),
        blocks: blocks.into_iter().collect(),
    }))
}

fn initialize_expression_variables<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
    types: &CanonicalTypes,
    variables: &mut BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for &expression in expressions {
        if request.is_cancelled() {
            return Ok(());
        }

        let Some(bound) = request.view().expression(expression) else {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        };

        let Some(variable) = inference.fresh(bound.is_recovered()) else {
            return Err(CheckerInfrastructureError::ExpressionTypeCapacityExceeded);
        };

        variables.insert(expression, variable);

        if let Some(ty) = bound.ty() {
            request
                .semantic_values()
                .type_data(ty)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            inference.add_evidence(variable, ty, expression);
        }

        add_intrinsic_constraints(bound, expression, variable, types, inference);
    }

    Ok(())
}

fn initialize_block_variables<C>(
    request: UnitCheckRequest<'_, C>,
    blocks: &[BoundBlockId],
    variables: &mut BTreeMap<BoundBlockId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for &block in blocks {
        if request.is_cancelled() {
            return Ok(());
        }

        let Some(bound) = request.view().block(block) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode {
                node: AnyBoundNodeId::from(block),
            });
        };

        let Some(variable) = inference.fresh(bound.is_recovered()) else {
            return Err(CheckerInfrastructureError::ExpressionTypeCapacityExceeded);
        };

        variables.insert(block, variable);
    }

    Ok(())
}

fn collect_block_owners<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
) -> Result<BTreeMap<BoundBlockId, BoundExpressionId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut owners = BTreeMap::new();

    for &expression in expressions {
        if request.is_cancelled() {
            return Ok(owners);
        }

        let Some(bound) = request.view().expression(expression) else {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        };

        for block in bound.child_blocks() {
            owners.insert(block, expression);
        }
    }

    Ok(owners)
}

fn validate_input<C>(
    request: UnitCheckRequest<'_, C>,
    input: &ExpressionTypeInput,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for evidence in input.evidence() {
        validate_input_pair(request, evidence.expression(), evidence.ty())?;
    }

    for expectation in input.expectations() {
        validate_input_pair(request, expectation.expression(), expectation.ty())?;
    }

    if let Some(result_type) = input.callable_result_type() {
        request
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;
    }

    Ok(())
}

fn validate_input_pair<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
    ty: TypeId,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.view().expression(expression).is_none() {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
    }

    request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(())
}
