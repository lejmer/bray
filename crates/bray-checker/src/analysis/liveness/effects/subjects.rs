use std::collections::BTreeSet;

use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BorrowCapabilityId, BoundDependencyRequirement, BoundDependencySubject, StorageAccessRoot,
    StoragePlan,
};

pub(in crate::analysis) fn dependency_subjects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
    inputs: &crate::dependency::ValueInputs,
    selections: &bray_bound_tree::CheckedSemanticSelections,
) -> BTreeSet<BoundDependencySubject> {
    let mut subjects = BTreeSet::new();

    collect_dependency_subjects(request, requirements, storage, inputs, selections, &mut subjects);

    subjects
}

pub(super) fn collect_dependency_subjects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
    inputs: &crate::dependency::ValueInputs,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    for requirement in requirements {
        match requirement {
            BoundDependencyRequirement::Direct { subject, .. } => {
                collect_subject(*subject, storage, subjects);
            }
            BoundDependencyRequirement::Value { source, path, ty } => {
                // A resolved component or address copy carries the source value's
                // dependencies. The reading loan is an invocation use. Resolve
                // carried loans in the ownership fixed point with the full requirement.
                if crate::representation::reference_kind(request, *ty)
                    != Some(bray_symbols::DependencyReferenceKind::Address)
                    && inputs.copied_value_source(request.semantic_values(), storage, selections, *source, path).is_none()
                {
                    collect_subject(
                        BoundDependencySubject::StorageAccess(*source),
                        storage,
                        subjects,
                    );
                }
            }
            BoundDependencyRequirement::Guarded(requirement) => {
                let guard = requirement.guard().subject();

                collect_subject(guard, storage, subjects);
                collect_dependency_subjects(request, requirement.requirements(), storage, inputs, selections, subjects);
            }
        }
    }
}

fn collect_subject(
    subject: BoundDependencySubject,
    storage: &StoragePlan,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    if matches!(
        subject,
        BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::ImplementationRequirement(_)
    ) {
        return;
    }

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
