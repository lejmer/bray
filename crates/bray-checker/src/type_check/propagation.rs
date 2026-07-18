use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundStructuredExpressionKind, BoundUnitView,
};
use bray_symbols::{
    ConstantTermData, ConstantValueData, ConstantValueKind, IntegerConstant, IntegerSign, TypeData,
    TypeId,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::canonical::CanonicalTypes;
use super::constraints::add_operand_expectation;
use super::inference::{InferenceTypeId, TypeInferenceContext};

pub(super) fn propagate_dynamic_constraints<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    block_owners: &BTreeMap<BoundBlockId, BoundExpressionId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) -> Result<Option<bool>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let before = inference.revision();

    let ordered_variables = expressions
        .iter()
        .filter_map(|expression| {
            variables
                .get(expression)
                .copied()
                .map(|variable| (*expression, variable))
        })
        .collect::<Vec<_>>();

    inference.check_expectations(&ordered_variables);

    if !propagate_blocks(
        request,
        block_variables,
        block_owners,
        variables,
        types,
        inference,
    )? {
        return Ok(None);
    }

    for &expression_id in expressions {
        if request.is_cancelled() {
            return Ok(None);
        }

        propagate_assignment(request.view(), expression_id, variables, inference);
        propagate_control_transfer(
            request.view(),
            expression_id,
            variables,
            block_variables,
            types,
            inference,
        );

        let Some(BoundExpression::Structured(expression)) =
            request.view().expression(expression_id)
        else {
            continue;
        };

        match expression.kind() {
            BoundStructuredExpressionKind::Tuple => infer_tuple(
                request,
                expression_id,
                expression.operands(),
                variables,
                types,
                inference,
            )?,
            BoundStructuredExpressionKind::Array => infer_array(
                request,
                expression_id,
                expression.operands(),
                variables,
                types,
                inference,
            )?,
            _ => {}
        }
    }

    Ok(Some(inference.revision() != before))
}

fn propagate_blocks<C>(
    request: UnitCheckRequest<'_, C>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    block_owners: &BTreeMap<BoundBlockId, BoundExpressionId>,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for (&block_id, &variable) in block_variables {
        if request.is_cancelled() {
            return Ok(false);
        }

        let Some(block) = request.view().block(block_id) else {
            continue;
        };

        if block.is_recovered() {
            inference.mark_recovered(variable);
        }

        let last = block.items().last().and_then(BoundBlockItem::expression);

        let Some(source) = block_owners.get(&block_id).copied() else {
            continue;
        };

        let normal = last
            .and_then(|expression| variables.get(&expression).copied())
            .and_then(|expression| inference.evidence(expression))
            .is_none_or(|ty| ty != types.never);

        if normal {
            inference.add_evidence(variable, types.unit, source);
        } else {
            inference.add_evidence(variable, types.never, source);
        }
    }

    Ok(true)
}

fn propagate_control_transfer(
    view: BoundUnitView<'_>,
    expression_id: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) {
    let Some(BoundExpression::ControlTransfer(transfer)) = view.expression(expression_id) else {
        return;
    };

    let Some(target) = transfer.target() else {
        return;
    };

    match transfer.kind() {
        BoundControlTransferKind::Yield => {
            for (&block_id, &block_variable) in block_variables {
                let Some(block) = view.block(block_id) else {
                    continue;
                };

                if block.origin().source_anchor().syntax() == target {
                    add_transfer_value(
                        block_variable,
                        transfer.operand(),
                        expression_id,
                        variables,
                        types,
                        inference,
                    );
                }
            }
        }
        BoundControlTransferKind::Break => {
            for (&candidate, &candidate_variable) in variables {
                let Some(BoundExpression::Structured(loop_expression)) = view.expression(candidate)
                else {
                    continue;
                };

                if matches!(
                    loop_expression.kind(),
                    BoundStructuredExpressionKind::Loop | BoundStructuredExpressionKind::While
                ) && loop_expression.origin().source_anchor().syntax() == target
                {
                    add_transfer_value(
                        candidate_variable,
                        transfer.operand(),
                        expression_id,
                        variables,
                        types,
                        inference,
                    );
                }
            }
        }
        BoundControlTransferKind::Return | BoundControlTransferKind::Continue => {}
    }
}

