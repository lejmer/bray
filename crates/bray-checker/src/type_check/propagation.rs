use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundStructuredExpressionKind, BoundUnitView,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    ConstantTermData, GenericArgument, IntegerConstant, IntegerSign, TargetSizedIntegerType,
    TypeData, TypeId,
};

use crate::representation::{representation_type, representation_union_type, type_representation};
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::constraints::add_operand_expectation;
use super::dependencies::ExpressionTypeDependencies;
use super::inference::{InferenceTypeId, TypeInferenceContext};
use super::region::{ExpressionTypeRegions, ResultRegionKind};

pub(super) fn propagate_dynamic_constraints<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    types: &ExpressionTypeDependencies,
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
        regions,
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
            regions,
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
            BoundStructuredExpressionKind::ArrayGenerator => infer_array_generator(
                request,
                expression_id,
                expression,
                variables,
                regions,
                types,
                inference,
            )?,
            BoundStructuredExpressionKind::GeneralGenerator => infer_general_generator(
                request,
                expression_id,
                expression,
                variables,
                regions,
                inference,
            )?,
            BoundStructuredExpressionKind::Catch => infer_catch(
                request,
                expression_id,
                expression,
                variables,
                block_variables,
                inference,
            )?,
            _ => {}
        }
    }

    Ok(Some(inference.revision() != before))
}

fn propagate_blocks<C>(
    request: CheckerUnitView<'_, C>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
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

        let Some(source) = regions.block_owner(block_id) else {
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
    regions: &ExpressionTypeRegions,
    types: &ExpressionTypeDependencies,
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
            if let Some(region) = regions.result(target) {
                add_transfer_value(
                    region.variable(),
                    transfer.operand(),
                    expression_id,
                    variables,
                    types,
                    inference,
                );
            }
        }
        BoundControlTransferKind::Break => {
            if let Some(variable) = regions.break_variable(target) {
                add_transfer_value(
                    variable,
                    transfer.operand(),
                    expression_id,
                    variables,
                    types,
                    inference,
                );
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
    types: &ExpressionTypeDependencies,
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
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
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
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
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

    let length = array_length(request, operands.len())?;
    let ty = request
        .semantic_values()
        .intern_type(TypeData::Array { element, length })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    add_aggregate_evidence(expression_id, ty, operands, variables, inference);

    Ok(())
}

fn infer_general_generator<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return Ok(());
    };

    let Some(region) = regions
        .result(expression.origin().source_anchor().syntax())
        .filter(|region| {
            region.owner() == expression_id && region.kind() == ResultRegionKind::GeneralGenerator
        })
    else {
        return Ok(());
    };

    if let Some(expected) = expected_generator_element(request, variable, inference)? {
        inference.add_expectation(region.variable(), expected, expression_id);
    }

    let Some(element) = inference.evidence(region.variable()) else {
        return Ok(());
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Generator(element))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    inference.add_evidence(variable, ty, expression_id);

    Ok(())
}

fn infer_catch<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return Ok(());
    };

    let success = expression
        .operands()
        .first()
        .and_then(|operand| variables.get(operand))
        .copied()
        .or_else(|| {
            expression
                .blocks()
                .first()
                .and_then(|block| block_variables.get(block))
                .copied()
        });

    let Some(success) = success else {
        return Ok(());
    };

    if let Some(expected) = expected_catch_success(request, variable, inference)? {
        inference.add_expectation(success, expected, expression_id);
    }

    let Some(success_type) = inference.evidence(success) else {
        return Ok(());
    };

    let panic_report = representation_type(request, RepresentationRole::PanicReport)?;

    let result = representation_union_type(
        request,
        RepresentationRole::Result,
        [success_type, panic_report],
    )?;

    inference.add_evidence(variable, result, expression_id);

    Ok(())
}

