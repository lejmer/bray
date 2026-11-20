use bray_bound_tree::{
    BoundDependencyContract, BoundDependencyGuard, BoundDependencyRequirement,
    BoundDependencyRequirementKind, BoundDependencySubject,
    PatternPredicate, Refinement, RefinementKind, StorageAccessId, StoragePlan,
    StorageOrigin, StorageRelationship, StorageTarget,
};
use bray_symbols::{BorrowKind, SemanticValueStore, TypeData};

use super::access::{borrow_chain_from, overlapping_borrow_conflict};
use crate::analysis::storage_flow::model::StorageFlowState;

pub(super) fn collect_unsatisfied_dependencies<'refinement>(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    refinements: impl Iterator<Item = &'refinement Refinement> + Clone,
    state: &StorageFlowState,
    contract: &BoundDependencyContract,
    unsatisfied: &mut Vec<UnsatisfiedDependency>,
) {
    for requirement in contract.requirements() {
        collect_unsatisfied_dependency_subjects(
            values,
            storage,
            refinements.clone(),
            state,
            requirement,
            unsatisfied,
        );
    }
}

fn collect_unsatisfied_dependency_subjects<'refinement>(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    refinements: impl Iterator<Item = &'refinement Refinement> + Clone,
    state: &StorageFlowState,
    requirement: &BoundDependencyRequirement,
    unsatisfied: &mut Vec<UnsatisfiedDependency>,
) {
    match requirement {
        BoundDependencyRequirement::Direct { subject, kind } => {
            if !dependency_subject_is_satisfied(values, storage, state, *subject, *kind) {
                unsatisfied.push((*subject, *kind));
            }
        }
        BoundDependencyRequirement::Value { source, .. } => {
            let subject = BoundDependencySubject::StorageAccess(*source);
            let kind = BoundDependencyRequirementKind::ValueDependencies;

            if !dependency_subject_is_satisfied(values, storage, state, subject, kind) {
                unsatisfied.push((subject, kind));
            }
        }
        BoundDependencyRequirement::Guarded(guarded) => {
            if !dependency_guard_may_apply(
                storage,
                refinements.clone(),
                state,
                guarded.guard(),
            ) {
                return;
            }

            for requirement in guarded.requirements() {
                collect_unsatisfied_dependency_subjects(
                    values,
                    storage,
                    refinements.clone(),
                    state,
                    requirement,
                    unsatisfied,
                );
            }
        }
    }
}

type UnsatisfiedDependency = (BoundDependencySubject, BoundDependencyRequirementKind);

pub(super) fn reached_dependency_is_satisfied(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageFlowState,
    target: &StorageTarget,
    ty: bray_symbols::TypeId,
    loan: Option<bray_bound_tree::BorrowCapabilityId>,
    kind: BoundDependencyRequirementKind,
) -> bool {
    if !target.is_exact() {
        return false;
    }

    let Some(physical) = state.memory.reached_dependency_storage(target, ty) else {
        return false;
    };

    let (alive, initialized) = if matches!(target.origin, StorageOrigin::Static { .. }) {
        (true, true)
    } else if matches!(target.origin, StorageOrigin::Storage(_) |
        StorageOrigin::Entry { kind: bray_symbols::DependencyReferenceKind::Borrow, .. })
        && let Some(path) = target.inline_state_path()
    {
        let moved = state.moved.keys().any(|moved| {
            !moved.access_id(storage).is_some_and(|source| access_is_borrow_value(values, storage, source))
                && storage.path_relationship(&path, moved) != StorageRelationship::Disjoint
        });

        (state.live.contains(&path.root), state.initialized.contains(&path.root) && !moved)
    } else {
        physical
    };

    match kind {
        BoundDependencyRequirementKind::StorageAlive => alive,
        BoundDependencyRequirementKind::StorageInitialized | BoundDependencyRequirementKind::ValueDependencies => alive && initialized,
        BoundDependencyRequirementKind::BorrowCapabilityActive(_) => {
            alive && loan.is_some_and(|loan| {
                    dependency_subject_is_satisfied(values, storage, state, BoundDependencySubject::BorrowCapability(loan), kind)
                })
        }
        BoundDependencyRequirementKind::ExclusiveMutationAuthority => {
            alive && initialized && loan.is_some_and(|loan| {
                dependency_subject_has_exclusive_access(
                    values, storage, state, BoundDependencySubject::BorrowCapability(loan), Some(target),
                )
            })
        }
        BoundDependencyRequirementKind::ScopedCapabilityLive
        | BoundDependencyRequirementKind::LifecycleObligationAttached(_) => false,
    }
}

