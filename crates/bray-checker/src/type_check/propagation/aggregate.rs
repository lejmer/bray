use std::collections::BTreeMap;

use bray_bound_tree::{BoundBlockId, BoundExpression, BoundExpressionId};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{GenericArgument, TypeData, TypeId};

use super::super::array::array_length;
use super::super::dependencies::ExpressionTypeDependencies;
use super::super::inference::{InferenceTypeId, TypeInferenceContext};
use super::super::region::{ExpressionTypeRegions, ResultRegionKind};
use crate::representation::{representation_type, representation_union_type, type_representation};
use crate::{CheckerRequestContext, CheckerUnitView};

pub(super) fn infer_tuple<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(elements) = aggregate_elements(operands, variables, inference) else {
        return;
    };

    let AggregateElements::Known(elements) = elements else {
        add_recovered_aggregate(expression_id, variables, types, inference);

        return;
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::tuple(elements)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_tuple: {error:?}"));

    add_aggregate_evidence(expression_id, ty, operands, variables, inference);
}

pub(super) fn infer_array<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let expected = contextual_container_element(request, variable, inference, array_element);

    if let Some((_, element)) = expected {
        for operand in operands {
            if let Some(operand) = variables.get(operand).copied() {
                inference.add_expectation(operand, element, expression_id);
            }
        }
    }

    let Some(elements) = aggregate_elements(operands, variables, inference) else {
        return;
    };

    let AggregateElements::Known(elements) = elements else {
        add_recovered_aggregate(expression_id, variables, types, inference);

        return;
    };

    let Some((&element, remaining)) = elements.split_first() else {
        return;
    };

    for operand in operands.iter().skip(1) {
        if let Some(variable) = variables.get(operand).copied() {
            inference.add_expectation(variable, element, *operand);
        }
    }

    if remaining.iter().any(|candidate| *candidate != element) {
        return;
    }

    let length = array_length(request, operands.len());

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Array { element, length }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_array: {error:?}"));

    add_aggregate_evidence(expression_id, ty, operands, variables, inference);
}

pub(super) fn infer_repeated_array<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let [value, count] = operands else {
        return;
    };

    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let usize = representation_type(request, RepresentationRole::ScalarUsize);

    if let Some(count) = variables.get(count).copied() {
        inference.add_expectation(count, usize, expression_id);
    }

    let Some((ty, element)) =
        contextual_container_element(request, variable, inference, array_element)
    else {
        return;
    };

    if let Some(value) = variables.get(value).copied() {
        inference.add_expectation(value, element, expression_id);
    }

    inference.add_evidence(variable, ty, expression_id);
}

pub(super) fn infer_range<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    operands: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let [start, end] = operands else {
        return;
    };

    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let expected = inference.unique_matching_expectation(variable, |ty| {
        type_representation(request, ty) == Some(RepresentationRole::Range)
    });

    if let Some(range) = expected {
        let Some(element) = request
            .available_compiler_known_symbols()
            .unary_representation_argument(
                request.semantic_values(),
                RepresentationRole::Range,
                range,
            )
        else {
            return;
        };

        if let Some(start) = variables.get(start).copied() {
            inference.add_expectation(start, element, expression_id);
        }

        if let Some(end) = variables.get(end).copied() {
            inference.add_expectation(end, element, expression_id);
        }

        inference.add_evidence(variable, range, expression_id);

        return;
    }

    let Some(start_variable) = variables.get(start).copied() else {
        return;
    };

    let Some(end_variable) = variables.get(end).copied() else {
        return;
    };

    inference.unify(start_variable, end_variable, expression_id);

    let Some(element) = inference
        .evidence(start_variable)
        .or_else(|| inference.evidence(end_variable))
    else {
        return;
    };

    let Some(range) = request
        .available_compiler_known_symbols()
        .unary_representation_type(
            request.semantic_values(),
            RepresentationRole::Range,
            element,
        ).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_range: {error:?}"))
    else {
        panic!("A required compiler-known representation is unavailable for the selected target. in infer_range, role: {:?}", RepresentationRole::Range);
    };

    inference.add_evidence(variable, range, expression_id);

    if inference.is_recovered(start_variable) || inference.is_recovered(end_variable) {
        inference.mark_recovered(variable);
    }
}

