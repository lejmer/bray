use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::{BindingQueryContext, qualified_union_variant};
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
use bray_symbols::{AnySymbolId, TypeId};

use super::super::Compilation;
use super::super::binder::CompilationBindingContext;
use super::super::checker::checker_result;
use super::super::unit::semantic_unit_context_for;
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, OperationSelectionQueryKey,
};

use super::model::OperationResolution;

impl Compilation {
    pub(in crate::compilation) fn operation_inputs(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<(Vec<OperationResolution>, DiagnosticBag, bool), FactQueryError> {
        let binding_context = self.binding_context_for(key, cancellation)?;
        let expressions = operation_expressions(&binding_context, bound, cancellation)?;

        let has_operations = !expressions.is_empty();
        let mut resolutions = Vec::with_capacity(expressions.len());
        let mut diagnostics = DiagnosticBag::new();

        for expression in expressions {
            // Each demand-driven operation selection owns the shared bound-unit identity.
            let resolution =
                self.operation_selection_with_cancellation(key.clone(), expression, cancellation)?;

            diagnostics = diagnostics.merged(resolution.diagnostics());

            if let Some(resolution) = resolution.value() {
                // The query cache and the combined type input own this immutable resolution.
                resolutions.push(resolution.clone());
            }
        }

        Ok((resolutions, diagnostics, has_operations))
    }

    pub(in crate::compilation) fn additional_operation_inputs(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        existing: &[OperationResolution],
        cancellation: &CancellationToken,
    ) -> Result<(Vec<OperationResolution>, DiagnosticBag), FactQueryError> {
        let binding_context = self.binding_context_for(key, cancellation)?;
        let expressions = operation_expressions(&binding_context, bound, cancellation)?;

        let resolved = existing
            .iter()
            .map(OperationResolution::expression)
            .collect::<BTreeSet<_>>();

        let mut resolutions = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for expression in expressions {
            if resolved.contains(&expression)
                || semantics.selections().expression(expression).is_some()
            {
                continue;
            }

            let operation_key = OperationSelectionQueryKey::new(key.clone(), expression);

            if let Some(resolution) = self.resolve_operation_selection(
                &operation_key,
                &binding_context,
                bound,
                semantics.types(),
                semantics.selections(),
                cancellation,
                &mut diagnostics,
            )? {
                resolutions.push(resolution);
            }
        }

        Ok((resolutions, diagnostics))
    }

    pub(in crate::compilation) fn operation_selection_with_cancellation(
        &self,
        unit: BoundUnitKey,
        expression: BoundExpressionId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<OperationResolution>>>, FactQueryError> {
        let key = OperationSelectionQueryKey::new(unit, expression);

        // The cache map, runtime dependency graph, and computation share this query identity.
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
        key: &OperationSelectionQueryKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<OperationResolution>>, FactQueryError> {
        // Independently cached prerequisite binding_context own the same shared bound-unit identity.
        let bound = self.bound_unit_with_cancellation(key.unit().clone(), cancellation)?;

        let semantics = self
            .provisional_expression_semantics_with_cancellation(key.unit().clone(), cancellation)?;

        let binding_context = self.binding_context_for(key.unit(), cancellation)?;

        let mut diagnostics = bound.result().diagnostics().clone();

        let resolution = self.resolve_operation_selection(
            key,
            &binding_context,
            bound.result().value(),
            semantics.result().value().types(),
            semantics.result().value().selections(),
            cancellation,
            &mut diagnostics,
        )?;

        Ok(DiagnosticResult::new(resolution, diagnostics))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "operation resolution keeps its exact unit, semantic input, and cancellation context explicit"
    )]
    fn resolve_operation_selection(
        &self,
        key: &OperationSelectionQueryKey,
        binding_context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let expression = unit.view().expression(key.expression()).ok_or_else(|| {
            operation_contract_failure(
                key,
                SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
            )
        })?;

        let selection_kind =
            selection_kind_for(binding_context, unit, key.expression(), expression)?;

        if selections.expression(key.expression()).is_some() {
            return Ok(None);
        }

        let Some(types) = self.operation_input_types(
            key,
            unit,
            expression,
            selection_kind,
            types,
            selections,
            cancellation,
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        let resolution = match selection_kind {
            bray_bound_tree::SelectionKind::Member => self.resolve_member_operation(
                binding_context,
                unit,
                &types,
                key.expression(),
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Index => {
                let built_in = self.resolve_index_operation(
                    binding_context,
                    unit,
                    &types,
                    key.expression(),
                    diagnostics,
                )?;

                match built_in {
                    Some(resolution) => Some(resolution),
                    None => self.resolve_custom_index_operation(
                        key,
                        binding_context,
                        unit,
                        &types,
                        cancellation,
                        diagnostics,
                    )?,
                }
            }
            bray_bound_tree::SelectionKind::Conversion => self.resolve_conversion_operation(
                key,
                binding_context,
                unit,
                &types,
                cancellation,
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Operator => self.resolve_operator_operation(
                key,
                binding_context,
                unit,
                &types,
                cancellation,
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Construction => self.resolve_construction_operation(
                key,
                binding_context,
                unit,
                &types,
                cancellation,
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Implementation
            | bray_bound_tree::SelectionKind::Callable => None,
        };

        Ok(resolution)
    }

    fn operation_input_types(
        &self,
        key: &OperationSelectionQueryKey,
        unit: &BoundUnit,
        expression: &BoundExpression,
        kind: bray_bound_tree::SelectionKind,
        provisional: &bray_bound_tree::CheckedExpressionTypes,
        provisional_selections: &bray_bound_tree::CheckedSemanticSelections,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_bound_tree::CheckedExpressionTypes>, FactQueryError> {
        let mut entries = provisional.entries().to_vec();

        for operand in selection_operands(expression, kind) {
            let result = provisional.expression(operand).ok_or_else(|| {
                expression_contract_failure(
                    key.unit(),
                    operand,
                    SemanticQueryViolation::Missing(SemanticDataKind::Type),
                )
            })?;

            if !result.is_recovered() {
                continue;
            }

            if kind == bray_bound_tree::SelectionKind::Construction {
                continue;
            }

            if let Some(ty) = provisional_selections
                .expression(operand)
                .and_then(bray_bound_tree::SemanticSelection::result_type)
            {
                replace_expression_type(key, &mut entries, operand, ty)?;

                continue;
            }

            let Some(operand_expression) = unit.view().expression(operand) else {
                return Err(expression_contract_failure(
                    key.unit(),
                    operand,
                    SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
                ));
            };

            if selection_kind(unit, operand, operand_expression).is_err() {
                return Ok(None);
            }

            // Nested operation selection is an independent demand-driven query.
            let resolution = self.operation_selection_with_cancellation(
                key.unit().clone(),
                operand,
                cancellation,
            )?;

            *diagnostics = diagnostics.merged(resolution.diagnostics());

            let Some(resolution) = resolution.value() else {
                return Ok(None);
            };

            replace_expression_type(key, &mut entries, operand, resolution.result_type())?;
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
        reason = "operation selection keeps lazy query inputs, cancellation, and diagnostics explicit"
    )]
    pub(super) fn select_operation(
        &self,
        key: &OperationSelectionQueryKey,
        binding_context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        operands: impl IntoIterator<Item = BoundExpressionId>,
        candidates: impl IntoIterator<Item = OperationCandidate>,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let context = self.checker_context_for(key.unit(), cancellation)?;

        let semantic_context = semantic_unit_context_for(binding_context.symbols(), unit)?;

        let request =
            crate::compilation::unit::checker_unit_view(unit, &semantic_context, &context)?;

        let input = OperationSelectionRequest::new(
            key.expression(),
            selection_kind_for(
                binding_context,
                unit,
                key.expression(),
                unit.view().expression(key.expression()).ok_or_else(|| {
                    operation_contract_failure(
                        key,
                        SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
                    )
                })?,
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

        let result_type = operation.result_type().ok_or_else(|| {
            operation_contract_failure(key, SemanticQueryViolation::Missing(SemanticDataKind::Type))
        })?;

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

fn replace_expression_type(
    key: &OperationSelectionQueryKey,
    entries: &mut [ExpressionTypeEntry],
    expression: BoundExpressionId,
    ty: TypeId,
) -> Result<(), FactQueryError> {
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.expression() == expression)
    else {
        return Err(expression_contract_failure(
            key.unit(),
            expression,
            SemanticQueryViolation::Missing(SemanticDataKind::Type),
        ));
    };

    *entry = ExpressionTypeEntry::new(
        expression,
        ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid),
    );

    Ok(())
}

pub(in crate::compilation) fn selection_kind(
    unit: &BoundUnit,
    expression_id: BoundExpressionId,
    expression: &BoundExpression,
) -> Result<bray_bound_tree::SelectionKind, FactQueryError> {
    match expression {
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            Ok(bray_bound_tree::SelectionKind::Member)
        }
        BoundExpression::Unary(_) | BoundExpression::Binary(_) => {
            Ok(bray_bound_tree::SelectionKind::Operator)
        }
        BoundExpression::Assignment(expression)
            if expression.operator().binary_operator().is_some() =>
        {
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
        BoundExpression::BoxConstruction(_) => Ok(bray_bound_tree::SelectionKind::Construction),
        _ => Err(expression_contract_failure(
            unit.key(),
            expression_id,
            SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
        )),
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
        BoundExpression::BoxConstruction(expression) => expression
            .arguments()
            .iter()
            .map(bray_bound_tree::BoundArgument::expression)
            .collect(),
        BoundExpression::LeadingDotVariant(_) => Vec::new(),
        BoundExpression::UnqualifiedVariant(_) => Vec::new(),
        _ => Vec::new(),
    };

    operands.into_iter()
}

fn variant_construction_callees(
    binding_context: &CompilationBindingContext<'_>,
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
                    qualified_union_variant(binding_context, unit, call.callee()).is_some()
                }
                _ => false,
            };

            is_variant.then_some(call.callee())
        })
        .collect()
}

fn operation_expressions(
    binding_context: &CompilationBindingContext<'_>,
    unit: &BoundUnit,
    cancellation: &CancellationToken,
) -> Result<Vec<BoundExpressionId>, FactQueryError> {
    let variant_construction_callees = variant_construction_callees(binding_context, unit);
    let mut expressions = Vec::new();
    let mut walk_failure = None;

    let outcome = walk_bound_unit_view(unit.view(), unit.root(), |event| {
        if let Err(error) = cancellation.check() {
            walk_failure = Some(error);

            return BoundWalkControl::Stop;
        }

        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event else {
            return BoundWalkControl::Continue;
        };

        let Some(expression) = unit.view().expression(id) else {
            walk_failure = Some(expression_contract_failure(
                unit.key(),
                id,
                SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
            ));

            return BoundWalkControl::Stop;
        };

        if selection_kind(unit, id, expression).is_ok()
            && !variant_construction_callees.contains(&id)
        {
            expressions.push(id);
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = walk_failure {
        return Err(error);
    }

    match outcome {
        BoundWalkOutcome::Completed => {}
        BoundWalkOutcome::MissingNode(node) => {
            return Err(unit_contract_failure(
                unit.key(),
                SemanticQueryViolation::MissingBoundNode(node),
            ));
        }
        BoundWalkOutcome::Stopped => {
            return Err(unit_contract_failure(
                unit.key(),
                SemanticQueryViolation::UnexpectedWalkOutcome(outcome),
            ));
        }
    }

    Ok(expressions)
}

fn selection_kind_for(
    binding_context: &CompilationBindingContext<'_>,
    unit: &BoundUnit,
    expression_id: BoundExpressionId,
    expression: &BoundExpression,
) -> Result<bray_bound_tree::SelectionKind, FactQueryError> {
    if qualified_union_variant(binding_context, unit, expression_id).is_some() {
        return Ok(bray_bound_tree::SelectionKind::Construction);
    }

    selection_kind(unit, expression_id, expression)
}

pub(super) fn operation_contract_failure(
    key: &OperationSelectionQueryKey,
    violation: SemanticQueryViolation,
) -> FactQueryError {
    expression_contract_failure(key.unit(), key.expression(), violation)
}

pub(super) fn expression_contract_failure(
    unit: &BoundUnitKey,
    expression: BoundExpressionId,
    violation: SemanticQueryViolation,
) -> FactQueryError {
    SemanticQueryFailure::contract(
        SemanticQueryContext::Expression {
            unit: unit.clone(),
            expression,
        },
        violation,
    )
    .into()
}

pub(super) fn unit_contract_failure(
    unit: &BoundUnitKey,
    violation: SemanticQueryViolation,
) -> FactQueryError {
    SemanticQueryFailure::contract(SemanticQueryContext::Unit(unit.clone()), violation).into()
}

pub(super) fn symbol_contract_failure(
    symbol: AnySymbolId,
    violation: SemanticQueryViolation,
) -> FactQueryError {
    SemanticQueryFailure::contract(SemanticQueryContext::Symbol(symbol), violation).into()
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
    let context = SemanticQueryContext::BoundExpression {
        unit: types.unit(),
        expression,
    };

    let result = types.expression(expression).ok_or_else(|| {
        SemanticQueryFailure::contract(
            context.clone(),
            SemanticQueryViolation::Missing(SemanticDataKind::Type),
        )
    })?;

    (!result.is_recovered())
        .then_some(result.ty())
        .ok_or_else(|| {
            SemanticQueryFailure::contract(
                context,
                SemanticQueryViolation::Unsupported(SemanticDataKind::Type),
            )
            .into()
        })
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundExpressionId, BoundNodeOrigin,
        BoundTreeBuilder, BoundUnitId, BoundUnitKind, CheckedExpressionTypes, ExpressionTypeEntry,
        ExpressionTypeResult, ExpressionTypeStatus, testing::push_expression,
    };
    use bray_symbols::{
        AnySymbolId, FunctionSymbolId, SemanticValueStore, SymbolId, TypeData, TypeId,
    };

    use super::{operation_contract_failure, symbol_contract_failure};
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::{FactQueryError, OperationSelectionQueryKey};
    use crate::test_support::callable_body_key;

    #[test]
    fn operation_contract_failure_retains_unit_expression_and_violation() {
        let unit = callable_body_key(7);
        let expression = expression_id(BoundUnitId::new(11), &unit);
        let key = OperationSelectionQueryKey::new(unit.clone(), expression);
        let violation = SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression);

        assert_eq!(
            operation_contract_failure(&key, violation.clone()),
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Expression { unit, expression },
                violation,
            ))
        );
    }

    #[test]
    fn symbol_contract_failure_retains_exact_operation_subject() {
        let symbol = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(17)));
        let violation = SemanticQueryViolation::Missing(SemanticDataKind::OperationSelection);

        assert_eq!(
            symbol_contract_failure(symbol, violation.clone()),
            FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(symbol),
                violation,
            ))
        );
    }

    #[test]
    fn missing_expression_type_retains_bound_unit_and_expression() {
        let unit = BoundUnitId::new(11);
        let key = callable_body_key(7);
        let expression = expression_id(unit, &key);
        let types = CheckedExpressionTypes::new(unit, BoundUnitKind::CallableBody, []);

        assert_eq!(
            super::expression_type(&types, expression),
            Err(FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::BoundExpression { unit, expression },
                SemanticQueryViolation::Missing(SemanticDataKind::Type),
            )))
        );
    }

    #[test]
    fn recovered_expression_type_retains_bound_unit_and_expression() {
        let unit = BoundUnitId::new(11);
        let key = callable_body_key(7);
        let expression = expression_id(unit, &key);

        let types = CheckedExpressionTypes::new(
            unit,
            BoundUnitKind::CallableBody,
            [ExpressionTypeEntry::new(
                expression,
                ExpressionTypeResult::new(test_type(), ExpressionTypeStatus::Recovered),
            )],
        );

        assert_eq!(
            super::expression_type(&types, expression),
            Err(FactQueryError::from(SemanticQueryFailure::contract(
                SemanticQueryContext::BoundExpression { unit, expression },
                SemanticQueryViolation::Unsupported(SemanticDataKind::Type),
            )))
        );
    }

    fn expression_id(unit: BoundUnitId, key: &bray_bound_tree::BoundUnitKey) -> BoundExpressionId {
        let mut tree = BoundTreeBuilder::new(unit);

        push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(
                BoundNodeOrigin::source(key.source()),
                test_type(),
            )),
        )
    }

    fn test_type() -> TypeId {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store must build: {error:?}"));

        values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("error type must intern: {error:?}"))
    }
}
