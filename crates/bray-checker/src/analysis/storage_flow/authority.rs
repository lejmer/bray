use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{StorageBinding, StorageBindingTarget, StorageIdentityId, StoragePlan};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableParameterMode, CallableSignatureQuery, ReceiverMode, SymbolQueryRequest, TypeData,
    TypeExpressionTemplate,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerQueryResult, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerStorageFlowFailure, CheckerUnitView, SemanticUnitContext,
};

pub(super) fn immutable_field_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    input: &super::model::StorageFlowInput,
) -> CheckerQueryResult<BTreeSet<bray_bound_tree::StorageAccessId>, C::UpstreamError> {
    let mut checked = BTreeSet::new();
    let mut immutable = BTreeSet::new();
    let mut fields = BTreeMap::new();

    for plan in storage.access_plans().iter().copied() {
        let Some(access) = input.mutation_authority_access(plan, plan.purpose(), storage) else {
            continue;
        };

        if !checked.insert(access) {
            continue;
        }

        let Some(record) = storage.access(access) else {
            continue;
        };

        let mut allowed = true;

        let Some(root) = storage.root_identity(access) else {
            continue;
        };

        let projections = storage.resolved_projections(access)
            .expect("resolved mutation access must retain its projections");

        for (index, projection) in projections.iter().enumerate() {
            let field = match projection {
                bray_bound_tree::StorageProjection::ProductField(field) => {
                    Some(bray_symbols::AnySymbolId::StructField(*field))
                }
                bray_bound_tree::StorageProjection::ActiveUnionPayloadField { field, .. } => {
                    Some(bray_symbols::AnySymbolId::UnionPayloadField(*field))
                }
                _ => None,
            };

            if let Some(field) = field {
                let mutable = match fields.entry(field) {
                    std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        let mutable = request.member_allows_mutation(field)?;

                        *entry.insert(mutable)
                    }
                };

                allowed &= mutable;
            }

            if allowed {
                continue;
            }

            let stored_type = storage
                .access_at(root, &projections[..=index])
                .and_then(|prefix| storage.access(prefix))
                .map(|prefix| request.semantic_values().type_data(prefix.reached_type()));

            if let Some(stored_type) = stored_type
                && let TypeData::Borrow { target, .. } = stored_type.as_ref()
                && (index + 1 < projections.len() || *target == record.reached_type())
            {
                // Crossing a borrow uses its target authority, not permission to replace its slot.
                allowed = true;
            }
        }

        if !allowed {
            immutable.insert(access);
        }
    }

    Ok(immutable)
}

pub(super) fn mutable_storage<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> CheckerQueryResult<(BTreeSet<StorageIdentityId>, DiagnosticBag), C::UpstreamError>
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

    let modes = parameter_modes(
        request,
        callable.into_any(),
        signature.value().callable_type(),
    )?;

    if modes.len() != signature.value().parameters().len() {
        return Err(CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::CallableParameterCountMismatch {
                    callable: callable.into_any(),
                    signature_parameters: signature.value().parameters().len(),
                    type_parameters: modes.len(),
                },
            ),
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
    callable_symbol: bray_symbols::AnySymbolId,
    callable: &TypeExpressionTemplate,
) -> CheckerQueryResult<Vec<CallableParameterMode>, C::UpstreamError>
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
            let data = request.semantic_values().type_data(*ty);

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerQueryError::Infrastructure(
                    CheckerInfrastructureError::StorageFlow(
                        CheckerStorageFlowFailure::CallableTypeNotCallable {
                            callable: callable_symbol,
                        },
                    ),
                ));
            };

            Ok(callable
                .parameters()
                .iter()
                .map(|parameter| parameter.mode())
                .collect())
        }
        TypeExpressionTemplate::CallableContract { target, .. } => {
            parameter_modes(request, callable_symbol, target)
        }
        TypeExpressionTemplate::Named { .. }
        | TypeExpressionTemplate::TypeValuedMemberProjection { .. }
        | TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::FlexibleArray(_)
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::OwnedIndirection { .. }
        | TypeExpressionTemplate::TraitView(_) => Err(CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::CallableTypeNotCallable {
                    callable: callable_symbol,
                },
            ),
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
