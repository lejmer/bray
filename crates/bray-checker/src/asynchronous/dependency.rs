use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencyContract, BoundDependencyContractId, BoundDependencyGuard,
    BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
    BoundExpressionId, CheckedDependencyContracts, CheckedRefinementFacts, LivenessFacts,
    PatternPredicate, RefinementFact, RefinementFactKind, StorageAccessId, StoragePlan,
    StorageRelationship, StorageSuspensionState,
};
use bray_symbols::BorrowKind;

pub(super) fn retained_suspension_subjects(
    liveness: &LivenessFacts,
    dependencies: &CheckedDependencyContracts,
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

    retained.into_iter().collect()
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
                subjects.insert(guard_subject(guarded.guard()));
                pending.extend(guarded.requirements());
            }
        }
    }
}

pub(super) fn dependency_contract_is_satisfied(
    storage: &StoragePlan,
    refinements: &CheckedRefinementFacts,
    expression: BoundExpressionId,
    state: &StorageSuspensionState,
    contract: &BoundDependencyContract,
) -> bool {
    contract.requirements().iter().all(|requirement| {
        dependency_requirement_is_satisfied(storage, refinements, expression, state, requirement)
    })
}

fn dependency_requirement_is_satisfied(
    storage: &StoragePlan,
    refinements: &CheckedRefinementFacts,
    expression: BoundExpressionId,
    state: &StorageSuspensionState,
    requirement: &BoundDependencyRequirement,
) -> bool {
    match requirement {
        BoundDependencyRequirement::Direct { subject, kind } => {
            dependency_subject_is_satisfied(storage, state, *subject, *kind)
        }
        BoundDependencyRequirement::Guarded(guarded) => {
            !dependency_guard_may_apply(
                storage,
                refinements.facts_before(AnyBoundNodeId::Expression(expression)),
                state,
                guarded.guard(),
            ) || guarded.requirements().iter().all(|requirement| {
                dependency_requirement_is_satisfied(
                    storage,
                    refinements,
                    expression,
                    state,
                    requirement,
                )
            })
        }
    }
}

fn dependency_subject_is_satisfied(
    storage: &StoragePlan,
    state: &StorageSuspensionState,
    subject: BoundDependencySubject,
    kind: BoundDependencyRequirementKind,
) -> bool {
    match kind {
        BoundDependencyRequirementKind::StorageAlive => match subject {
            BoundDependencySubject::Storage(identity) => storage.identity(identity).is_some(),
            BoundDependencySubject::StorageAccess(access) => storage.access(access).is_some(),
            BoundDependencySubject::BorrowCapability(capability) => {
                storage.borrow_capability(capability).is_some()
            }
            BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::LifecycleObligation(_) => true,
        },
        BoundDependencyRequirementKind::StorageInitialized => {
            dependency_subject_is_initialized(storage, state, subject)
        }
        BoundDependencyRequirementKind::BorrowCapabilityActive(expected) => {
            let BoundDependencySubject::BorrowCapability(capability) = subject else {
                return false;
            };

            state.active_borrows().contains(&capability)
                && storage
                    .borrow_capability(capability)
                    .is_some_and(|capability| capability.kind() == expected)
        }
        BoundDependencyRequirementKind::ExclusiveMutationAuthority => {
            dependency_subject_has_exclusive_access(storage, state, subject)
        }
        BoundDependencyRequirementKind::ScopedCapabilityLive => {
            matches!(subject, BoundDependencySubject::ScopedCapability(_))
        }
        BoundDependencyRequirementKind::LifecycleObligationAttached(_) => {
            matches!(subject, BoundDependencySubject::LifecycleObligation(_))
        }
    }
}

fn dependency_subject_is_initialized(
    storage: &StoragePlan,
    state: &StorageSuspensionState,
    subject: BoundDependencySubject,
) -> bool {
    match subject {
        BoundDependencySubject::Storage(identity) => {
            state.initialized().contains(&identity)
                && !state
                    .moved()
                    .iter()
                    .any(|moved| storage.root_identity(*moved) == Some(identity))
        }
        BoundDependencySubject::StorageAccess(access) => {
            let Some(root) = storage.root_identity(access) else {
                return false;
            };

            state.initialized().contains(&root)
                && !state.moved().iter().any(|moved| {
                    storage.relationship(*moved, access) != StorageRelationship::Disjoint
                })
        }
        BoundDependencySubject::BorrowCapability(capability) => {
            state.active_borrows().contains(&capability)
        }
        BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::LifecycleObligation(_) => false,
    }
}