fn add_transfer_value(
    target: InferenceTypeId,
    operand: Option<BoundExpressionId>,
    transfer: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) {
    match operand.and_then(|operand| variables.get(&operand).copied()) {
        Some(operand) => inference.unify(target, operand, transfer),
        None => inference.add_evidence(target, types.unit, transfer),
    }
}

fn propagate_assignment(
    view: BoundUnitView<'_>,
    expression_id: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) {
    let Some(BoundExpression::Assignment(assignment)) = view.expression(expression_id) else {
        return;
    };

    let [target, value] = assignment.operands() else {
        return;
    };

    let Some(target) = variables.get(target).copied() else {
        return;
    };

    let Some(expected) = inference.evidence(target) else {
        return;
    };

    add_operand_expectation(Some(*value), Some(expected), variables, inference);
}

fn infer_tuple<C>(
    request: UnitCheckRequest<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(elements) = aggregate_elements(operands, variables, inference) else {
        return Ok(());
    };

    let AggregateElements::Known(elements) = elements else {
        add_recovered_aggregate(expression_id, variables, types, inference);

        return Ok(());
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::tuple(elements))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    add_aggregate_evidence(expression_id, ty, operands, variables, inference);

    Ok(())
}

fn infer_array<C>(
    request: UnitCheckRequest<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(elements) = aggregate_elements(operands, variables, inference) else {
        return Ok(());
    };

    let AggregateElements::Known(elements) = elements else {
        add_recovered_aggregate(expression_id, variables, types, inference);

        return Ok(());
    };

    let Some((&element, remaining)) = elements.split_first() else {
        return Ok(());
    };

    for operand in operands.iter().skip(1) {
        if let Some(variable) = variables.get(operand).copied() {
            inference.add_expectation(variable, element, *operand);
        }
    }

    if remaining.iter().any(|candidate| *candidate != element) {
        return Ok(());
    }

    let length = array_length(request, operands.len(), types.usize)?;
    let ty = request
        .semantic_values()
        .intern_type(TypeData::Array { element, length })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    add_aggregate_evidence(expression_id, ty, operands, variables, inference);

    Ok(())
}

fn aggregate_elements(
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Option<AggregateElements> {
    let mut elements = Vec::with_capacity(operands.len());

    for operand in operands {
        let variable = variables.get(operand).copied()?;

        if inference.is_recovered(variable) {
            return Some(AggregateElements::Recovered);
        }

        elements.push(inference.evidence(variable)?);
    }

    Some(AggregateElements::Known(elements))
}

enum AggregateElements {
    Known(Vec<TypeId>),
    Recovered,
}

fn add_recovered_aggregate(
    expression: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &CanonicalTypes,
    inference: &mut TypeInferenceContext,
) {
    let Some(variable) = variables.get(&expression).copied() else {
        return;
    };

    inference.add_evidence(variable, types.error, expression);
    inference.mark_recovered(variable);
}

fn add_aggregate_evidence(
    expression_id: BoundExpressionId,
    ty: TypeId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) {
    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let recovered = operands.iter().any(|operand| {
        variables
            .get(operand)
            .copied()
            .is_none_or(|operand| inference.is_recovered(operand))
    });

    inference.add_evidence(variable, ty, expression_id);

    if recovered {
        inference.mark_recovered(variable);
    }
}

fn array_length<C>(
    request: UnitCheckRequest<'_, C>,
    length: usize,
    usize_type: TypeId,
) -> Result<bray_symbols::ConstantTermId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let magnitude = u64::try_from(length)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?
        .to_be_bytes();
    let integer = IntegerConstant::new(IntegerSign::NonNegative, magnitude);
    let value = ConstantValueData::new(usize_type, ConstantValueKind::Integer(integer));

    let value = request
        .semantic_values()
        .intern_constant_value(value)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    request
        .semantic_values()
        .intern_constant_term(ConstantTermData::Value(value))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}
