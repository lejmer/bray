use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpression, BoundExpressionId, BoundWalkControl,
    BoundWalkEvent, BoundWalkOutcome, CheckedExpressionTypes, ExpressionTypeEntry,
    ExpressionTypeResult, ExpressionTypeStatus, walk_bound_unit_view,
};
use bray_symbols::TypeId;

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::constraints::{
    add_expectations, add_intrinsic_constraints, add_relationship_constraints,
    add_semantic_context_constraints, block_expectations,
};
use super::dependencies::ExpressionTypeDependencies;
use super::inference::{InferenceTypeId, TypeConflict, TypeInferenceContext};
use super::literal::{adapt_contextual_literals, apply_literal_defaults};
use super::propagation::propagate_dynamic_constraints;
use super::region::{ExpressionTypeRegions, initialize_expression_type_regions};
use super::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};

pub(crate) enum SessionProgress<T> {
    Complete(T),
    Cancelled,
}

impl<T> SessionProgress<T> {
    pub(crate) const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub(crate) fn into_value(self) -> Option<T> {
        match self {
            Self::Complete(value) => Some(value),
            Self::Cancelled => None,
        }
    }
}

pub(super) struct FinishedExpressionTypes {
    pub(super) results: Vec<(BoundExpressionId, ExpressionTypeResult)>,
    pub(super) conflicts: Vec<TypeConflict>,
    pub(super) unresolved: Vec<BoundExpressionId>,
    pub(super) callable_result_type: Option<TypeId>,
}

/// Mutable checker-local state shared with operation selection before publication.
pub(crate) struct ExpressionTypeSession<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'view, C>,
    expressions: Vec<BoundExpressionId>,
    variables: BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: BTreeMap<BoundBlockId, InferenceTypeId>,
    regions: ExpressionTypeRegions,
    types: ExpressionTypeDependencies,
    inference: TypeInferenceContext,
    callable_result_type: Option<TypeId>,
}

