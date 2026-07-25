use std::collections::BTreeSet;

use bray_bound_tree::{StorageBinding, StorageBindingTarget, StorageIdentityId, StoragePlan};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableParameterMode, CallableSignatureFact, ReceiverMode, SymbolFactRequest, TypeData,
    TypeExpressionTemplate,
};

use crate::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView, SemanticUnitContext,
};

pub(super) fn mutable_storage<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> CheckerFactResult<(BTreeSet<StorageIdentityId>, DiagnosticBag)>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if matches!(
        request.semantic_context(),
        SemanticUnitContext::AnonymousCallable(_)
    ) {
        let mut mutable = BTreeSet::new();

        for parameter in request.unit().local_symbols().anonymous_parameters() {
            if parameter.mode() == CallableParameterMode::Mutable {
                insert_identity(
                    &mut mutable,
                    storage,
                    StorageBindingTarget::AnonymousParameter(parameter.id()),
                );
            }
        }

        return Ok((mutable, DiagnosticBag::new()));
    }

    let Some(callable) = request.containing_callable() else {
        return Ok((BTreeSet::new(), DiagnosticBag::new()));
    };

    let signature =
        request.symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))?;

    let mut mutable = BTreeSet::new();

    let modes = parameter_modes(request, signature.value().callable_type())?;

    if modes.len() != signature.value().parameters().len() {
        return Err(CheckerFactError::Infrastructure(
            CheckerInfrastructureError::InvalidStorageFlowFacts,
        ));
    }

    for (parameter, mode) in signature.value().parameters().iter().copied().zip(modes) {
        if mode == CallableParameterMode::Mutable {
            insert_identity(
                &mut mutable,
                storage,
                StorageBindingTarget::Parameter(parameter),
            );
        }
    }

    if let Some(receiver) = signature.value().receiver()
        && matches!(
            receiver.mode(),
            ReceiverMode::Mutable | ReceiverMode::ConsumingMutable
        )
    {
        insert_identity(
            &mut mutable,
            storage,
            StorageBindingTarget::Receiver(receiver.parameter()),
        );
    }

    // The storage fact owns diagnostics independently of the shared symbol fact.
    Ok((mutable, signature.diagnostics().clone()))
}

fn parameter_modes<C>(
    request: CheckerUnitView<'_, C>,
    callable: &TypeExpressionTemplate,
) -> CheckerFactResult<Vec<CallableParameterMode>>
where
    C: CheckerRequestContext + ?Sized,
{
    match callable {
        TypeExpressionTemplate::Callable(callable) => Ok(callable
            .parameters()
            .iter()
            .map(|parameter| parameter.mode())
            .collect()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = request.semantic_values().type_data(*ty).map_err(|_| {
                CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::InvalidStorageFlowFacts,
                ));
            };

            Ok(callable
                .parameters()
                .iter()
                .map(|parameter| parameter.mode())
                .collect())
        }
        TypeExpressionTemplate::Named { .. }
        | TypeExpressionTemplate::TypeValuedMemberProjection { .. }
        | TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::OwnedIndirection { .. }
        | TypeExpressionTemplate::TraitView(_) => Err(CheckerFactError::Infrastructure(
            CheckerInfrastructureError::InvalidStorageFlowFacts,
        )),
    }
}

fn insert_identity(
    mutable: &mut BTreeSet<StorageIdentityId>,
    storage: &StoragePlan,
    target: StorageBindingTarget,
) {
    if let Some(StorageBinding::Identity(identity)) = storage.binding(target) {
        mutable.insert(identity);
    }
}
