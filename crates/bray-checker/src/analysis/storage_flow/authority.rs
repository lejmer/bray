use std::collections::BTreeSet;

use bray_bound_tree::{StorageBinding, StorageBindingTarget, StorageIdentityId, StoragePlan};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableParameterMode, CallableSignatureQuery, ReceiverMode, SymbolQueryRequest, TypeData,
    TypeExpressionTemplate,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerQueryResult, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView, SemanticUnitContext,
};

pub(super) fn mutable_storage<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> CheckerQueryResult<(BTreeSet<StorageIdentityId>, DiagnosticBag)>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
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

    let signature = request
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))?;

    let mut mutable = BTreeSet::new();

    let modes = parameter_modes(request, signature.value().callable_type())?;

    if modes.len() != signature.value().parameters().len() {
        return Err(CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::InvalidStorageFlow,
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

    // The storage analysis owns diagnostics independently of the shared symbol query result.
    Ok((mutable, signature.diagnostics().clone()))
}

fn parameter_modes<C>(
    request: CheckerUnitView<'_, C>,
    callable: &TypeExpressionTemplate,
) -> CheckerQueryResult<Vec<CallableParameterMode>>
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
                CheckerQueryError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerQueryError::Infrastructure(
                    CheckerInfrastructureError::InvalidStorageFlow,
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
        | TypeExpressionTemplate::TraitView(_) => Err(CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::InvalidStorageFlow,
        )),
    }
}

fn insert_identity(
    mutable: &mut BTreeSet<StorageIdentityId>,
    storage: &StoragePlan,
    target: StorageBindingTarget,
) {
    let identity = match storage.binding(target) {
        Some(StorageBinding::Identity(identity)) => Some(identity),
        Some(StorageBinding::Access(access)) => storage.root_identity(access),
        None => None,
    };

    if let Some(identity) = identity {
        mutable.insert(identity);
    }
}
