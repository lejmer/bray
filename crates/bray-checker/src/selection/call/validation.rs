use bray_bound_tree::{
    CheckedExpressionTypes, ConversionTarget, ExpressionTypeResult, SelectedConversion,
    SelectedImplementationWitness, SelectedReceiver,
};
use bray_symbols::{
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, CallableSignature,
    CallableTypeData, ImplementationSelection, ReceiverMode, TypeData,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::super::{
    CallableSelectionRequest, ImplementationSelectionEvidence, ReceiverCapability,
    ReceiverSelection,
};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum CallableSelectionMode {
    Direct,
    Overload,
}

#[derive(Clone, Copy)]
pub(super) enum Compatibility {
    Yes,
    No,
    Recovered,
}

pub(super) fn validate_unit<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    input: &CallableSelectionRequest,
) -> Result<CallableSelectionMode, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if types.unit() != request.view().unit() || types.kind() != request.view().kind() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let Some(bray_bound_tree::BoundExpression::Call(call)) =
        request.view().expression(input.expression())
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if call.arguments() != input.arguments() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let Some(callee) = request.view().expression(call.callee()) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    callable_selection_mode(callee, input)
}

pub(super) fn callable_surface_is_consistent(
    resolution: &bray_bound_tree::BoundResolvedCall,
    signature: &CallableSignature,
    callable: &CallableTypeData,
    defaults: &[(
        CallableParameterSymbolId,
        CallableParameterDefaultProviderSymbolId,
    )],
) -> bool {
    if callable.parameters().len() != signature.parameters().len()
        || callable.result() != signature.result()
        || !callable
            .parameters()
            .iter()
            .zip(signature.parameters())
            .all(|(parameter, signature)| parameter.ty() == signature.ty())
        || defaults.windows(2).any(|pair| pair[0].0 == pair[1].0)
        || defaults.iter().any(|(parameter, _)| {
            !signature
                .parameters()
                .iter()
                .any(|item| item.parameter() == *parameter)
        })
    {
        return false;
    }

    match resolution.result() {
        bray_bound_tree::BoundCallResult::Immediate(result) => result == signature.result(),
        bray_bound_tree::BoundCallResult::LazyFuture(future) => {
            future.completion_type() == signature.result()
        }
    }
}

pub(super) fn selected_witnesses<C>(
    request: UnitCheckRequest<'_, C>,
    resolution: &bray_bound_tree::BoundResolvedCall,
    evidence: &[ImplementationSelectionEvidence],
) -> Result<Option<Vec<SelectedImplementationWitness>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut evidence = evidence.iter().collect::<Vec<_>>();

    evidence.sort_unstable_by_key(|item| item.requirement());

    if evidence
        .windows(2)
        .any(|pair| pair[0].requirement() == pair[1].requirement())
    {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let mut selected = Vec::with_capacity(evidence.len());

    for evidence in evidence {
        let ImplementationSelection::Selected(witness) = evidence.selection() else {
            return Ok(None);
        };

        request
            .semantic_values()
            .implementation_instance_data(*witness)
            .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        selected.push(SelectedImplementationWitness::new(
            evidence.requirement(),
            *witness,
        ));
    }

    let mut witness_ids = selected
        .iter()
        .map(|selection| selection.witness())
        .collect::<Vec<_>>();

    witness_ids.sort_unstable();
    witness_ids.dedup();

    if witness_ids != resolution.implementation_witnesses() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(Some(selected))
}

pub(super) fn callable_type<C>(
    request: UnitCheckRequest<'_, C>,
    signature: &CallableSignature,
) -> Result<std::sync::Arc<TypeData>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(signature.callable_type())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(_) = data.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    Ok(data)
}

pub(super) fn receiver_is_compatible(
    types: &CheckedExpressionTypes,
    actual: Option<ReceiverSelection>,
    expected: Option<bray_symbols::ReceiverParameterSignature>,
) -> Result<Compatibility, CheckerInfrastructureError> {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return Ok(if actual.is_none() && expected.is_none() {
            Compatibility::Yes
        } else {
            Compatibility::No
        });
    };

    let actual_type = expression_type(types, actual.expression())?;

    if actual_type.is_recovered() {
        return Ok(Compatibility::Recovered);
    }

    if actual_type.ty() != expected.ty()
        || !receiver_capability_supports(actual.capability(), expected.mode())
    {
        return Ok(Compatibility::No);
    }

    Ok(Compatibility::Yes)
}

pub(super) fn selected_receiver(
    types: &CheckedExpressionTypes,
    actual: Option<ReceiverSelection>,
    expected: Option<bray_symbols::ReceiverParameterSignature>,
) -> Result<Option<SelectedReceiver>, CheckerInfrastructureError> {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return Ok(None);
    };

    let actual_type = expression_type(types, actual.expression())?.ty();

    Ok(Some(SelectedReceiver::new(
        actual.expression(),
        expected.parameter(),
        identity_conversion(actual_type, expected.ty()),
    )))
}

pub(super) const fn identity_conversion(
    source: bray_symbols::TypeId,
    target: bray_symbols::TypeId,
) -> SelectedConversion {
    SelectedConversion::new(source, target, ConversionTarget::Identity)
}

pub(super) fn expression_type(
    types: &CheckedExpressionTypes,
    expression: bray_bound_tree::BoundExpressionId,
) -> Result<ExpressionTypeResult, CheckerInfrastructureError> {
    types
        .expression(expression)
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)
}

const fn receiver_capability_supports(actual: ReceiverCapability, expected: ReceiverMode) -> bool {
    match expected {
        ReceiverMode::Shared => true,
        ReceiverMode::Mutable => {
            matches!(
                actual,
                ReceiverCapability::Mutable | ReceiverCapability::OwnedMutable
            )
        }
        ReceiverMode::Consuming => {
            matches!(
                actual,
                ReceiverCapability::Owned | ReceiverCapability::OwnedMutable
            )
        }
        ReceiverMode::ConsumingMutable => matches!(actual, ReceiverCapability::OwnedMutable),
    }
}

fn callable_selection_mode(
    callee: &bray_bound_tree::BoundExpression,
    input: &CallableSelectionRequest,
) -> Result<CallableSelectionMode, CheckerInfrastructureError> {
    let is_overload = match callee {
        bray_bound_tree::BoundExpression::MemberAccess(member) => {
            member_selection_is_overload(input, member.receiver())?
        }
        bray_bound_tree::BoundExpression::TraitQualifiedMember(member) => {
            member_selection_is_overload(input, member.receiver())?
        }
        bray_bound_tree::BoundExpression::Name(name) => {
            validate_non_member_request(input)?;

            matches!(
                name.target(),
                bray_bound_tree::BoundReferenceTarget::Surface(symbol)
                    if symbol.kind() == bray_symbols::SymbolKind::CallableOverload
            )
        }
        _ => {
            validate_non_member_request(input)?;

            false
        }
    };

    Ok(if is_overload {
        CallableSelectionMode::Overload
    } else {
        CallableSelectionMode::Direct
    })
}

fn member_selection_is_overload(
    input: &CallableSelectionRequest,
    receiver: bray_bound_tree::BoundExpressionId,
) -> Result<bool, CheckerInfrastructureError> {
    let Some(member) = input.callee_member() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if input.receiver().map(ReceiverSelection::expression) != Some(receiver) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(member.member().kind() == bray_symbols::SymbolKind::CallableOverload)
}

fn validate_non_member_request(
    input: &CallableSelectionRequest,
) -> Result<(), CheckerInfrastructureError> {
    if input.callee_member().is_some() || input.receiver().is_some() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(())
}
