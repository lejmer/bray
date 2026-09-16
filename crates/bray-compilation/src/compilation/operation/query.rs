use std::collections::BTreeSet;

use bray_binder::semantic_unit_context;
use bray_binder::{BindingQueryContext, qualified_union_variant};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
    BoundUnitKey, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, walk_bound_unit_view,
};
use bray_checker::{
    CandidateSelection, DefaultSemanticSelector, ExpressionTypeEvidence, ExpressionTypeExpectation,
    ExpressionTypeInput, OperationCandidate, OperationSelectionRequest, SemanticSelector,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, TypeId};

use super::super::Compilation;
use super::super::binder::CompilationBindingContext;
use super::super::checker::checker_result;

use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

use super::model::{OperationResolution, OperationSubject};

impl Compilation {
    pub(in crate::compilation) fn operation_inputs(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        expressions: &[BoundExpressionId],
        existing: &[OperationResolution],
        cancellation: &CancellationToken,
    ) -> Result<(Vec<OperationResolution>, DiagnosticBag), FactQueryError> {
        let binding_context = self.binding_context_for(key, cancellation)?;

        let existing = existing
            .iter()
            .map(OperationResolution::expression)
            .collect::<BTreeSet<_>>();

        let mut resolutions = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for &expression in expressions {
            if semantics.selections().expression(expression).is_some()
                && !existing.contains(&expression)
            {
                continue;
            }

            // Operation diagnostics retain this shared bound-unit identity.
            let operation_key = OperationSubject::new(key.clone(), expression);

            if let Some(resolution) = self.resolve_operation_selection(
                &operation_key,
                &binding_context,
                bound,
                semantics.types(),
                cancellation,
                &mut diagnostics,
            )? {
                resolutions.push(resolution);
            }
        }

        Ok((resolutions, diagnostics))
    }

    fn resolve_operation_selection(
        &self,
        key: &OperationSubject,
        binding_context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
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

        if selection_kind != bray_bound_tree::SelectionKind::Construction {
            for operand in expression.child_expressions() {
                let result = types.expression(operand).ok_or_else(|| {
                    expression_contract_failure(
                        key.unit(),
                        operand,
                        SemanticQueryViolation::Missing(SemanticDataKind::Type),
                    )
                })?;

                if result.is_recovered() {
                    return Ok(None);
                }
            }
        }

        let resolution = match selection_kind {
            bray_bound_tree::SelectionKind::Member => self.resolve_member_operation(
                binding_context,
                unit,
                types,
                key.expression(),
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Index => {
                let built_in = self.resolve_index_operation(
                    binding_context,
                    unit,
                    types,
                    key.expression(),
                    diagnostics,
                )?;

                match built_in {
                    Some(resolution) => Some(resolution),
                    None => self.resolve_custom_index_operation(
                        key,
                        binding_context,
                        unit,
                        types,
                        cancellation,
                        diagnostics,
                    )?,
                }
            }
            bray_bound_tree::SelectionKind::Conversion => self.resolve_conversion_operation(
                key,
                binding_context,
                unit,
                types,
                cancellation,
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Operator => self.resolve_operator_operation(
                key,
                binding_context,
                unit,
                types,
                cancellation,
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Construction => self.resolve_construction_operation(
                key,
                binding_context,
                unit,
                types,
                cancellation,
                diagnostics,
            )?,
            bray_bound_tree::SelectionKind::Implementation
            | bray_bound_tree::SelectionKind::Callable => None,
        };

        Ok(resolution)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "operation selection keeps lazy query inputs, cancellation, and diagnostics explicit"
    )]
    pub(super) fn select_operation(
        &self,
        key: &OperationSubject,
        binding_context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        operands: impl IntoIterator<Item = BoundExpressionId>,
        candidates: impl IntoIterator<Item = OperationCandidate>,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let context = self.checker_context_for(key.unit(), cancellation)?;

        let semantic_context = semantic_unit_context(binding_context.symbols(), unit);

        let request = bray_checker::CheckerUnitView::new(unit, &semantic_context, &context);

        let input = OperationSelectionRequest::new(
            key.expression(),
            selection_kind_for(
                binding_context,
                unit,
                key.expression(),
                unit.view().expression(key.expression()).unwrap_or_else(|| {
                    panic!("operation subject {key:?} must name a committed expression")
                }),
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

pub(in crate::compilation) fn operation_expressions(
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
    key: &OperationSubject,
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

    use super::{OperationSubject, operation_contract_failure, symbol_contract_failure};
    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::FactQueryError;
    use crate::test_support::callable_body_key;

    #[test]
    fn operation_contract_failure_retains_unit_expression_and_violation() {
        let unit = callable_body_key(7);
        let expression = expression_id(BoundUnitId::new(11), &unit);
        let key = OperationSubject::new(unit.clone(), expression);
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
