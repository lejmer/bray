use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundStructuredExpressionKind, BoundUnitView,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{BorrowKind, GenericArgument, TypeData};

use crate::representation::{representation_union_type, type_representation};
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::ExpressionTypeExpectation;
use super::super::constraints::{add_expectations, add_operand_expectation};
use super::super::dependencies::ExpressionTypeDependencies;
use super::super::inference::{InferenceTypeId, TypeInferenceContext};
use super::super::region::ExpressionTypeRegions;
use super::aggregate::{
    infer_array, infer_array_generator, infer_catch, infer_general_generator, infer_range,
    infer_repeated_array, infer_tuple,
};

pub(crate) fn propagate_dynamic_constraints<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> Option<bool>
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
    ) {
        return None;
    }

    for &expression_id in expressions {
        if request.is_cancelled() {
            return None;
        }

        propagate_assignment(request, expression_id, variables, inference);

        propagate_control_transfer(request, expression_id, variables, regions, types, inference);

        if let Some(BoundExpression::BoxConstruction(construction)) =
            request.view().expression(expression_id)
        {
            super::box_construction::infer_box(
                request,
                expression_id,
                construction,
                variables,
                types,
                inference,
            );

            continue;
        }

        if let Some(BoundExpression::Await(expression)) = request.view().expression(expression_id) {
            infer_await(
                request,
                expression_id,
                expression.operand(),
                variables,
                inference,
            );

            continue;
        }

        let Some(BoundExpression::Structured(expression)) =
            request.view().expression(expression_id)
        else {
            continue;
        };

        match expression.kind() {
            BoundStructuredExpressionKind::With => {
                if let Some(&failure) = types.scoped_failures.get(&expression_id)
                    && let Some(&variable) = variables.get(&expression_id)
                    && let Some(&body) = expression
                        .blocks()
                        .first()
                        .and_then(|body| block_variables.get(body))
                {
                    let mut expected_success = None;

                    if let Some(expected) = inference.unique_expectation(variable)
                        && type_representation(request, expected)
                            == Some(RepresentationRole::Result)
                        && let TypeData::Named { substitution, .. } =
                            request.semantic_values().type_data(expected).as_ref()
                        && let Some(GenericArgument::Type(success)) = request
                            .semantic_values()
                            .generic_substitution_data(*substitution)
                            .bindings()
                            .first()
                            .map(|binding| binding.argument())
                    {
                        expected_success = Some(success);
                        inference.add_expectation(body, success, expression_id);
                    }

                    if let Some(success) = inference.evidence(body) {
                        let success = if success == types.never {
                            expected_success.unwrap_or(success)
                        } else {
                            success
                        };

                        let result = representation_union_type(
                            request,
                            RepresentationRole::Result,
                            [success, failure],
                        );

                        inference.add_evidence(variable, result, expression_id);
                    }
                }
            }
            BoundStructuredExpressionKind::Tuple => infer_tuple(
                request,
                expression_id,
                expression.operands(),
                variables,
                types,
                inference,
            ),
            BoundStructuredExpressionKind::Array => infer_array(
                request,
                expression_id,
                expression.operands(),
                variables,
                types,
                inference,
            ),
            BoundStructuredExpressionKind::RepeatedArray => infer_repeated_array(
                request,
                expression_id,
                expression.operands(),
                variables,
                inference,
            ),
            BoundStructuredExpressionKind::Range => infer_range(
                request,
                expression_id,
                expression.operands(),
                variables,
                inference,
            ),
            BoundStructuredExpressionKind::ArrayGenerator => infer_array_generator(
                request,
                expression_id,
                expression,
                variables,
                regions,
                types,
                inference,
            ),
            BoundStructuredExpressionKind::GeneralGenerator => infer_general_generator(
                request,
                expression_id,
                expression,
                variables,
                regions,
                inference,
            ),
            BoundStructuredExpressionKind::Catch => infer_catch(
                request,
                expression_id,
                expression,
                variables,
                block_variables,
                inference,
            ),
            BoundStructuredExpressionKind::Absence => {
                infer_absence(request, expression_id, variables, inference)
            }
            BoundStructuredExpressionKind::Borrow => {
                infer_borrow(request, expression_id, expression, variables, inference)
            }
            BoundStructuredExpressionKind::ResultPropagation => {
                infer_result_propagation(request, expression_id, expression, variables, inference)
            }
            _ => {}
        }
    }

    Some(inference.revision() != before)
}