pub(super) fn infer_general_generator<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some((variable, result_variable)) = generator_inference_variables(
        expression_id,
        expression,
        variables,
        regions,
        ResultRegionKind::GeneralGenerator,
    ) else {
        return;
    };

    if let Some((_, element)) =
        contextual_container_element(request, variable, inference, generator_element)
    {
        inference.add_expectation(result_variable, element, expression_id);
    }

    let Some(element) = inference.evidence(result_variable) else {
        return;
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Generator(element)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_general_generator: {error:?}"));

    inference.add_evidence(variable, ty, expression_id);
}

pub(super) fn infer_catch<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
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
        return;
    };

    if let Some(expected) = expected_catch_success(request, variable, inference) {
        inference.add_expectation(success, expected, expression_id);
    }

    let Some(success_type) = inference.evidence(success) else {
        return;
    };

    let panic_report = representation_type(request, RepresentationRole::PanicReport);

    let result = representation_union_type(
        request,
        RepresentationRole::Result,
        [success_type, panic_report],
    );

    inference.add_evidence(variable, result, expression_id);
}

fn expected_catch_success<C>(
    request: CheckerUnitView<'_, C>,
    variable: InferenceTypeId,
    inference: &mut TypeInferenceContext,
) -> Option<TypeId>
where
    C: CheckerRequestContext + ?Sized,
{
    let expected = inference.unique_matching_expectation(variable, |ty| {
        type_representation(request, ty) == Some(RepresentationRole::Result)
    });

    let Some(expected) = expected else {
        return None;
    };

    let data = request.semantic_values().type_data(expected);

    let TypeData::Named { substitution, .. } = data.as_ref() else {
        return None;
    };

    let substitution = request
        .semantic_values()
        .generic_substitution_data(*substitution);

    match substitution
        .bindings()
        .first()
        .map(|binding| binding.argument())
    {
        Some(GenericArgument::Type(success)) => Some(success),
        Some(GenericArgument::Constant(_)) | None => None,
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
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some((variable, result_variable)) = generator_inference_variables(
        expression_id,
        expression,
        variables,
        regions,
        ResultRegionKind::ArrayGenerator,
    ) else {
        return;
    };

    if let Some((ty, element)) =
        contextual_container_element(request, variable, inference, array_element)
    {
        inference.add_expectation(result_variable, element, expression_id);
        inference.add_evidence(variable, ty, expression_id);

        return;
    }

    let Some(element) = inference.evidence(result_variable) else {
        return;
    };

    let Some(length) = generator_source_array_length(request, expression, variables, inference)
    else {
        return;
    };

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Array { element, length }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_array_generator: {error:?}"));

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
}

pub(super) fn contextual_container_element<C>(
    request: CheckerUnitView<'_, C>,
    variable: InferenceTypeId,
    inference: &mut TypeInferenceContext,
    element: fn(&TypeData) -> Option<TypeId>,
) -> Option<(TypeId, TypeId)>
where
    C: CheckerRequestContext + ?Sized,
{
    let expected = inference.unique_matching_expectation(variable, |ty| {
        let data = request.semantic_values().type_data(ty);

        element(data.as_ref()).is_some()
    });

    let expected = expected.or_else(|| inference.evidence(variable));

    let Some(expected) = expected else {
        return None;
    };

    let data = request.semantic_values().type_data(expected);

    element(data.as_ref()).map(|element| (expected, element))
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
) -> Option<bray_symbols::ConstantTermId>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(iteration) = expression.operands().first().copied() else {
        return None;
    };

    let Some(BoundExpression::Generator(iteration)) = request.view().expression(iteration) else {
        return None;
    };

    let Some(source) = variables.get(&iteration.source()).copied() else {
        return None;
    };

    let Some(source) = inference.evidence(source) else {
        return None;
    };

    let data = request.semantic_values().type_data(source);

    match data.as_ref() {
        TypeData::Array { length, .. } => Some(*length),
        _ => None,
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
