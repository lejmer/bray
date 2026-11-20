use bray_bound_tree::{
    AsyncStorageCleanupRequirement, AsyncStorageExitRecoveryCause, StoragePlan,
    StorageReplacementDecision, StorageReplacementState,
};

use super::cleanup::CleanupShapeResolver;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> CleanupShapeResolver<'_, C> {
    pub(crate) fn select_replacement_cleanup(
        &self,
        storage: &StoragePlan,
        decision: StorageReplacementDecision,
    ) -> StorageReplacementDecision {
        if storage.access(decision.access()).is_none() || decision.is_recovered() {
            return decision.with_cleanup_selection(
                AsyncStorageCleanupRequirement::Recovered(
                    AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                ),
                None,
            );
        }

        let requirements = self.requirements.as_ref().expect("replacement selection uses prepared destination cleanup");
        let cleanup = requirements.replacement(decision.access());

        if decision.state() == StorageReplacementState::Absent {
            return decision.with_cleanup_selection(AsyncStorageCleanupRequirement::None, None);
        }

        if !matches!(cleanup, AsyncStorageCleanupRequirement::Cleanup(_)) {
            return decision.with_cleanup_selection(cleanup, None);
        }

        let partial_cleanup_unavailable = AsyncStorageCleanupRequirement::Recovered(
            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
        );

        let Some(path) = storage.logical_path(decision.access()) else {
            return decision.with_cleanup_selection(partial_cleanup_unavailable, None);
        };

        let moved = decision.moved().iter().any(|moved| {
            path.contains(moved) && moved.projections.len() > path.projections.len()
        });

        if !moved {
            return decision.with_cleanup_selection(cleanup, None);
        }

        let parts = requirements.get(path.root).and_then(|owner| owner.parts_below(&path.projections));

        let cleanup = if parts.is_some() { cleanup } else { partial_cleanup_unavailable };

        decision.with_cleanup_selection(cleanup, parts)
    }
}
