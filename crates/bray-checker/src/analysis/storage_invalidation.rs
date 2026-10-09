use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundExecutionSite, CheckedSemanticSelections, SelectedCall, SemanticSelection,
    StorageAccessId, StorageAccessPurpose, StoragePlan, StorageRelationship,
};
use bray_symbols::{ExecutionProperty, TypeData};
use std::collections::{BTreeMap, BTreeSet};

pub(super) enum StorageInvalidation {
    All,
    Accesses(Vec<StorageAccessId>),
}

impl StorageInvalidation {
    pub(super) fn invalidates(&self, access: StorageAccessId, storage: &StoragePlan) -> bool {
        match self {
            Self::All => true,
            Self::Accesses(mutations) => mutations.iter().any(|mutation| {
                storage.relationship(access, *mutation) != StorageRelationship::Disjoint
            }),
        }
    }
}

pub(super) fn invalidating_operation_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    copied_types: &BTreeSet<bray_symbols::TypeId>,
) -> BTreeMap<BoundExecutionSite, StorageInvalidation> {
    let mut accesses = BTreeMap::new();

    for plan in storage.access_plans().iter().filter(|plan| {
        if plan.purpose() == StorageAccessPurpose::ValueTransfer
            && storage.access(plan.access()).is_some_and(|access| {
                matches!(
                    request
                        .semantic_values()
                        .type_data(access.reached_type())
                        .as_ref(),
                    TypeData::Borrow { .. }
                ) || copied_types.contains(&access.reached_type())
            })
        {
            return false;
        }

        access_invalidates_refinements(plan.purpose())
    }) {
        let invalidation = accesses
            .entry(plan.occurrence())
            .or_insert_with(|| StorageInvalidation::Accesses(Vec::new()));

        if let StorageInvalidation::Accesses(changed) = invalidation {
            changed.push(plan.access());
        }
    }

    for (expression, _) in request.unit().tree().expressions() {
        if let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(expression) {
            for occurrence in [
                BoundExecutionSite::ScopedEnter(expression),
                BoundExecutionSite::ScopedExit(expression),
            ] {
                let signature = scoped.invocation(occurrence).1;

                let data = request
                    .semantic_values()
                    .type_data(signature.callable_type());

                let TypeData::Callable(callable) = data.as_ref() else {
                    panic!("scoped invocation retains its selected callable type");
                };

                if !callable
                    .phase_behaviors()
                    .invocation()
                    .execution_properties()
                    .contains(&ExecutionProperty::Pure)
                {
                    accesses.insert(occurrence, StorageInvalidation::All);
                }
            }

            continue;
        }

        let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
            continue;
        };

        if call_preserves_storage(call, copied_types) {
            continue;
        }

        // Opaque calls may carry mutation authority inside aggregates or raw pointers.
        // Without a purity certificate or a mutation summary, require fresh observations.
        accesses.insert(expression.into(), StorageInvalidation::All);
    }

    for invalidation in accesses.values_mut() {
        let StorageInvalidation::Accesses(changed) = invalidation else {
            continue;
        };

        let aliases_storage = changed.iter().any(|access| {
            storage.root_identity(*access).is_some_and(|identity| {
                matches!(
                    storage.identity(identity),
                    Some(bray_bound_tree::StorageIdentity::LocalOwned(_))
                ) && storage.identity_type(identity).is_none_or(|ty| {
                    matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        TypeData::Borrow { .. }
                    )
                })
            })
        });

        if aliases_storage {
            // Local borrows can be reassigned or returned by calls. Until their referent
            // sets are available here, a write through one may affect any observation.
            *invalidation = StorageInvalidation::All;

            continue;
        }

        changed.sort_unstable();
        changed.dedup();
    }

    accesses
}

pub(in crate::analysis) fn call_preserves_storage(
    call: &SelectedCall,
    copied_types: &BTreeSet<bray_symbols::TypeId>,
) -> bool {
    call.phase_behaviors()
        .invocation()
        .execution_properties()
        .contains(&ExecutionProperty::Pure)
        || (call.implementation_hook()
            == Some(bray_compiler_known::ImplementationHook::RawPointerRead)
            && copied_types.contains(&call.resolution().result().ty()))
        || matches!(
            call.implementation_hook(),
            Some(
                bray_compiler_known::ImplementationHook::SequenceLength
                    // Fresh allocation cannot reach or retire existing input storage.
                    | bray_compiler_known::ImplementationHook::RawAllocate
                    | bray_compiler_known::ImplementationHook::Allocate
                    | bray_compiler_known::ImplementationHook::RawBufferAllocate
                    | bray_compiler_known::ImplementationHook::SequenceIsEmpty
                    // Pointer arithmetic, layout queries and scalar conversion do not access storage.
                    | bray_compiler_known::ImplementationHook::RawPointerNull
                    | bray_compiler_known::ImplementationHook::RawPointerIsNull
                    | bray_compiler_known::ImplementationHook::RawPointerOffset
                    | bray_compiler_known::ImplementationHook::RawPointerByteOffset
                    | bray_compiler_known::ImplementationHook::MemorySizeOf
                    | bray_compiler_known::ImplementationHook::MemoryAlignOf
                    | bray_compiler_known::ImplementationHook::MemoryStrideOf
                    | bray_compiler_known::ImplementationHook::MemoryLayoutOf
                    | bray_compiler_known::ImplementationHook::MemoryTrailingLayoutOf
                    | bray_compiler_known::ImplementationHook::NumericTruncate
                    | bray_compiler_known::ImplementationHook::RawPointerReinterpret
                    | bray_compiler_known::ImplementationHook::AtomicLoad
                    | bray_compiler_known::ImplementationHook::ByteBufferRead
                    | bray_compiler_known::ImplementationHook::AddressOf
                    | bray_compiler_known::ImplementationHook::AddressOfMut
                    | bray_compiler_known::ImplementationHook::RawBufferCapacity
                    | bray_compiler_known::ImplementationHook::RawBufferInitializedCount
                    | bray_compiler_known::ImplementationHook::RawBufferPointer
                    | bray_compiler_known::ImplementationHook::RawBufferInitializedSlice
                    | bray_compiler_known::ImplementationHook::RawBufferInitializedSliceMut
                    | bray_compiler_known::ImplementationHook::RawBufferSparePointer
            )
        )
}

const fn access_invalidates_refinements(purpose: StorageAccessPurpose) -> bool {
    // Write checks the destination while evaluating its address. Assignment invalidates
    // the old value after the right-hand side and replacement cleanup have run.
    matches!(
        purpose,
        StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Assignment
    )
}
