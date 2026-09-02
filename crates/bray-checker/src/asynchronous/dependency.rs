use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencyContract, BoundDependencyContractId, BoundDependencyGuard,
    BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
    BoundExpressionId, CheckedDependencyContracts, CheckedRefinements, Liveness, PatternPredicate,
    Refinement, RefinementKind, StorageAccessId, StoragePlan, StorageRelationship,
    StorageSuspensionState,
};
use bray_symbols::{BorrowKind, SemanticValueStore, TypeData};

pub(super) fn retained_suspension_subjects(
    liveness: &Liveness,
    dependencies: &CheckedDependencyContracts,
    storage: &StoragePlan,
    await_expression: BoundExpressionId,
    dependency_contract: Option<BoundDependencyContractId>,
) -> Vec<BoundDependencySubject> {
    let mut retained = liveness
        .live_across_suspensions()
        .iter()
        .filter(|entry| entry.await_expression() == await_expression)
        .map(|entry| entry.subject())
        .collect::<BTreeSet<_>>();

    if let Some(contract) = dependency_contract.and_then(|id| dependencies.contract(id)) {
        for requirement in contract.requirements() {
            collect_requirement_subjects(requirement, &mut retained);
        }
    }

    retained
        .into_iter()
        .filter(|subject| retained_subject_has_storage(storage, *subject))
        .collect()
}

fn retained_subject_has_storage(storage: &StoragePlan, subject: BoundDependencySubject) -> bool {
    match subject {
        BoundDependencySubject::StorageAccess(access) => storage.root_identity(access).is_some(),
        BoundDependencySubject::Storage(_)
        | BoundDependencySubject::BorrowCapability(_)
        | BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_)
        | BoundDependencySubject::LifecycleObligation(_) => true,
    }
}

fn collect_requirement_subjects(
    requirement: &BoundDependencyRequirement,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    let mut pending = vec![requirement];

    while let Some(requirement) = pending.pop() {
        match requirement {
            BoundDependencyRequirement::Direct { subject, .. } => {
                subjects.insert(*subject);
            }
            BoundDependencyRequirement::Guarded(guarded) => {
                subjects.insert(guarded.guard().subject());
                pending.extend(guarded.requirements());
            }
        }
    }
}

pub(super) fn unsatisfied_dependency_subjects(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    refinements: &CheckedRefinements,
    expression: BoundExpressionId,
    state: &StorageSuspensionState,
    contract: &BoundDependencyContract,
) -> Result<Vec<UnsatisfiedDependency>, bray_symbols::SemanticValueStoreError> {
    let mut unsatisfied = Vec::new();

    for requirement in contract.requirements() {
        collect_unsatisfied_dependency_subjects(
            values,
            storage,
            refinements,
            expression,
            state,
            requirement,
            &mut unsatisfied,
        )?;
    }

    unsatisfied.sort_unstable();
    unsatisfied.dedup();

    Ok(unsatisfied)
}

