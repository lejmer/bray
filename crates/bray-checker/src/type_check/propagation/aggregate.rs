use std::collections::BTreeMap;

use bray_bound_tree::{BoundBlockId, BoundExpression, BoundExpressionId};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    ConstantTermData, GenericArgument, IntegerConstant, IntegerSign, TargetSizedIntegerType,
    TypeData, TypeId,
};

use super::super::dependencies::ExpressionTypeDependencies;
use super::super::inference::{InferenceTypeId, TypeInferenceContext};
use super::super::region::{ExpressionTypeRegions, ResultRegionKind};
use crate::representation::{representation_type, representation_union_type, type_representation};
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(super) fn infer_tuple<C>(
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

pub(super) fn infer_array<C>(
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

pub(super) fn infer_general_generator<C>(
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
    let Some((variable, result_variable)) = generator_inference_variables(
        expression_id,
        expression,
        variables,
        regions,
        ResultRegionKind::GeneralGenerator,
    ) else {
        return Ok(());
    };

    if let Some((_, element)) =
        expected_container_element(request, variable, inference, generator_element)?
    {
        inference.add_expectation(result_variable, element, expression_id);
    }

    let Some(element) = inference.evidence(result_variable) else {
        return Ok(());
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Generator(element))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    inference.add_evidence(variable, ty, expression_id);

    Ok(())
}

pub(super) fn infer_catch<C>(
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

pub(super) fn infer_array_generator<C>(
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
    let Some((variable, result_variable)) = generator_inference_variables(
        expression_id,
        expression,
        variables,
        regions,
        ResultRegionKind::ArrayGenerator,
    ) else {
        return Ok(());
    };

    if let Some((ty, element)) =
        expected_container_element(request, variable, inference, array_element)?
    {
        inference.add_expectation(result_variable, element, expression_id);
        inference.add_evidence(variable, ty, expression_id);

        return Ok(());
    }

    let Some(element) = inference.evidence(result_variable) else {
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

    if inference.is_recovered(result_variable) {
        add_recovered_aggregate(expression_id, variables, types, inference);
    }

    Ok(())
}

fn expected_container_element<C>(
    request: CheckerUnitView<'_, C>,
    variable: InferenceTypeId,
    inference: &mut TypeInferenceContext,
    element: fn(&TypeData) -> Option<TypeId>,
) -> Result<Option<(TypeId, TypeId)>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let expected = inference.try_unique_matching_expectation(variable, |ty| {
        request
            .semantic_values()
            .type_data(ty)
            .map(|data| element(data.as_ref()).is_some())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    })?;

    let Some(expected) = expected else {
        return Ok(None);
    };

    let data = request
        .semantic_values()
        .type_data(expected)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(element(data.as_ref()).map(|element| (expected, element)))
}

const fn generator_element(data: &TypeData) -> Option<TypeId> {
    match data {
        TypeData::Generator(element) => Some(*element),
        _ => None,
    }
}

const fn array_element(data: &TypeData) -> Option<TypeId> {
    match data {
        TypeData::Array { element, .. } => Some(*element),
        _ => None,
    }
}

fn generator_inference_variables(
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    kind: ResultRegionKind,
) -> Option<(InferenceTypeId, InferenceTypeId)> {
    let variable = variables.get(&expression_id).copied()?;

    let result = regions
        .result(expression.origin().source_anchor().syntax())
        .filter(|region| region.owner() == expression_id && region.kind() == kind)?;

    Some((variable, result.variable()))
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
