use std::collections::BTreeSet;

use super::{Liveness, StoragePlan};
use crate::BoundDependencySubject;

impl Liveness {
    /// Returns the complete ordered subject set retained across one suspension.
    pub fn retained_suspension_subjects(
        &self,
        storage: &StoragePlan,
        occurrence: impl Into<crate::BoundExecutionSite>,
        state: &super::StorageSuspensionState,
    ) -> Vec<BoundDependencySubject> {
        let occurrence = occurrence.into();

        let mut retained = self
            .live_across_suspensions()
            .iter()
            .filter(|entry| entry.occurrence() == occurrence)
            .map(|entry| entry.subject())
            .collect::<BTreeSet<_>>();

        retained.extend(state.cleanup_retention().iter().chain(state.execution_retention()).copied());

        retained
            .into_iter()
            .filter(|subject| retained_subject_has_storage(storage, *subject))
            .collect()
    }
}

fn retained_subject_has_storage(storage: &StoragePlan, subject: BoundDependencySubject) -> bool {
    match subject {
        BoundDependencySubject::StorageAccess(access) => storage.root_identity(access).is_some(),
        BoundDependencySubject::Storage(_)
        | BoundDependencySubject::BorrowCapability(_)
        | BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ImplementationRequirement(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_)
        | BoundDependencySubject::LifecycleObligation(_) => true,
    }
}