impl<'view, C> ExpressionTypeSession<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(crate) fn begin(
        request: CheckerUnitView<'view, C>,
    ) -> Result<SessionProgress<Self>, CheckerInfrastructureError> {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(nodes) = collect_nodes(request)? else {
            return Ok(SessionProgress::Cancelled);
        };

        let types = ExpressionTypeDependencies::new(request)?;
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

        initialize_non_value_expressions(
            &nodes.non_value_expressions,
            &variables,
            types.error,
            &mut inference,
        );

        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        initialize_block_variables(request, &nodes.blocks, &mut block_variables, &mut inference)?;

        let block_owners =
            crate::unit::expression_block_owners(request, nodes.expressions.iter().copied())?;

        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let regions = initialize_expression_type_regions(
            request,
            &nodes.expressions,
            &nodes.blocks,
            &variables,
            &block_variables,
            block_owners,
            &mut inference,
        )?;

        if !add_relationship_constraints(
            request,
            &nodes.expressions,
            &variables,
            &block_variables,
            &regions,
            &types,
            &mut inference,
        ) {
            return Ok(SessionProgress::Cancelled);
        }

        add_semantic_context_constraints(request, &variables, types.boolean, &mut inference);

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
            regions,
            types,
            inference,
            callable_result_type: None,
        }))
    }

    pub(crate) fn apply_input(
        &mut self,
        input: &ExpressionTypeInput,
    ) -> Result<(), CheckerInfrastructureError> {
        self.apply_input_replacing_evidence(input, &BTreeSet::new())
    }

    pub(crate) fn apply_input_replacing_evidence(
        &mut self,
        input: &ExpressionTypeInput,
        replacements: &BTreeSet<BoundExpressionId>,
    ) -> Result<(), CheckerInfrastructureError> {
        validate_input(self.request, input)?;

        for &(expression, policy) in input.box_storage_policies() {
            if self
                .types
                .box_storage_policies
                .get(&expression)
                .is_some_and(|previous| *previous != policy)
            {
                return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
            }

            self.types.box_storage_policies.insert(expression, policy);
        }

        let mut applied_replacements = BTreeSet::new();

        for evidence in input.evidence() {
            if replacements.contains(&evidence.expression())
                && applied_replacements.insert(evidence.expression())
            {
                self.replace_evidence(evidence.expression(), evidence.ty())?;
            } else {
                self.apply_evidence(*evidence)?;
            }
        }

        for expectation in input.expectations() {
            self.add_expectation(expectation.expression(), expectation.ty())?;
        }

        if let Some(result_type) = input.callable_result_type() {
            self.add_return_expectations(result_type)?;
            self.callable_result_type = Some(result_type);
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

    fn replace_evidence(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        self.validate_expression_and_type(expression, ty)?;

        let Some(variable) = self.variables.get(&expression).copied() else {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        };

        self.inference.replace_evidence(variable, ty);

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

    pub(crate) fn unique_matching_expectation<E>(
        &mut self,
        expression: BoundExpressionId,
        is_match: impl FnMut(TypeId) -> Result<bool, E>,
    ) -> Result<Option<TypeId>, E> {
        let Some(variable) = self.variables.get(&expression).copied() else {
            return Ok(None);
        };

        self.inference
            .try_unique_matching_expectation(variable, is_match)
    }

    pub(crate) fn preview(&mut self) -> CheckedExpressionTypes {
        let entries = self.expressions.iter().map(|expression| {
            let result = self
                .variables
                .get(expression)
                .copied()
                .and_then(|variable| self.inference.result(variable))
                .unwrap_or_else(|| {
                    ExpressionTypeResult::new(self.types.error, ExpressionTypeStatus::Recovered)
                });

            ExpressionTypeEntry::new(*expression, result)
        });

        let types = CheckedExpressionTypes::new(
            self.request.view().unit(),
            self.request.view().kind(),
            entries,
        );

        match self.callable_result_type {
            Some(ty) => types.with_callable_result_type(ty),
            None => types,
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.inference.revision()
    }

    pub(crate) fn expressions(&self) -> &[BoundExpressionId] {
        &self.expressions
    }

    pub(crate) fn propagate(&mut self) -> Result<SessionProgress<()>, CheckerInfrastructureError> {
        loop {
            if self.request.is_cancelled() {
                return Ok(SessionProgress::Cancelled);
            }

            let Some(propagated) = propagate_dynamic_constraints(
                self.request,
                &self.expressions,
                &self.variables,
                &self.block_variables,
                &self.regions,
                &self.types,
                &mut self.inference,
            )?
            else {
                return Ok(SessionProgress::Cancelled);
            };

            let Some(adapted) = adapt_contextual_literals(
                self.request,
                &self.expressions,
                &self.variables,
                &mut self.inference,
            )?
            else {
                return Ok(SessionProgress::Cancelled);
            };

            if propagated || adapted {
                continue;
            }

            return Ok(SessionProgress::Complete(()));
        }
    }

    pub(crate) fn apply_literal_defaults(&mut self) -> SessionProgress<()> {
        match apply_literal_defaults(
            self.request,
            &self.expressions,
            &self.variables,
            &self.types,
            &mut self.inference,
        ) {
            Some(_) => SessionProgress::Complete(()),
            None => SessionProgress::Cancelled,
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
            callable_result_type: self.callable_result_type,
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
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

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
    non_value_expressions: Vec<BoundExpressionId>,
    blocks: Vec<BoundBlockId>,
}

fn collect_nodes<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<Option<CollectedNodes>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let root = match request.root() {
        crate::CheckerUnitRoot::CallableBody(body) => AnyBoundNodeId::from(body),
        crate::CheckerUnitRoot::Expression(expression) => AnyBoundNodeId::from(expression),
        crate::CheckerUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::from(block),
    };

    let mut expressions = BTreeSet::new();
    let mut non_value_expressions = BTreeSet::new();
    let mut blocks = BTreeSet::new();

    let outcome = walk_bound_unit_view(request.view(), root, |event| {
        if request.is_cancelled() {
            return BoundWalkControl::Stop;
        }

        if let BoundWalkEvent::Enter(node) = event {
            match node {
                AnyBoundNodeId::Expression(id) => {
                    expressions.insert(id);

                    if let Some(bray_bound_tree::BoundExpression::StructConstruction(construction)) =
                        request.view().expression(id)
                        && let Some(head) = construction.head()
                    {
                        non_value_expressions.insert(head);
                    }
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
        non_value_expressions: non_value_expressions.into_iter().collect(),
        blocks: blocks.into_iter().collect(),
    }))
}

fn initialize_non_value_expressions(
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    error: TypeId,
    inference: &mut TypeInferenceContext,
) {
    for expression in expressions {
        let Some(variable) = variables.get(expression).copied() else {
            continue;
        };

        inference.add_evidence(variable, error, *expression);
        inference.mark_recovered(variable);
    }
}

fn initialize_expression_variables<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    types: &ExpressionTypeDependencies,
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

        if let Some(ty) = intrinsic_expression_type(bound) {
            request
                .semantic_values()
                .type_data(ty)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            inference.add_evidence(variable, ty, expression);
        }

        add_intrinsic_constraints(bound, expression, variable, types, inference);
    }

    Ok(())
}

fn intrinsic_expression_type(expression: &BoundExpression) -> Option<TypeId> {
    match expression {
        BoundExpression::Name(name)
            if matches!(
                name.target(),
                bray_bound_tree::BoundReferenceTarget::Surface(
                    bray_symbols::AnySymbolId::ReceiverParameter(_)
                        | bray_symbols::AnySymbolId::CallableParameter(_)
                )
            ) =>
        {
            None
        }
        _ => expression.ty(),
    }
}

fn initialize_block_variables<C>(
    request: CheckerUnitView<'_, C>,
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

fn validate_input<C>(
    request: CheckerUnitView<'_, C>,
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

    for &(expression, policy) in input.box_storage_policies() {
        if !matches!(request.view().expression(expression), Some(BoundExpression::BoxConstruction(construction)) if construction.policy().is_some())
        {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        }

        validate_input_pair(request, expression, policy)?;
    }

    if let Some(result_type) = input.callable_result_type() {
        request
            .semantic_values()
            .type_data(result_type)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;
    }

    Ok(())
}

fn validate_input_pair<C>(
    request: CheckerUnitView<'_, C>,
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
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(())
}