pub(super) fn dependency_subject_is_satisfied(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageFlowState,
    subject: BoundDependencySubject,
    kind: BoundDependencyRequirementKind,
) -> bool {
    match kind {
        BoundDependencyRequirementKind::StorageAlive => match subject {
            BoundDependencySubject::Storage(identity) => state.live.contains(&identity),
            BoundDependencySubject::StorageAccess(access) => storage
                .root_identity(access)
                .is_some_and(|identity| state.live.contains(&identity)),
            BoundDependencySubject::BorrowCapability(capability) => {
                state.definitely_active_borrows.contains(&capability)
            }
            BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::ImplementationRequirement(_)
            | BoundDependencySubject::LifecycleObligation(_) => false,
            BoundDependencySubject::ProductStatic(_)
            | BoundDependencySubject::ExactThreadStatic(_) => true,
        },
        BoundDependencyRequirementKind::StorageInitialized
        | BoundDependencyRequirementKind::ValueDependencies => {
            dependency_subject_is_initialized(values, storage, state, subject)
        }
        BoundDependencyRequirementKind::BorrowCapabilityActive(expected) => {
            let BoundDependencySubject::BorrowCapability(capability) = subject else {
                return false;
            };

            state.definitely_active_borrows.contains(&capability)
                && storage
                    .borrow_capability(capability)
                    .is_some_and(|capability| capability.kind() == expected)
        }
        BoundDependencyRequirementKind::ExclusiveMutationAuthority => {
            dependency_subject_has_exclusive_access(values, storage, state, subject, None)
        }
        BoundDependencyRequirementKind::ScopedCapabilityLive => false,
        BoundDependencyRequirementKind::LifecycleObligationAttached(_) => false,
    }
}

fn dependency_subject_is_initialized(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageFlowState,
    subject: BoundDependencySubject,
) -> bool {
    match subject {
        BoundDependencySubject::Storage(identity) => {
            state.initialized.contains(&identity) && !state.moved.keys().any(|moved| moved.root == identity)
        }
        BoundDependencySubject::StorageAccess(access) => {
            let Some(root) = storage.root_identity(access) else {
                return false;
            };

            if !state.initialized.contains(&root) {
                return false;
            }

            for moved in state.moved.keys() {
                if !moved
                    .access_id(storage)
                    .is_some_and(|source| access_is_borrow_value(values, storage, source))
                    && storage.relationship_to_path(access, moved) != StorageRelationship::Disjoint
                {
                    return false;
                }
            }

            true
        }
        BoundDependencySubject::BorrowCapability(capability) => {
            state.definitely_active_borrows.contains(&capability)
        }
        BoundDependencySubject::ProductStatic(_) | BoundDependencySubject::ExactThreadStatic(_) => {
            true
        }
        BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ImplementationRequirement(_)
        | BoundDependencySubject::LifecycleObligation(_) => false,
    }
}

fn dependency_subject_has_exclusive_access(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageFlowState,
    subject: BoundDependencySubject,
    reached: Option<&StorageTarget>,
) -> bool {
    let (access, authorizing_borrow) = match subject {
        BoundDependencySubject::StorageAccess(access) => {
            let authorizing_borrow = storage.access(access).and_then(|access| access.root().borrow_capability());

            (access, authorizing_borrow)
        }
        BoundDependencySubject::BorrowCapability(capability) => {
            let Some(capability_data) = storage.borrow_capability(capability) else {
                return false;
            };

            (capability_data.access(), Some(capability))
        }
        BoundDependencySubject::Storage(_)
        | BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ImplementationRequirement(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_)
        | BoundDependencySubject::LifecycleObligation(_) => return false,
    };

    let Some(authorizing_borrows) = borrow_chain_from(storage, Some(&state.memory), authorizing_borrow) else {
        return false;
    };

    if !authorizing_borrows.iter().all(|&borrow| {
        state.definitely_active_borrows.contains(&borrow)
            && storage.borrow_capability(borrow).is_some_and(|loan| loan.kind() == BorrowKind::Mutable)
    }) {
        return false;
    }

    let loan_address = match (reached, subject) {
        (None, BoundDependencySubject::BorrowCapability(loan)) =>
            state.memory.borrow_address(loan).filter(|address| !address.is_unknown() && !address.is_displaced()),
        _ => None,
    };

    let targets = reached.into_iter().chain(loan_address.into_iter().flat_map(|address| &address.targets));
    let located = reached.is_some() || loan_address.is_some_and(bray_bound_tree::StorageAddressValue::has_values);

    (reached.is_some() || dependency_subject_is_initialized(values, storage, state, BoundDependencySubject::StorageAccess(access)))
        && overlapping_borrow_conflict(storage, state, access, Some((targets, located)),
            BorrowKind::Mutable, &authorizing_borrows).is_none()
}

fn access_is_borrow_value(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    access: StorageAccessId,
) -> bool {
    let Some(access) = storage.access(access) else {
        return false;
    };

    let data = values.type_data(access.reached_type());

    matches!(data.as_ref(), TypeData::Borrow { .. })
}