fn infer_await<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    operand: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression).copied() else {
        return;
    };

    let Some(operand_variable) = variables.get(&operand).copied() else {
        return;
    };

    if let Some(future) = inference.evidence(operand_variable) {
        let completion = request
            .available_compiler_known_symbols()
            .unary_representation_argument(
                request.semantic_values(),
                RepresentationRole::Future,
                future,
            );

        if let Some(completion) = completion {
            inference.add_evidence(variable, completion, expression);
        }
    }

    let completion = inference
        .evidence(variable)
        .or(inference.unique_expectation(variable));

    let Some(completion) = completion else {
        return;
    };

    let Some(future) = request
        .available_compiler_known_symbols()
        .unary_representation_type(
            request.semantic_values(),
            RepresentationRole::Future,
            completion,
        ).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_await: {error:?}"))
    else {
        return ;
    };

    inference.add_evidence(operand_variable, future, operand);
}

fn infer_absence<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let expected = inference.unique_matching_expectation(variable, |ty| {
        let data = request.semantic_values().type_data(ty);

        matches!(data.as_ref(), TypeData::Nullable(_))
    });

    if let Some(expected) = expected {
        inference.add_evidence(variable, expected, expression_id);
    }
}

fn infer_result_propagation<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let Some(operand) = expression.operands().first() else {
        return;
    };

    let Some(operand_variable) = variables.get(operand).copied() else {
        return;
    };

    if inference.is_recovered(operand_variable) {
        inference.mark_recovered(variable);

        return;
    }

    let Some(operand_type) = inference.evidence(operand_variable) else {
        return;
    };

    let data = request.semantic_values().type_data(operand_type);

    let TypeData::Named { substitution, .. } = data.as_ref() else {
        return;
    };

    if !matches!(
        type_representation(request, operand_type),
        Some(RepresentationRole::Result | RepresentationRole::RunResult)
    ) {
        return;
    }

    let substitution = request
        .semantic_values()
        .generic_substitution_data(*substitution);

    let Some(GenericArgument::Type(success)) = substitution
        .bindings()
        .first()
        .map(|binding| binding.argument())
    else {
        return;
    };

    inference.add_evidence(variable, success, expression_id);
}

fn infer_borrow<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(kind) = expression.borrow_kind() else {
        return;
    };

    let Some(operand) = expression.operands().first().copied() else {
        return;
    };

    let Some(operand_variable) = variables.get(&operand).copied() else {
        return;
    };

    let Some(variable) = variables.get(&expression_id).copied() else {
        return;
    };

    let operand_type = inference.evidence(operand_variable);

    let expected = inference.unique_matching_expectation(variable, |ty| {
        let data = request.semantic_values().type_data(ty);

        matches!(data.as_ref(), TypeData::Borrow { kind: expected, .. } if *expected == kind)
    });

    if let Some(expected) = expected {
        let data = request.semantic_values().type_data(expected);

        let TypeData::Borrow {
            target: expected_target,
            ..
        } = data.as_ref()
        else {
            return;
        };

        let is_reborrow = match operand_type {
            Some(operand_type) => {
                let data = request.semantic_values().type_data(operand_type);

                matches!(
                    data.as_ref(),
                    TypeData::Borrow {
                        kind: operand_kind,
                        target,
                    } if target == expected_target
                        && (*operand_kind == kind || kind == BorrowKind::Shared)
                )
            }
            None => false,
        };

        if !is_reborrow {
            inference.add_expectation(operand_variable, *expected_target, operand);
        }

        inference.add_evidence(variable, expected, expression_id);

        return;
    }

    if let Some(target) = operand_type {
        let target_data = request.semantic_values().type_data(target);

        if matches!(target_data.as_ref(), TypeData::Borrow { .. }) {
            return;
        }

        let ty = request
            .semantic_values()
            .intern_type(TypeData::Borrow { kind, target }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_borrow: {error:?}"));

        inference.add_evidence(variable, ty, expression_id);

        return;
    }
}

