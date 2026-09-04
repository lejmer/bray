use std::collections::BTreeSet;

use super::{Liveness, StoragePlan};
use crate::{
    BoundDependencyContractId, BoundDependencySubject, BoundExpressionId,
    CheckedDependencyContracts,
};

impl Liveness {
    /// Returns the complete ordered subject set retained across one suspension.
    pub fn retained_suspension_subjects(
        &self,
        dependencies: &CheckedDependencyContracts,
        storage: &StoragePlan,
        expression: BoundExpressionId,
        dependency_contract: Option<BoundDependencyContractId>,
    ) -> Vec<BoundDependencySubject> {
        let mut retained = self
            .live_across_suspensions()
            .iter()
            .filter(|entry| entry.await_expression() == expression)
            .map(|entry| entry.subject())
            .collect::<BTreeSet<_>>();

        if let Some(contract) = dependency_contract.and_then(|id| dependencies.contract(id)) {
            retained.extend(contract.required_subjects());
        }

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
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_)
        | BoundDependencySubject::LifecycleObligation(_) => true,
    }
}
