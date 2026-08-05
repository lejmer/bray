use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::{BinderFactContext, qualified_union_variant};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
    BoundUnitKey, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, ExpressionTypeEntry,
    ExpressionTypeResult, ExpressionTypeStatus, walk_bound_unit_view,
};
use bray_checker::{
    CandidateSelection, DefaultSemanticSelector, ExpressionTypeEvidence, ExpressionTypeExpectation,
    ExpressionTypeInput, OperationCandidate, OperationSelectionRequest, SemanticSelector,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::TypeId;

use super::super::Compilation;
use super::super::binder::CompilationBinderFacts;
use super::super::checker::checker_result;
use super::super::unit::semantic_unit_context_for;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, OperationSelectionFactKey,
};

use super::model::OperationResolution;

impl Compilation {
    pub(in crate::compilation) fn operation_inputs(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<(Vec<OperationResolution>, DiagnosticBag, bool), FactQueryError> {
        let mut expressions = Vec::new();
        let facts = self.binder_facts_for(key, cancellation)?;
        let variant_construction_callees = variant_construction_callees(&facts, bound);

        let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
            if cancellation.is_cancelled() {
                return BoundWalkControl::Stop;
            }

            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(expression) = bound.view().expression(id) else {
                return BoundWalkControl::Stop;
            };

            if selection_kind(expression).is_ok() && !variant_construction_callees.contains(&id) {
                expressions.push(id);
            }

            BoundWalkControl::Continue
        });

        if cancellation.is_cancelled() {
            return Err(FactQueryError::Cancelled);
        }

        if outcome != BoundWalkOutcome::Completed {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let has_operations = !expressions.is_empty();
        let mut resolutions = Vec::with_capacity(expressions.len());
        let mut diagnostics = DiagnosticBag::new();

        for expression in expressions {
            // Each demand-driven operation fact owns the shared bound-unit identity.
            let resolution =
                self.operation_selection_with_cancellation(key.clone(), expression, cancellation)?;

            diagnostics = diagnostics.merged(resolution.diagnostics());

            if let Some(resolution) = resolution.value() {
                // The fact cache and the combined type input own this immutable resolution.
                resolutions.push(resolution.clone());
            }
        }

        Ok((resolutions, diagnostics, has_operations))
    }

