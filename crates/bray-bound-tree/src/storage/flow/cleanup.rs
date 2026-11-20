use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{
    AnalysisOperationId, BoundExecutionSite, StorageAccessId, StorageCleanupAction,
    StorageCleanupActionKind, StorageExitPoint, StorageFlow, StorageInvalidation, StorageMutation,
    StoragePath, StorageTarget,
};

/// Why an evaluated place has no usable value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageAbsence {
    /// A previous operation transferred its value.
    Moved,
    /// Its nullable or union payload is inactive.
    NoPayload,
}

/// Storage admitted to a cleanup phase or a resumed suffix.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageCleanupEntry {
    orders: Arc<[Arc<[StorageAccessId]>]>,
    absent: Arc<BTreeMap<StoragePath, StorageAbsence>>,
    replacement: Option<StorageTarget>,
}

impl StorageCleanupEntry {
    /// Retains the reached orders and the incoming absence and address facts.
    pub fn new(
        orders: Arc<[Arc<[StorageAccessId]>]>,
        absent: Arc<BTreeMap<StoragePath, StorageAbsence>>,
        replacement: Option<StorageTarget>,
    ) -> Self {
        Self { orders, absent, replacement }
    }

    /// The checked order alternatives at entry to this phase.
    pub fn orders(&self) -> &[Arc<[StorageAccessId]>] {
        &self.orders
    }

    /// The owners or payloads absent at every reached entry.
    pub fn absent(&self) -> impl Iterator<Item = &StoragePath> + Clone {
        self.absent.keys()
    }

    /// The reached address of the displaced owner, when established.
    pub fn replacement(&self) -> Option<&StorageTarget> {
        self.replacement.as_ref()
    }
}

/// The selected schedule remaining immediately after one reached cleanup action.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCleanupRemainder {
    /// The lexical exit that owns the remaining duties. Replacement resumes at the same operation.
    pub point: Option<StorageExitPoint>,
    /// The current owner and the remaining owners, in their selected order.
    pub accesses: Arc<[StorageAccessId]>,
    /// The failed action, or the first part of a repeated element region.
    pub after: StorageCleanupAction,
}

impl StorageCleanupRemainder {
    /// Retains the current owner and the part of the reached order still to run.
    pub fn at(point: Option<StorageExitPoint>, order: &[StorageAccessId], after: StorageCleanupAction) -> Self {
        let current = order.iter().position(|access| *access == after.access)
            .expect("cleanup continuation retains a reached owner");

        Self { point, accesses: order[current..].into(), after }
    }
}

/// The schedule selected by storage flow for lowering one lifecycle phase.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageCleanupSchedule {
    /// One order covers the reached paths.
    Fixed(Arc<[StorageAccessId]>),
    /// The runtime selects the first present owner whose consumers have finished.
    Conditional {
        /// Reverse declaration priority before dependency ordering.
        priority: Arc<[StorageAccessId]>,
        /// Consumers that must finish before each owner in priority order.
        consumers: Arc<[Arc<[StorageAccessId]>]>,
    },
}

impl StorageCleanupSchedule {
    /// Returns all accesses whose cleanup can be selected.
    pub fn accesses(&self) -> &[StorageAccessId] {
        match self {
            Self::Fixed(order) => order,
            Self::Conditional { priority, .. } => priority,
        }
    }
}

impl StorageFlow {
    /// Reached guarded normal outcomes for one lifecycle action.
    pub fn cleanup_outcomes(&self, action: StorageCleanupAction) -> impl Iterator<Item = (StorageMutation, usize)> + '_ {
        let first = StorageMutation::CleanupOutcome { action, row: 0 };
        let last = StorageMutation::CleanupOutcome { action, row: usize::MAX };