fn dependency_guard_may_apply<'refinement>(
    storage: &StoragePlan,
    refinements: impl Iterator<Item = &'refinement Refinement>,
    state: &StorageFlowState,
    guard: BoundDependencyGuard,
) -> bool {
    match guard {
        BoundDependencyGuard::NullablePresent(access) => {
            refinement_guard_value(storage, refinements, access, |kind| match kind {
                RefinementKind::NullablePresence { is_present, .. } => Some(is_present),
                RefinementKind::Pattern {
                    predicate, value, ..
                } => match predicate {
                    PatternPredicate::NullableAbsent => Some(!value),
                    PatternPredicate::NullablePresent => Some(value),
                    PatternPredicate::Literal(_)
                    | PatternPredicate::Constant(_)
                    | PatternPredicate::ActiveUnionVariant(_)
                    | PatternPredicate::ProductShape(_)
                    | PatternPredicate::TupleShape(_)
                    | PatternPredicate::ArrayShape(_)
                    | PatternPredicate::OwnedTarget => None,
                },
                RefinementKind::Condition { .. } | RefinementKind::TrustBoundary(_) => None,
            })
            .unwrap_or(true)
        }
        BoundDependencyGuard::ActiveUnionVariant { access, variant } => {
            refinement_guard_value(storage, refinements, access, |kind| match kind {
                RefinementKind::Pattern {
                    predicate: PatternPredicate::ActiveUnionVariant(active),
                    value,
                    ..
                } => {
                    if value {
                        Some(active == variant)
                    } else if active == variant {
                        Some(false)
                    } else {
                        None
                    }
                }
                RefinementKind::Condition { .. }
                | RefinementKind::NullablePresence { .. }
                | RefinementKind::Pattern { .. }
                | RefinementKind::TrustBoundary(_) => None,
            })
            .unwrap_or(true)
        }
        BoundDependencyGuard::BorrowCapabilityActive(capability) => {
            state.active_borrows.contains(&capability)
        }
        BoundDependencyGuard::ScopedCapabilityLive(_) => true,
    }
}

fn refinement_guard_value<'refinement>(
    storage: &StoragePlan,
    mut refinements: impl Iterator<Item = &'refinement Refinement>,
    access: StorageAccessId,
    value: impl Fn(RefinementKind) -> Option<bool>,
) -> Option<bool> {
    refinements.find_map(|refinement| {
        if let RefinementKind::Pattern {
            access: subject, ..
        } = refinement.kind()
        {
            return (storage.relationship(subject, access) == StorageRelationship::Identical)
                .then(|| value(refinement.kind()))
                .flatten();
        }

        refinement
            .dependencies()
            .iter()
            .any(|dependency| {
                storage.relationship(*dependency, access) != StorageRelationship::Disjoint
            })
            .then(|| value(refinement.kind()))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundDependencySubject, BoundErrorExpression, BoundExpression,
        BoundNodeOrigin, BoundTreeBuilder, BoundUnitId, BoundUnitKind, PlannedBorrowCapability,
        StorageAccess, StorageAccessRoot, StorageIdentity, StoragePlanBuilder,
    };
    use bray_symbols::{BorrowKind, TypeData};

    use super::{StorageFlowState, dependency_subject_has_exclusive_access};
    use crate::test_support::{error_type, push_expression, semantic_values, test_source_origins};

    #[test]
    fn mutable_borrows_supply_their_own_exclusive_authority() {
        let unit = BoundUnitId::new(73);
        let source = test_source_origins()[0].source_anchor();
        let origin = BoundNodeOrigin::source(source);
        let mut tree = BoundTreeBuilder::new(unit);

        let expression = push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        );

        let mut builder = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let borrow_type = semantic_values()
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: error_type(),
            })
            .unwrap_or_else(|error| panic!("test borrow type must intern: {error:?}"));

        let identity = builder
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                borrow_type,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));

        let mutable = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Operation((expression).into()),
                BorrowKind::Mutable,
                access,
                None,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test mutable borrow must build: {error:?}"));

        let shared = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Operation((expression).into()),
                BorrowKind::Shared,
                access,
                Some(mutable),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test shared borrow must build: {error:?}"));

        let reborrow = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Operation((expression).into()),
                BorrowKind::Mutable,
                access,
                Some(mutable),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test mutable reborrow must build: {error:?}"));

        let storage = builder.finish();

        let state = |active: &[bray_bound_tree::BorrowCapabilityId], moved| {
            let mut state = StorageFlowState::default();

            state.live.insert(identity);
            state.initialized.insert(identity);
            state.active_borrows.extend(active.iter().copied());
            state.definitely_active_borrows.extend(active.iter().copied());

            if moved {
                let path = storage.logical_path(access).expect("fixture retains its source path");

                state.moved.insert(path.clone(), Default::default());
            }

            state
        };

        let mutable_state = state(&[mutable], false);
        let shared_state = state(&[mutable, shared], false);
        let moved_borrow_state = state(&[mutable], true);
        let reborrow_state = state(&[mutable, reborrow], true);
        let mut possibly_shared = state(&[mutable], false);

        possibly_shared.active_borrows.insert(shared);

        let has_exclusive_access = |state, capability| {
            dependency_subject_has_exclusive_access(
                semantic_values(),
                &storage,
                state,
                BoundDependencySubject::BorrowCapability(capability),
                None,
            )
        };

        assert!(has_exclusive_access(&mutable_state, mutable));
        assert!(!has_exclusive_access(&shared_state, shared));
        assert!(has_exclusive_access(&moved_borrow_state, mutable));
        assert!(has_exclusive_access(&reborrow_state, reborrow));
        assert!(!has_exclusive_access(&possibly_shared, mutable));
    }
}