    pub(in crate::compilation) fn operation_selection_with_cancellation(
        &self,
        unit: BoundUnitKey,
        expression: BoundExpressionId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<OperationResolution>>>, FactQueryError> {
        let key = OperationSelectionFactKey::new(unit, expression);

        // The cache map, runtime dependency graph, and computation share this fact identity.
        let cell = self.state.operation_selections.cell(key.clone())?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::OperationSelection(key.clone()),
            cancellation,
            || {
                self.compute_operation_selection(&key, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_operation_selection(
        &self,
        key: &OperationSelectionFactKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<OperationResolution>>, FactQueryError> {
        // Independently cached prerequisite facts own the same shared bound-unit identity.
        let bound = self.bound_unit_with_cancellation(key.unit().clone(), cancellation)?;

        let semantics = self
            .provisional_expression_semantics_with_cancellation(key.unit().clone(), cancellation)?;

        let facts = self.binder_facts_for(key.unit(), cancellation)?;

        let expression = bound
            .result()
            .value()
            .view()
            .expression(key.expression())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut diagnostics = bound.result().diagnostics().clone();

        let selection_kind =
            selection_kind_for(&facts, bound.result().value(), key.expression(), expression)?;

        if !matches!(expression, BoundExpression::Call(_))
            && semantics
                .result()
                .value()
                .1
                .expression(key.expression())
                .is_some()
            && selection_kind != bray_bound_tree::SelectionKind::Construction
        {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let Some(types) = self.operation_input_types(
            key,
            bound.result().value(),
            expression,
            selection_kind,
            &semantics.result().value().0,
            cancellation,
            &mut diagnostics,
        )?
        else {
            return Ok(DiagnosticResult::new(None, diagnostics));
        };

        let resolution = match selection_kind {
            bray_bound_tree::SelectionKind::Member => self.resolve_member_operation(
                &facts,
                bound.result().value(),
                &types,
                key.expression(),
                &mut diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Index => {
                let built_in = self.resolve_index_operation(
                    &facts,
                    bound.result().value(),
                    &types,
                    key.expression(),
                    &mut diagnostics,
                )?;

                match built_in {
                    Some(resolution) => Some(resolution),
                    None => self.resolve_custom_index_operation(
                        key,
                        &facts,
                        bound.result().value(),
                        &types,
                        cancellation,
                        &mut diagnostics,
                    )?,
                }
            }
            bray_bound_tree::SelectionKind::Conversion => self.resolve_conversion_operation(
                key,
                &facts,
                bound.result().value(),
                &types,
                cancellation,
                &mut diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Operator => self.resolve_operator_operation(
                key,
                &facts,
                bound.result().value(),
                &types,
                cancellation,
                &mut diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Construction => self.resolve_construction_operation(
                key,
                &facts,
                bound.result().value(),
                &types,
                cancellation,
                &mut diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Implementation
            | bray_bound_tree::SelectionKind::Callable => None,
        };

        Ok(DiagnosticResult::new(resolution, diagnostics))
    }

    fn operation_input_types(
        &self,
        key: &OperationSelectionFactKey,
        unit: &bray_bound_tree::BoundUnit,
        expression: &BoundExpression,
        kind: bray_bound_tree::SelectionKind,
        provisional: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_bound_tree::CheckedExpressionTypes>, FactQueryError> {
        let mut entries = provisional.entries().to_vec();

        for operand in selection_operands(expression, kind) {
            let result = provisional
                .expression(operand)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            if !result.is_recovered() {
                continue;
            }

            let Some(operand_expression) = unit.view().expression(operand) else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            if selection_kind(operand_expression).is_err() {
                return Ok(None);
            }

            // Nested operation selection is an independent demand-driven fact.
            let resolution = self.operation_selection_with_cancellation(
                key.unit().clone(),
                operand,
                cancellation,
            )?;

            *diagnostics = diagnostics.merged(resolution.diagnostics());

            let Some(resolution) = resolution.value() else {
                return Ok(None);
            };

            let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.expression() == operand)
            else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            *entry = ExpressionTypeEntry::new(
                operand,
                ExpressionTypeResult::new(resolution.result_type(), ExpressionTypeStatus::Valid),
            );
        }

        let types = bray_bound_tree::CheckedExpressionTypes::new(
            provisional.unit(),
            provisional.kind(),
            entries,
        );

        Ok(Some(match provisional.callable_result_type() {
            Some(result) => types.with_callable_result_type(result),
            None => types,
        }))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "operation selection keeps lazy fact inputs, cancellation, and diagnostics explicit"
    )]
    pub(super) fn select_operation(
        &self,
        key: &OperationSelectionFactKey,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        operands: impl IntoIterator<Item = BoundExpressionId>,
        candidates: impl IntoIterator<Item = OperationCandidate>,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let context = self.checker_context_for(key.unit(), cancellation)?;

        let semantic_context = semantic_unit_context_for(facts.symbols(), unit)?;

        let request = bray_checker::CheckerUnitView::new(unit, &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(
                    bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                )
            })?;

        let input = OperationSelectionRequest::new(
            key.expression(),
            selection_kind_for(
                facts,
                unit,
                key.expression(),
                unit.view()
                    .expression(key.expression())
                    .ok_or(FactQueryError::InfrastructureFailure)?,
            )?,
            operands,
            candidates,
        );

        let result =
            checker_result(DefaultSemanticSelector.select_operation(request, types, input))?;

        let (selection, selection_diagnostics) = result.into_parts();

        diagnostics.add_range(selection_diagnostics);

        let CandidateSelection::Selected(operation) = selection else {
            return Ok(None);
        };

        let result_type = operation
            .result_type()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let expectations = match &operation {
            bray_bound_tree::SelectedOperation::Construction(construction) => construction
                .inputs()
                .iter()
                .filter_map(|input| match input {
                    bray_bound_tree::SelectedConstructionInput::Explicit {
                        expression, ty, ..
                    } => Some((*expression, *ty)),
                    bray_bound_tree::SelectedConstructionInput::Default { .. } => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };

        Ok(Some(OperationResolution::new(
            key.expression(),
            result_type,
            expectations,
            Some(operation),
        )))
    }
}

pub(in crate::compilation) fn selection_kind(
    expression: &BoundExpression,
) -> Result<bray_bound_tree::SelectionKind, FactQueryError> {
    match expression {
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            Ok(bray_bound_tree::SelectionKind::Member)
        }
        BoundExpression::Unary(_) | BoundExpression::Binary(_) => {
            Ok(bray_bound_tree::SelectionKind::Operator)
        }
        BoundExpression::Conversion(_) => Ok(bray_bound_tree::SelectionKind::Conversion),
        BoundExpression::StructConstruction(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::Call(_) => Ok(bray_bound_tree::SelectionKind::Construction),
        BoundExpression::Structured(expression)
            if matches!(
                expression.kind(),
                BoundStructuredExpressionKind::ElementIndex
                    | BoundStructuredExpressionKind::SliceIndex
            ) =>
        {
            Ok(bray_bound_tree::SelectionKind::Index)
        }
        BoundExpression::Structured(expression)
            if expression.kind() == BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            Ok(bray_bound_tree::SelectionKind::Construction)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

pub(super) fn construction_operands(
    expression: &BoundExpression,
) -> impl Iterator<Item = BoundExpressionId> + '_ {
    let operands = match expression {
        BoundExpression::StructConstruction(construction) => construction
            .fields()
            .iter()
            .map(bray_bound_tree::BoundStructFieldInitializer::expression)
            .collect(),
        BoundExpression::Call(call) => call
            .arguments()
            .iter()
            .map(bray_bound_tree::BoundArgument::expression)
            .collect(),
        BoundExpression::Structured(expression) => expression.operands().to_vec(),
        BoundExpression::LeadingDotVariant(_) => Vec::new(),
        BoundExpression::UnqualifiedVariant(_) => Vec::new(),
        _ => Vec::new(),
    };

    operands.into_iter()
}

fn variant_construction_callees(
    facts: &CompilationBinderFacts<'_>,
    unit: &BoundUnit,
) -> BTreeSet<BoundExpressionId> {
    unit.tree()
        .expressions()
        .filter_map(|(_, expression)| {
            let BoundExpression::Call(call) = expression else {
                return None;
            };

            let callee = unit.view().expression(call.callee())?;

            let is_variant = match callee {
                BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_) => {
                    true
                }
                BoundExpression::MemberAccess(_) => {
                    qualified_union_variant(facts, unit, call.callee()).is_some()
                }
                _ => false,
            };

            is_variant.then_some(call.callee())
        })
        .collect()
}

fn selection_kind_for(
    facts: &CompilationBinderFacts<'_>,
    unit: &BoundUnit,
    expression_id: BoundExpressionId,
    expression: &BoundExpression,
) -> Result<bray_bound_tree::SelectionKind, FactQueryError> {
    if qualified_union_variant(facts, unit, expression_id).is_some() {
        return Ok(bray_bound_tree::SelectionKind::Construction);
    }

    selection_kind(expression)
}

fn selection_operands(
    expression: &BoundExpression,
    kind: bray_bound_tree::SelectionKind,
) -> impl Iterator<Item = BoundExpressionId> + '_ {
    let operands: Vec<_> = if kind == bray_bound_tree::SelectionKind::Construction {
        construction_operands(expression).collect()
    } else {
        expression.child_expressions().collect()
    };

    operands.into_iter()
}

pub(in crate::compilation) fn operation_type_input(
    resolutions: &[OperationResolution],
) -> ExpressionTypeInput {
    ExpressionTypeInput::new()
        .with_evidence(resolutions.iter().map(|resolution| {
            ExpressionTypeEvidence::new(resolution.expression(), resolution.result_type())
        }))
        .with_expectations(resolutions.iter().flat_map(|resolution| {
            resolution
                .expectations()
                .iter()
                .map(|(expression, ty)| ExpressionTypeExpectation::new(*expression, *ty))
        }))
        .with_operation_selections(
            resolutions
                .iter()
                .filter_map(OperationResolution::selection_entry),
        )
}

pub(super) fn expression_type(
    types: &bray_bound_tree::CheckedExpressionTypes,
    expression: BoundExpressionId,
) -> Result<TypeId, FactQueryError> {
    let result = types
        .expression(expression)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    (!result.is_recovered())
        .then_some(result.ty())
        .ok_or(FactQueryError::InfrastructureFailure)
}
