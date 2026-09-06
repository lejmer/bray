use bray_bound_tree::{
    AsyncStorageCleanupRequirement, AsyncStorageExitRecoveryCause, StorageCleanupType, StorageFlow,
    StoragePlan, StorageReplacementPlan, StorageReplacementState,
};

use super::cleanup::{CleanupShapeResolver, cleanup_requirement};
use super::parts::CleanupExpansion;
use crate::{CheckerQueryError, CheckerRequestContext};

pub(super) fn replacement_plans<C: CheckerRequestContext + ?Sized>(
    storage: &StoragePlan,
    flow: &StorageFlow,
    shapes: &mut CleanupShapeResolver<'_, C>,
) -> Result<Vec<StorageReplacementPlan>, CheckerQueryError<C::UpstreamError>> {
    let mut plans = Vec::new();

    for decision in flow.replacements() {
        let mut cleanup = AsyncStorageCleanupRequirement::Recovered(
            AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
        );

        let mut parts = None;

        if !decision.is_recovered()
            && let Some(access) = storage.access(decision.access())
        {
            let ty = access.reached_type();
            cleanup = cleanup_requirement(shapes.resolve(ty)?);

            shapes
                .cleanup_types
                .entry(ty)
                .or_insert_with(|| StorageCleanupType::new(ty, cleanup));

            if decision.state() == StorageReplacementState::Absent {
                cleanup = AsyncStorageCleanupRequirement::None;
            } else {
                let prefix = storage.resolved_projections(decision.access());

                let moved = decision
                    .moved()
                    .iter()
                    .map(|access| storage.resolved_projections(*access))
                    .collect::<Option<Vec<_>>>();

                match (prefix, moved) {
                    (Some(prefix), Some(moved)) => {
                        let moved = moved
                            .into_iter()
                            .filter_map(|path| path.strip_prefix(prefix))
                            .filter(|path| !path.is_empty())
                            .collect::<Vec<_>>();

                        if !moved.is_empty() {
                            parts = shapes.represented_parts(
                                ty,
                                &moved,
                                CleanupExpansion::MovedPaths,
                                access.source(),
                            )?;

                            if parts.is_none() {
                                cleanup = AsyncStorageCleanupRequirement::Recovered(
                                    AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                                );
                            }
                        }
                    }
                    _ => {
                        cleanup = AsyncStorageCleanupRequirement::Recovered(
                            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                        );
                    }
                }
            }
        }

        plans.push(StorageReplacementPlan::new(
            decision.expression(),
            decision.access(),
            cleanup,
            parts,
        ));
    }

    Ok(plans)
}