fn dependency_subject_has_exclusive_access(
    storage: &StoragePlan,
    state: &StorageSuspensionState,
    subject: BoundDependencySubject,
) -> bool {
    let (access, authorizing_borrow) = match subject {
        BoundDependencySubject::StorageAccess(access) => (access, None),
        BoundDependencySubject::BorrowCapability(capability) => {
            let Some(capability_data) = storage.borrow_capability(capability) else {
                return false;
            };

            if capability_data.kind() != BorrowKind::Mutable
                || !state.active_borrows().contains(&capability)
            {
                return false;
            }

            (capability_data.access(), Some(capability))
        }
        BoundDependencySubject::Storage(_)
        | BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::LifecycleObligation(_) => return false,
    };

    dependency_subject_is_initialized(
        storage,
        state,
        BoundDependencySubject::StorageAccess(access),
    ) && state.active_borrows().iter().copied().all(|active| {
        Some(active) == authorizing_borrow
            || storage.borrow_capability(active).is_some_and(|capability| {
                storage.relationship(capability.access(), access) == StorageRelationship::Disjoint
            })
    })
}

fn dependency_guard_may_apply(
    storage: &StoragePlan,
    facts: &[RefinementFact],
    state: &StorageSuspensionState,
    guard: BoundDependencyGuard,
) -> bool {
    match guard {
        BoundDependencyGuard::NullablePresent(access) => {
            refinement_guard_value(storage, facts, access, |kind| match kind {
                RefinementFactKind::NullablePresence { is_present, .. } => Some(is_present),
                RefinementFactKind::Pattern { predicate, .. } => match predicate {
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
                RefinementFactKind::Condition { .. }
                | RefinementFactKind::TrustBoundary(_)
                | RefinementFactKind::NormalCompletion(_) => None,
            })
            .unwrap_or(true)
        }
        BoundDependencyGuard::ActiveUnionVariant { access, variant } => {
            refinement_guard_value(storage, facts, access, |kind| match kind {
                RefinementFactKind::Pattern {
                    predicate: PatternPredicate::ActiveUnionVariant(active),
                    ..
                } => Some(active == variant),
                RefinementFactKind::Condition { .. }
                | RefinementFactKind::NullablePresence { .. }
                | RefinementFactKind::Pattern { .. }
                | RefinementFactKind::TrustBoundary(_)
                | RefinementFactKind::NormalCompletion(_) => None,
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
    facts: &[RefinementFact],
    access: StorageAccessId,
    value: impl Fn(RefinementFactKind) -> Option<bool>,
) -> Option<bool> {
    facts.iter().find_map(|fact| {
        fact.dependencies()
            .iter()
            .any(|dependency| {
                storage.relationship(*dependency, access) != StorageRelationship::Disjoint
            })
            .then(|| value(fact.kind()))
            .flatten()
    })
}

const fn guard_subject(guard: BoundDependencyGuard) -> BoundDependencySubject {
    match guard {
        BoundDependencyGuard::NullablePresent(access)
        | BoundDependencyGuard::ActiveUnionVariant { access, .. } => {
            BoundDependencySubject::StorageAccess(access)
        }
        BoundDependencyGuard::BorrowCapabilityActive(capability) => {
            BoundDependencySubject::BorrowCapability(capability)
        }
        BoundDependencyGuard::ScopedCapabilityLive(capability) => {
            BoundDependencySubject::ScopedCapability(capability)
        }
    }
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
    use bray_symbols::{BorrowKind, testing::implementation_instance};

    use super::{collect_requirement_subjects, dependency_subject_has_exclusive_access};
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

        let identity = builder
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                error_type(),
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

        let storage = builder.finish();
        let mutable_state = StorageSuspensionState::new(expression, [identity], [], [mutable]);
        let shared_state = StorageSuspensionState::new(expression, [identity], [], [shared]);

        assert!(dependency_subject_has_exclusive_access(
            &storage,
            &mutable_state,
            BoundDependencySubject::BorrowCapability(mutable)
        ));

        assert!(!dependency_subject_has_exclusive_access(
            &storage,
            &shared_state,
            BoundDependencySubject::BorrowCapability(shared)
        ));
    }
}
