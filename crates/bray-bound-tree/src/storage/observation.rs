/// Identity of a checked operation that creates a new contents observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageMutation {
    /// One explicit or scoped call.
    Operation(crate::BoundExecutionSite),
    /// Cancellation broadcast for one owned task-bearing value.
    Cancellation {
        /// Source boundary containing the broadcast.
        occurrence: crate::BoundExecutionSite,
        /// Owner receiving cancellation.
        access: crate::StorageAccessId,
    },
    /// One opaque represented component within an ordered cleanup.
    Cleanup {
        /// Source boundary containing the cleanup.
        occurrence: crate::BoundExecutionSite,
        /// Owner being cleaned up.
        access: crate::StorageAccessId,
        /// Prepared component within that owner.
        part: usize,
    },
    /// One lifecycle invocation within an ordered cleanup.
    Lifecycle {
        /// Source boundary containing the invocation.
        occurrence: crate::BoundExecutionSite,
        /// Owner being cleaned up.
        access: crate::StorageAccessId,
        /// Prepared component within that owner.
        part: usize,
        /// Selected lifecycle phase.
        slot: bray_symbols::TypeAssociatedLifecycleSlot,
    },
}

impl StorageMutation {
    /// Returns the enclosing source occurrence.
    pub const fn occurrence(self) -> crate::BoundExecutionSite {
        match self {
            Self::Operation(occurrence)
            | Self::Lifecycle { occurrence, .. }
            | Self::Cleanup { occurrence, .. }
            | Self::Cancellation { occurrence, .. } => occurrence,
        }
    }
}

/// Checked facts at a reachable normal completion.
/// Value observations and required storage lifetimes are distinct relationships.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct StorageCompletion {
    references: std::collections::BTreeMap<crate::StoragePath, crate::StoragePath>,
    borrow_origins: std::collections::BTreeMap<
        crate::BoundExecutionSite,
        std::collections::BTreeSet<crate::StoragePath>,
    >,
}

impl StorageCompletion {
    /// Creates a completion from exact value observations and all possible local borrowed owners.
    pub fn new(
        references: std::collections::BTreeMap<crate::StoragePath, crate::StoragePath>,
        borrow_origins: std::collections::BTreeMap<
            crate::BoundExecutionSite,
            std::collections::BTreeSet<crate::StoragePath>,
        >,
    ) -> Self {
        Self {
            references,
            borrow_origins,
        }
    }

    /// Returns exact current aliases used to observe mutated referents.
    pub fn references(
        &self,
    ) -> &std::collections::BTreeMap<crate::StoragePath, crate::StoragePath> {
        &self.references
    }

    /// Returns the possible invocation-local owners retained through each borrowed input.
    /// An empty set records that checked incoming borrows need no local owner. Their
    /// ordinary dependency contract still retains every caller-owned lifetime.
    pub fn borrow_origins(
        &self,
    ) -> &std::collections::BTreeMap<
        crate::BoundExecutionSite,
        std::collections::BTreeSet<crate::StoragePath>,
    > {
        &self.borrow_origins
    }

    /// Merges alternative normal completions. Exact aliases must agree, and lifetimes accumulate.
    /// A missing origin mapping remains unknown, never an empty lifetime requirement.
    pub fn merge(&mut self, incoming: &Self) {
        self.references
            .retain(|path, target| incoming.references.get(path) == Some(target));

        self.borrow_origins.retain(|path, targets| {
            let Some(other) = incoming.borrow_origins.get(path) else {
                return false;
            };

            // Each completion retains its own finite set of shared immutable storage paths.
            targets.extend(other.iter().cloned());

            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::StorageCompletion;
    use std::collections::{BTreeMap, BTreeSet};
    #[test]
    fn completion_merge_intersects_exact_aliases_and_unions_required_origins() {
        let unit = crate::BoundUnitId::new(3);

        let path = |slot| {
            crate::StoragePath::root(crate::StorageIdentityId::from_storage_slot(unit, slot))
        };

        let argument = crate::BoundExpressionId::from_slot(unit, 0);

        let mut left = StorageCompletion::new(
            BTreeMap::from([(path(0), path(1))]),
            BTreeMap::from([(argument.into(), BTreeSet::from([path(1)]))]),
        );

        let right = StorageCompletion::new(
            BTreeMap::from([(path(0), path(2))]),
            BTreeMap::from([(argument.into(), BTreeSet::from([path(2)]))]),
        );

        left.merge(&right);
        assert!(left.references().is_empty());

        assert_eq!(
            left.borrow_origins()[&argument.into()],
            BTreeSet::from([path(1), path(2)])
        );

        left.merge(&StorageCompletion::default());
        assert!(!left.borrow_origins().contains_key(&argument.into()));
    }
}
