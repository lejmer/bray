use std::collections::BTreeMap;

use bray_bound_tree::{BoundBoxConstructionExpression, BoundExpressionId};
use bray_symbols::TypeData;

use super::super::dependencies::ExpressionTypeDependencies;
use super::super::inference::{InferenceTypeId, TypeInferenceContext};
use crate::{CheckerRequestContext, CheckerUnitView};

pub(super) fn infer_box<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    construction: &BoundBoxConstructionExpression,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) where
    C: CheckerRequestContext + ?Sized,
{
    let Some(variable) = variables.get(&expression).copied() else {
        return;
    };

    let Some(operand) = construction
        .arguments()
        .first()
        .map(|argument| argument.expression())
    else {
        return;
    };

    let Some(operand_variable) = variables.get(&operand).copied() else {
        return;
    };

    let values = request.semantic_values();

    let expected = super::aggregate::contextual_container_element(
        request,
        variable,
        inference,
        |ty| match ty {
            TypeData::OwnedIndirection { target, .. } => Some(*target),
            _ => None,
        },
    );

    let expected = expected.map(|(ty, _)| values.type_data(ty));

    let expected = expected.as_deref().and_then(|ty| match ty {
        TypeData::OwnedIndirection { storage, target } => Some((*storage, *target)),
        _ => None,
    });

    if let Some((_, target)) = expected {
        inference.add_expectation(operand_variable, target, expression);
    }

    let Some(target) = expected
        .map(|(_, target)| target)
        .or_else(|| inference.evidence(operand_variable))
    else {
        return;
    };

    let storage = if construction.policy().is_some() {
        let Some(storage) = types.box_storage_policies.get(&expression).copied() else {
            return;
        };

        storage
    } else if let Some((storage, _)) = expected {
        storage
    } else {
        let Some(heap) = request
            .symbols()
            .compiler_known_provider()
            .heap_storage_policy()
        else {
            return;
        };

        let Some(heap) = values
            .intern_open_named_type(request.symbols(), heap.into()).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_box: {error:?}"))
        else {
            return ;
        };

        heap
    };

    let ty = values
        .intern_type(TypeData::OwnedIndirection { storage, target }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in infer_box: {error:?}"));

    inference.add_evidence(variable, ty, expression);
}