fn propagate_blocks<C>(
    request: CheckerUnitView<'_, C>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    for (&block_id, &variable) in block_variables {
        if request.is_cancelled() {
            return false;
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

        if last.is_some_and(|expression| {
            own_block_yield(
                request.view(),
                expression,
                block.origin().source_anchor().syntax(),
            )
        }) {
            continue;
        }

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

    true
}

fn own_block_yield(
    view: BoundUnitView<'_>,
    expression: BoundExpressionId,
    target: bray_declarations::SyntaxAnchor,
) -> bool {
    let Some(BoundExpression::ControlTransfer(transfer)) = view.expression(expression) else {
        return false;
    };

    transfer.kind() == BoundControlTransferKind::Yield && transfer.target() == Some(target)
}

fn propagate_control_transfer<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    regions: &ExpressionTypeRegions,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::ControlTransfer(transfer)) = request.view().expression(expression_id)
    else {
        return;
    };

    let Some(target) = transfer.target() else {
        return;
    };

    match transfer.kind() {
        BoundControlTransferKind::Yield => {
            if let Some(region) = regions.result(target) {
                add_transfer_value(
                    request,
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
                    request,
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

fn add_transfer_value<C>(
    request: CheckerUnitView<'_, C>,
    target: InferenceTypeId,
    operand: Option<BoundExpressionId>,
    transfer: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let nullable_expectation = inference.unique_expectation(target).filter(|expected| {
        matches!(
            request.semantic_values().type_data(*expected).as_ref(),
            TypeData::Nullable(_)
        )
    });

    if let (Some(expected), Some(operand)) = (nullable_expectation, operand) {
        add_expectations(
            request,
            [ExpressionTypeExpectation::new(operand, expected)],
            variables,
            inference,
        );

        inference.add_evidence(target, expected, transfer);

        return;
    }

    match operand.and_then(|operand| variables.get(&operand).copied()) {
        Some(operand) => {
            // A yielded value contributes to the join; the join cannot change its type.
            // In particular, a never-valued operand stays never when another arm yields.
            if let Some(actual) = inference.evidence(operand) {
                inference.add_evidence(target, actual, transfer);
            }

            if let Some(expected) = inference
                .evidence(target)
                .filter(|expected| *expected != types.never && *expected != types.error)
                .or_else(|| inference.unique_expectation(target))
            {
                inference.add_expectation(operand, expected, transfer);
            }

            if inference.is_recovered(operand) {
                inference.mark_recovered(target);
            }
        }
        None => inference.add_evidence(target, types.unit, transfer),
    }
}

fn propagate_assignment<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Assignment(assignment)) = request.view().expression(expression_id)
    else {
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

    let actual = variables
        .get(value)
        .copied()
        .and_then(|value| inference.evidence(value));

    let expected_data = request.semantic_values().type_data(expected);

    let expected = match expected_data.as_ref() {
        TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target,
        } => *target,
        _ => expected,
    };

    if assignment.operator().binary_operator().is_some() {
        if actual.is_none() {
            add_operand_expectation(Some(*value), Some(expected), variables, inference);
        }

        return;
    }

    if matches!(expected_data.as_ref(), TypeData::Nullable(element) if Some(*element) == actual) {
        return;
    }

    add_operand_expectation(Some(*value), Some(expected), variables, inference);
}
