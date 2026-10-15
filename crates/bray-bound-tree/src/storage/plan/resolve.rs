use super::contracts::{PlannedBorrowCapability, StorageAlternative};
use super::model::ResolvedStorageAccess;
use crate::{BoundUnitId, StorageAccess, StorageIdentity, StorageIdentityId, StoragePath};
use bray_base::shared_slice;

pub(super) fn resolve_accesses(
    unit: BoundUnitId,
    identities: &[StorageIdentity],
    accesses: &[StorageAccess],
    alternatives: &[StorageAlternative],
    capabilities: &[PlannedBorrowCapability],
) -> Vec<Option<ResolvedStorageAccess>> {
    let mut resolved = Vec::with_capacity(accesses.len());

    for access in accesses {
        if access.is_recovered() {
            resolved.push(None);

            continue;
        }

        let current = match access.root() {
            crate::StorageAccessRoot::Storage(root)
            | crate::StorageAccessRoot::OwnedIndirection { storage: root, .. } => {
                resolve_storage_access(unit, access, root, identities, alternatives, &resolved)
            }
            crate::StorageAccessRoot::Recovery(_) => None,
            borrow @ (crate::StorageAccessRoot::Borrow(_)
            | crate::StorageAccessRoot::BorrowedStorage { .. }) => {
                resolve_borrowed_access(unit, access, borrow, capabilities, &resolved)
            }
        };

        resolved.push(current);
    }

    resolved
}

fn resolve_borrowed_access(
    unit: BoundUnitId,
    access: &StorageAccess,
    root: crate::StorageAccessRoot,
    capabilities: &[PlannedBorrowCapability],
    resolved: &[Option<ResolvedStorageAccess>],
) -> Option<ResolvedStorageAccess> {
    let capability = root.borrow_capability()?;
    let retained = root.retained_borrow_storage();

    if capability.unit() != unit {
        return None;
    }

    let capability = capabilities.get(capability.storage_index()?)?;

    if capability.is_recovered() || capability.access().unit() != unit {
        return None;
    }

    let inherited = resolved
        .get(capability.access().storage_index()?)?
        .as_ref()?;

    let (logical_root, logical_projections, paths) = match retained {
        None => (
            inherited.logical.root,
            shared_slice(
                inherited
                    .logical
                    .projections
                    .iter()
                    .chain(access.projections())
                    .copied(),
            ),
            shared_slice(inherited.paths.iter().map(|path| StoragePath {
                root: path.root,
                projections: shared_slice(
                    path.projections.iter().chain(access.projections()).copied(),
                ),
            })),
        ),
        Some(storage) => (
            storage,
            shared_slice(access.projections().iter().copied()),
            inherited.paths.clone(),
        ),
    };

    Some(ResolvedStorageAccess {
        logical: StoragePath {
            root: logical_root,
            projections: logical_projections,
        },
        paths,
    })
}

fn resolve_storage_access(
    unit: BoundUnitId,
    access: &StorageAccess,
    root: StorageIdentityId,
    identities: &[StorageIdentity],
    alternatives: &[StorageAlternative],
    resolved: &[Option<ResolvedStorageAccess>],
) -> Option<ResolvedStorageAccess> {
    let identity = identities.get(root.storage_index()?)?;

    let paths = match identity {
        StorageIdentity::Alternative { alternative, .. } => {
            if alternative.unit() != unit {
                return None;
            }

            let alternative = alternatives.get(alternative.storage_index()?)?;

            let mut paths = Vec::new();

            for branch in alternative.accesses() {
                if branch.unit() != unit {
                    return None;
                }

                let branch = resolved.get(branch.storage_index()?)?.as_ref()?;

                paths.extend(branch.paths.iter().map(|path| StoragePath {
                    root: path.root,
                    projections: shared_slice(
                        path.projections.iter().chain(access.projections()).copied(),
                    ),
                }));
            }

            paths
        }
        _ => vec![StoragePath {
            root,
            projections: shared_slice(access.projections().iter().copied()),
        }],
    };

    Some(ResolvedStorageAccess {
        logical: StoragePath {
            root,
            projections: shared_slice(access.projections().iter().copied()),
        },
        paths: paths.into(),
    })
}