        self.outcome_mutations.range(first..=last).map(|(&mutation, _)| {
            let StorageMutation::CleanupOutcome { row, .. } = mutation else {
                unreachable!("guarded cleanup publications have one action's normal rows");
            };

            (mutation, row)
        })
    }

    /// Publishes the reached inputs of each cleanup phase and continuation.
    pub fn with_cleanup_entries(
        mut self,
        entries: BTreeMap<(AnalysisOperationId, Option<StorageCleanupAction>), StorageCleanupEntry>,
    ) -> Self {
        assert!(entries.iter().all(|((operation, after), entry)| {
            operation.unit() == self.unit()
                && after.is_none_or(|action| action.operation.unit() == self.unit())
                && entry.orders().iter().flat_map(|order| order.iter()).all(|access| access.unit() == self.unit())
                && entry.absent().all(|path| path.is_valid_for(self.unit()))
                && entry.replacement().is_none_or(|target| target.is_valid_for(self.unit()))
        }), "cleanup entries belong to their checked unit");

        self.cleanup_entries = entries;

        self
    }

    /// Reads the reached inputs of this phase or of the suffix after the given action.
    pub fn cleanup_entry(&self, operation: AnalysisOperationId, after: Option<StorageCleanupAction>) -> Option<&StorageCleanupEntry> {
        self.cleanup_entries.get(&(operation, after))
    }

    /// Publishes continuations captured at an actual failed action.
    pub fn with_cleanup_remainders(
        mut self,
        remainders: BTreeMap<StorageMutation, BTreeSet<StorageCleanupRemainder>>,
    ) -> Self {
        assert!(remainders.iter().all(|(failure, rows)| {
            matches!(failure, StorageMutation::CleanupFailure { action, .. }
                if action.operation.unit() == self.unit()
                    && rows.iter().all(|row| row.after == *action && row.accesses.first() == Some(&action.access)))
        }), "cleanup remainders start at their owning action");

        self.cleanup_remainders = remainders;

        self
    }

    /// Returns the checked suffixes of a reached cleanup action.
    pub fn cleanup_remainders(&self, failure: StorageMutation) -> impl Iterator<Item = &StorageCleanupRemainder> {
        self.cleanup_remainders.get(&failure).into_iter().flatten()
    }

    /// Returns the reached failure effects of one cleanup action.
    pub fn cleanup_failures(
        &self,
        action: StorageCleanupAction,
    ) -> impl Iterator<Item = (StorageMutation, &StorageInvalidation)> {
        use bray_symbols::DependencyFailureKind::{Cancellation, Panic};

        let first = StorageMutation::CleanupFailure { action, kind: Panic, row: None };
        let last = StorageMutation::CleanupFailure { action, kind: Cancellation, row: Some(usize::MAX) };

        self.outcome_mutations.range(first..=last).map(|(mutation, invalidation)| (*mutation, invalidation))
    }

    /// Returns whether this owner has a reached failure before the scope completes.
    pub fn cleanup_owner_can_fail(
        &self,
        point: StorageExitPoint,
        access: StorageAccessId,
        kind: bray_symbols::DependencyFailureKind,
    ) -> bool {
        let occurrence = BoundExecutionSite::Cleanup(point.exit());

        let first = StorageCleanupAction {
            occurrence,
            access,
            kind: StorageCleanupActionKind::Broadcast,
            operation: AnalysisOperationId::new(self.unit(), 0),
            part: 0,
        };

        let first = StorageMutation::CleanupFailure {
            action: first,
            kind: bray_symbols::DependencyFailureKind::Panic,
            row: None,
        };

        self.cleanup_remainders.range(first..)
            .take_while(|(failure, _)| {
                matches!(failure, StorageMutation::CleanupFailure { action, .. }
                    if action.occurrence == occurrence && action.access == access)
            })
            .any(|(failure, remainders)| {
                matches!(failure, StorageMutation::CleanupFailure { kind: actual, .. } if *actual == kind)
                    && remainders.iter().any(|remainder| remainder.point == Some(point))
            })
    }

    /// Returns each reached opaque cleanup of the access at this source boundary.
    pub fn opaque_cleanup_mutations(
        &self,
        occurrence: BoundExecutionSite,
        access: StorageAccessId,
    ) -> impl Iterator<Item = (AnalysisOperationId, usize, &StorageInvalidation)> {
        let first = StorageCleanupAction {
            occurrence,
            access,
            kind: StorageCleanupActionKind::Opaque,
            operation: AnalysisOperationId::new(self.unit(), 0),
            part: 0,
        };

        let last = StorageCleanupAction {
            operation: AnalysisOperationId::new(self.unit(), u32::MAX),
            part: usize::MAX,
            ..first
        };

        self.outcome_mutations.range(StorageMutation::Cleanup(first)..=StorageMutation::Cleanup(last)).map(|(mutation, invalidation)| {
            let StorageMutation::Cleanup(action) = mutation else {
                unreachable!("opaque cleanup range retains only those actions");
            };

            (action.operation, action.part, invalidation)
        })
    }
}
