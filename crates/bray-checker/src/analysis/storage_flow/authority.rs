use std::collections::BTreeSet;

use bray_bound_tree::{StorageBinding, StorageBindingTarget, StorageIdentityId, StoragePlan};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableParameterMode, CallableSignatureFact, CallableSymbolId, ReceiverMode,
    SymbolFactRequest, TypeData, TypeExpressionTemplate,
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
    let Some(callable) = containing_callable(request) else {
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

fn containing_callable<C>(request: CheckerUnitView<'_, C>) -> Option<CallableSymbolId>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut symbol = match request.semantic_context() {
        SemanticUnitContext::CallableBody(context)
        | SemanticUnitContext::RuntimeDefault(context)
        | SemanticUnitContext::ConstantTemplate(context)
        | SemanticUnitContext::EmbeddedConstant(context)
        | SemanticUnitContext::PredicateDefinition(context)
        | SemanticUnitContext::Constraint(context)
        | SemanticUnitContext::TargetGate(context) => context.declaration(),
        SemanticUnitContext::ContractClause(context) => context.declaration().declaration(),
        SemanticUnitContext::AnonymousCallable(_) => return None,
    };

    loop {
        if let Some(callable) = CallableSymbolId::try_from_any(symbol) {
            return Some(callable);
        }

        symbol = request.symbols().containing_symbol(symbol)?;
    }
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