fn collect_unsatisfied_dependency_subjects(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    refinements: &CheckedRefinements,
    expression: BoundExpressionId,
    state: &StorageSuspensionState,
    requirement: &BoundDependencyRequirement,
    unsatisfied: &mut Vec<UnsatisfiedDependency>,
) -> Result<(), bray_symbols::SemanticValueStoreError> {
    match requirement {
        BoundDependencyRequirement::Direct { subject, kind } => {
            if !dependency_subject_is_satisfied(values, storage, state, *subject, *kind)? {
                unsatisfied.push(UnsatisfiedDependency {
                    subject: *subject,
                    requirement: *kind,
                });
            }
        }
        BoundDependencyRequirement::Guarded(guarded) => {
            if !dependency_guard_may_apply(
                storage,
                refinements.refinements_before(AnyBoundNodeId::Expression(expression)),
                state,
                guarded.guard(),
            ) {
                return Ok(());
            }

            for requirement in guarded.requirements() {
                collect_unsatisfied_dependency_subjects(
                    values,
                    storage,
                    refinements,
                    expression,
                    state,
                    requirement,
                    unsatisfied,
                )?;
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct UnsatisfiedDependency {
    pub(super) subject: BoundDependencySubject,
    pub(super) requirement: BoundDependencyRequirementKind,
}

fn dependency_subject_is_satisfied(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageSuspensionState,
    subject: BoundDependencySubject,
    kind: BoundDependencyRequirementKind,
) -> Result<bool, bray_symbols::SemanticValueStoreError> {
    let satisfied = match kind {
        BoundDependencyRequirementKind::StorageAlive => match subject {
            BoundDependencySubject::Storage(identity) => state.live().contains(&identity),
            BoundDependencySubject::StorageAccess(access) => storage
                .root_identity(access)
                .is_some_and(|identity| state.live().contains(&identity)),
            BoundDependencySubject::BorrowCapability(capability) => {
                state.active_borrows().contains(&capability)
            }
            BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::LifecycleObligation(_) => false,
            BoundDependencySubject::ProductStatic(_)
            | BoundDependencySubject::ExactThreadStatic(_) => true,
        },
        BoundDependencyRequirementKind::StorageInitialized => {
            return dependency_subject_is_initialized(values, storage, state, subject);
        }
        BoundDependencyRequirementKind::BorrowCapabilityActive(expected) => {
            let BoundDependencySubject::BorrowCapability(capability) = subject else {
                return Ok(false);
            };

            state.active_borrows().contains(&capability)
                && storage
                    .borrow_capability(capability)
                    .is_some_and(|capability| capability.kind() == expected)
        }
        BoundDependencyRequirementKind::ExclusiveMutationAuthority => {
            return dependency_subject_has_exclusive_access(values, storage, state, subject);
        }
        BoundDependencyRequirementKind::ScopedCapabilityLive => false,
        BoundDependencyRequirementKind::LifecycleObligationAttached(_) => false,
    };

    Ok(satisfied)
}

fn dependency_subject_is_initialized(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageSuspensionState,
    subject: BoundDependencySubject,
) -> Result<bool, bray_symbols::SemanticValueStoreError> {
    match subject {
        BoundDependencySubject::Storage(identity) => {
            Ok(state.initialized().contains(&identity)
                && !state
                    .moved()
                    .iter()
                    .any(|moved| storage.root_identity(*moved) == Some(identity)))
        }
        BoundDependencySubject::StorageAccess(access) => {
            let Some(root) = storage.root_identity(access) else {
                return Ok(false);
            };

            if !state.initialized().contains(&root) {
                return Ok(false);
            }

            for moved in state.moved() {
                if !access_is_borrow_value(values, storage, *moved)?
                    && storage.relationship(*moved, access) != StorageRelationship::Disjoint
                {
                    return Ok(false);
                }
            }

            Ok(true)
        }
        BoundDependencySubject::BorrowCapability(capability) => {
            Ok(state.active_borrows().contains(&capability))
        }
        BoundDependencySubject::ProductStatic(_) | BoundDependencySubject::ExactThreadStatic(_) => {
            Ok(true)
        }
        BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::LifecycleObligation(_) => Ok(false),
    }
}

fn dependency_subject_has_exclusive_access(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    state: &StorageSuspensionState,
    subject: BoundDependencySubject,
) -> Result<bool, bray_symbols::SemanticValueStoreError> {
    let (access, authorizing_borrow) = match subject {
        BoundDependencySubject::StorageAccess(access) => {
            let authorizing_borrow = storage.access(access).and_then(|access| {
                let Some(capability) = access.root().borrow_capability() else {
                    return None;
                };

                Some(capability)
            });

            (access, authorizing_borrow)
        }
        BoundDependencySubject::BorrowCapability(capability) => {
            let Some(capability_data) = storage.borrow_capability(capability) else {
                return Ok(false);
            };

            if capability_data.kind() != BorrowKind::Mutable
                || !state.active_borrows().contains(&capability)
            {
                return Ok(false);
            }

            (capability_data.access(), Some(capability))
        }
        BoundDependencySubject::Storage(_)
        | BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_)
        | BoundDependencySubject::LifecycleObligation(_) => return Ok(false),
    };

    let authorizing_borrows = authorizing_borrow
        .map(|capability| borrow_chain(storage, capability))
        .unwrap_or_default();

    Ok(dependency_subject_is_initialized(
        values,
        storage,
        state,
        BoundDependencySubject::StorageAccess(access),
    )? && state.active_borrows().iter().copied().all(|active| {
        authorizing_borrows.contains(&active)
            || storage.borrow_capability(active).is_some_and(|capability| {
                storage.relationship(capability.access(), access) == StorageRelationship::Disjoint
            })
    }))
}

fn borrow_chain(
    storage: &StoragePlan,
    capability: bray_bound_tree::BorrowCapabilityId,
) -> BTreeSet<bray_bound_tree::BorrowCapabilityId> {
    let mut chain = BTreeSet::new();
    let mut current = Some(capability);

    while let Some(capability) = current {
        chain.insert(capability);

        current = storage
            .borrow_capability(capability)
            .and_then(|capability| capability.parent());
    }

    chain
}

fn access_is_borrow_value(
    values: &SemanticValueStore,
    storage: &StoragePlan,
    access: StorageAccessId,
) -> Result<bool, bray_symbols::SemanticValueStoreError> {
    let Some(access) = storage.access(access) else {
        return Ok(false);
    };

    let data = values.type_data(access.reached_type())?;

    Ok(matches!(data.as_ref(), TypeData::Borrow { .. }))
}

fn dependency_guard_may_apply(
    storage: &StoragePlan,
    refinements: &[Refinement],
    state: &StorageSuspensionState,
    guard: BoundDependencyGuard,
) -> bool {
    match guard {
        BoundDependencyGuard::NullablePresent(access) => {
            refinement_guard_value(storage, refinements, access, |kind| match kind {
                RefinementKind::NullablePresence { is_present, .. } => Some(is_present),
                RefinementKind::Pattern { predicate, .. } => match predicate {
                    PatternPredicate::NullableAbsent => Some(false),
                    PatternPredicate::NullablePresent => Some(true),
                    PatternPredicate::Literal(_)
                    | PatternPredicate::Constant(_)
                    | PatternPredicate::ActiveUnionVariant(_)
                    | PatternPredicate::ProductShape(_)
                    | PatternPredicate::TupleShape(_)
                    | PatternPredicate::ArrayShape(_)
                    | PatternPredicate::OwnedTarget => None,
                },
                RefinementKind::Condition { .. }
                | RefinementKind::TrustBoundary(_)
                | RefinementKind::NormalCompletion(_) => None,
            })
            .unwrap_or(true)
        }
        BoundDependencyGuard::ActiveUnionVariant { access, variant } => {
            refinement_guard_value(storage, refinements, access, |kind| match kind {
                RefinementKind::Pattern {
                    predicate: PatternPredicate::ActiveUnionVariant(active),
                    ..
                } => Some(active == variant),
                RefinementKind::Condition { .. }
                | RefinementKind::NullablePresence { .. }
                | RefinementKind::Pattern { .. }
                | RefinementKind::TrustBoundary(_)
                | RefinementKind::NormalCompletion(_) => None,
            })
            .unwrap_or(true)
        }
        BoundDependencyGuard::BorrowCapabilityActive(capability) => {
            state.active_borrows().contains(&capability)
        }
        BoundDependencyGuard::ScopedCapabilityLive(_) => true,
    }
}

fn refinement_guard_value(
    storage: &StoragePlan,
    refinements: &[Refinement],
    access: StorageAccessId,
    value: impl Fn(RefinementKind) -> Option<bool>,
) -> Option<bool> {
    refinements.iter().find_map(|refinement| {
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
    use std::collections::BTreeSet;

    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundDependencyRequirement, BoundDependencyRequirementKind,
        BoundDependencySubject, BoundErrorExpression, BoundExpression, BoundNodeOrigin,
        BoundTreeBuilder, BoundUnitId, BoundUnitKind, PlannedBorrowCapability, StorageAccess,
        StorageAccessRoot, StorageIdentity, StoragePlanBuilder, StorageSuspensionState,
    };
    use bray_symbols::{BorrowKind, TypeData, testing::implementation_instance};

    use super::{
        collect_requirement_subjects, dependency_subject_has_exclusive_access,
        retained_subject_has_storage,
    };
    use crate::test_support::{error_type, push_expression, semantic_values, test_source_origins};

    #[test]
    fn deferred_contracts_retain_non_storage_frame_subjects() {
        let witness = implementation_instance(semantic_values(), 72);
        let subject = BoundDependencySubject::ImplementationWitness(witness);

        let requirement = BoundDependencyRequirement::direct(
            subject,
            BoundDependencyRequirementKind::StorageAlive,
        );

        let mut subjects = BTreeSet::new();

        collect_requirement_subjects(&requirement, &mut subjects);

        assert_eq!(subjects, BTreeSet::from([subject]));
    }

    #[test]
    fn frame_retention_requires_a_resolved_storage_root_for_access_subjects() {
        let unit = BoundUnitId::new(74);
        let source = test_source_origins()[0].source_anchor();
        let mut builder = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let identity = builder
            .push_identity(StorageIdentity::CompilerCreated(BoundNodeOrigin::source(
                source,
            )))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Recovery(identity),
                [],
                error_type(),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));

        let storage = builder.finish();

        assert!(!retained_subject_has_storage(
            &storage,
            BoundDependencySubject::StorageAccess(access),
        ));

        assert!(retained_subject_has_storage(
            &storage,
            BoundDependencySubject::Storage(identity),
        ));
    }

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
                BorrowCapabilityOrigin::Expression(expression),
                BorrowKind::Mutable,
                access,
                None,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test mutable borrow must build: {error:?}"));

        let shared = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Expression(expression),
                BorrowKind::Shared,
                access,
                None,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test shared borrow must build: {error:?}"));

        let reborrow = builder
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Expression(expression),
                BorrowKind::Mutable,
                access,
                Some(mutable),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test mutable reborrow must build: {error:?}"));

        let storage = builder.finish();

        let mutable_state =
            StorageSuspensionState::new(expression, [identity], [identity], [], [mutable]);

        let shared_state =
            StorageSuspensionState::new(expression, [identity], [identity], [], [shared]);

        let moved_borrow_state =
            StorageSuspensionState::new(expression, [identity], [identity], [access], [mutable]);

        let reborrow_state = StorageSuspensionState::new(
            expression,
            [identity],
            [identity],
            [access],
            [mutable, reborrow],
        );

        assert!(dependency_subject_has_exclusive_access(
            semantic_values(),
            &storage,
            &mutable_state,
            BoundDependencySubject::BorrowCapability(mutable)
        )
        .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}")));

        assert!(!dependency_subject_has_exclusive_access(
            semantic_values(),
            &storage,
            &shared_state,
            BoundDependencySubject::BorrowCapability(shared)
        )
        .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}")));

        assert!(dependency_subject_has_exclusive_access(
            semantic_values(),
            &storage,
            &moved_borrow_state,
            BoundDependencySubject::BorrowCapability(mutable)
        )
        .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}")));

        assert!(dependency_subject_has_exclusive_access(
            semantic_values(),
            &storage,
            &reborrow_state,
            BoundDependencySubject::BorrowCapability(reborrow)
        )
        .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}")));
    }
}
