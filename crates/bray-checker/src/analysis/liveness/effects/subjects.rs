use std::collections::BTreeSet;

use bray_bound_tree::{
    BorrowCapabilityId, BoundDependencyRequirement, BoundDependencySubject, StorageAccessRoot,
    StoragePlan,
};

pub(super) fn dependency_subjects(
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
) -> BTreeSet<BoundDependencySubject> {
    let mut subjects = BTreeSet::new();

    collect_dependency_subjects(requirements, storage, &mut subjects);

    subjects
}

pub(super) fn collect_dependency_subjects(
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    for requirement in requirements {
        match requirement {
            BoundDependencyRequirement::Direct { subject, .. } => {
                collect_subject(*subject, storage, subjects);
            }
            BoundDependencyRequirement::Guarded(requirement) => {
                let guard = requirement.guard().subject();

                collect_subject(guard, storage, subjects);
                collect_dependency_subjects(requirement.requirements(), storage, subjects);
            }
        }
    }
}

fn collect_subject(
    subject: BoundDependencySubject,
    storage: &StoragePlan,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    subjects.insert(subject);

    match subject {
        BoundDependencySubject::StorageAccess(access) => {
            if let Some(access) = storage.access(access) {
                subjects.extend(access_root_subjects(storage, access.root()));
            }
        }
        BoundDependencySubject::BorrowCapability(capability) => {
            subjects.extend(borrow_capability_subjects(storage, capability));
        }
        _ => {}
    }
}

pub(super) fn access_root_subjects(
    storage: &StoragePlan,
    root: StorageAccessRoot,
) -> Vec<BoundDependencySubject> {
    match root {
        StorageAccessRoot::Storage(storage)
        | StorageAccessRoot::Recovery(storage)
        | StorageAccessRoot::OwnedIndirection { storage, .. } => {
            vec![BoundDependencySubject::Storage(storage)]
        }
        StorageAccessRoot::Borrow(capability) => borrow_capability_subjects(storage, capability),
        StorageAccessRoot::BorrowedStorage {
            capability,
            storage: retained,
        } => {
            let mut subjects = vec![BoundDependencySubject::Storage(retained)];

            subjects.extend(borrow_capability_subjects(storage, capability));

            subjects
        }
    }
}

pub(super) fn borrow_capability_subjects(
    storage: &StoragePlan,
    capability: BorrowCapabilityId,
) -> Vec<BoundDependencySubject> {
    let mut subjects = Vec::new();
    let mut current = Some(capability);

    while let Some(capability) = current {
        subjects.push(BoundDependencySubject::BorrowCapability(capability));

        let planned = storage.borrow_capability(capability);

        if let Some(root) = planned.and_then(|planned| storage.root_identity(planned.access())) {
            subjects.push(BoundDependencySubject::Storage(root));
        }

        current = planned.and_then(|planned| planned.parent());
    }

    subjects
}