fn expected_catch_success<C>(
    request: CheckerUnitView<'_, C>,
    variable: InferenceTypeId,
    inference: &mut TypeInferenceContext,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let expected = inference.try_unique_matching_expectation(variable, |ty| {
        type_representation(request, ty).map(|role| role == Some(RepresentationRole::Result))
    })?;

    let Some(expected) = expected else {
        return Ok(None);
    };

    let data = request
        .semantic_values()
        .type_data(expected)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Named { substitution, .. } = data.as_ref() else {
        return Ok(None);
    };

    let substitution = request
        .semantic_values()
        .generic_substitution_data(*substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    match substitution
        .bindings()
        .first()
        .map(|binding| binding.argument())
    {
        Some(GenericArgument::Type(success)) => Ok(Some(success)),
        Some(GenericArgument::Constant(_)) | None => Ok(None),
    }
}

fn infer_array_generator<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return Ok(());
    };

    let Some(region) = regions
        .result(expression.origin().source_anchor().syntax())
        .filter(|region| {
            region.owner() == expression_id && region.kind() == ResultRegionKind::ArrayGenerator
        })
    else {
        return Ok(());
    };

    if let Some((ty, element)) = expected_array(request, variable, inference)? {
        inference.add_expectation(region.variable(), element, expression_id);
        inference.add_evidence(variable, ty, expression_id);

        return Ok(());
    }

    let Some(element) = inference.evidence(region.variable()) else {
        return Ok(());
    };

    let Some(length) = generator_source_array_length(request, expression, variables, inference)?
    else {
        return Ok(());
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Array { element, length })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    add_aggregate_evidence(
        expression_id,
        ty,
        expression.operands(),
        variables,
        inference,
    );

    if inference.is_recovered(region.variable()) {
        add_recovered_aggregate(expression_id, variables, types, inference);
    }

    Ok(())
}

fn expected_generator_element<C>(
    request: CheckerUnitView<'_, C>,
    variable: InferenceTypeId,
    inference: &mut TypeInferenceContext,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let expected = inference.try_unique_matching_expectation(variable, |ty| {
        request
            .semantic_values()
            .type_data(ty)
            .map(|data| matches!(data.as_ref(), TypeData::Generator(_)))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    })?;

    let Some(expected) = expected else {
        return Ok(None);
    };

    let data = request
        .semantic_values()
        .type_data(expected)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    match data.as_ref() {
        TypeData::Generator(element) => Ok(Some(*element)),
        _ => Ok(None),
    }
}

fn expected_array<C>(
    request: CheckerUnitView<'_, C>,
    variable: InferenceTypeId,
    inference: &mut TypeInferenceContext,
) -> Result<Option<(TypeId, TypeId)>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let expected = inference.try_unique_matching_expectation(variable, |ty| {
        request
            .semantic_values()
            .type_data(ty)
            .map(|data| matches!(data.as_ref(), TypeData::Array { .. }))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    })?;

    let Some(expected) = expected else {
        return Ok(None);
    };

    let data = request
        .semantic_values()
        .type_data(expected)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    match data.as_ref() {
        TypeData::Array { element, .. } => Ok(Some((expected, *element))),
        _ => Ok(None),
    }
}

fn generator_source_array_length<C>(
    request: CheckerUnitView<'_, C>,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<Option<bray_symbols::ConstantTermId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(iteration) = expression.operands().first().copied() else {
        return Ok(None);
    };

    let Some(BoundExpression::Generator(iteration)) = request.view().expression(iteration) else {
        return Ok(None);
    };

    let Some(source) = variables.get(&iteration.source()).copied() else {
        return Ok(None);
    };

    let Some(source) = inference.evidence(source) else {
        return Ok(None);
    };

    let data = request
        .semantic_values()
        .type_data(source)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    match data.as_ref() {
        TypeData::Array { length, .. } => Ok(Some(*length)),
        _ => Ok(None),
    }
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
    types: &ExpressionTypeDependencies,
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
    request: CheckerUnitView<'_, C>,
    length: usize,
) -> Result<bray_symbols::ConstantTermId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let magnitude = u64::try_from(length)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?
        .to_be_bytes();

    let integer = IntegerConstant::new(IntegerSign::NonNegative, magnitude);

    request
        .semantic_values()
        .intern_constant_term(ConstantTermData::IntegerLiteral {
            ty: TargetSizedIntegerType::Usize,
            value: integer,
        })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}
