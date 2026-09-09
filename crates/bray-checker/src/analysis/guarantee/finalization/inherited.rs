use bray_bound_tree::{
    CheckedAsync, StorageAccessId, StorageCleanupProjection, StorageRelationship,
};

use super::super::super::model::AnalysisOperationKind;
use super::super::flow::GuaranteeDomain;
use crate::{CheckerRequestContext, CheckerUnitView};

/// Identifies receiver parts whose inherited obligations survive the authored destructor body.
pub(super) fn inherited_destructor_part<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    asynchronous: &CheckedAsync,
    access: StorageAccessId,
    path: &[StorageCleanupProjection],
) -> bool {
    let Some(identity) = domain.storage.root_identity(access) else {
        return false;
    };

    if !bray_bound_tree::storage_identity_is_destructor_receiver(
        request.unit(),
        domain.storage,
        identity,
    ) {
        return false;
    }

    let path = super::super::super::storage_index::cleanup_mutation_path(path);

    domain
        .graph
        .blocks()
        .iter()
        .flat_map(|block| block.operations())
        .filter_map(|operation| domain.graph.operation(*operation))
        .all(|operation| {
            if domain.pure_operations.contains(&operation.id()) {
                return true;
            }

            if let AnalysisOperationKind::ScopeExit { block, exit, .. } = operation.kind() {
                // Receiver remainder runs after the authored body. Other owned cleanup can mutate it.
                return asynchronous
                    .scope_exits()
                    .iter()
                    .filter(|plan| plan.scope() == block && plan.exit() == exit)
                    .all(|plan| {
                        !plan.is_recovered()
                            && plan
                                .lifecycle_resolution()
                                .iter()
                                .chain(plan.cancellation_broadcast())
                                .all(|access| {
                                    domain.storage.root_identity(*access) == Some(identity)
                                })
                    });
            }

            domain.assignments.contains_key(&operation.id())
                && !domain.storage_calls.contains(&operation.kind().node())
                && domain
                    .mutations
                    .get(&operation.kind().point())
                    .is_some_and(|mutations| {
                        mutations.iter().all(|mutation| {
                            domain
                                .storage
                                .projected_relationship(access, &path, *mutation)
                                == StorageRelationship::Disjoint
                        })
                    })
        })
}
