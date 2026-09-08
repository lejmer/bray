use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, CallableProofDependency, StorageAccessId, StorageCleanupPart,
};
use bray_symbols::ExecutionProperty;

use super::finalization::{CompletionReceiver, completion_receiver, part_was_moved};
use super::flow::{DomainState, GuaranteeDomain};
use crate::CheckerInfrastructureError;

impl GuaranteeDomain<'_> {
    pub(super) fn cleanup_targets(
        &self,
        access: StorageAccessId,
    ) -> impl Iterator<Item = (Option<usize>, Option<&StorageCleanupPart>)> {
        let identity = self.storage.root_identity(access);

        let parts = self
            .asynchronous
            .storage_requirements()
            .iter()
            .find(|requirement| Some(requirement.identity()) == identity)
            .and_then(bray_bound_tree::AsyncStorageRequirement::parts);

        parts
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, part)| (Some(index), Some(part)))
            .chain(parts.is_none().then_some((None, None)))
    }

    pub(super) fn cleanup_dependencies(
        &self,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        property: ExecutionProperty,
        state: &DomainState,
    ) -> Result<Option<Vec<CallableProofDependency>>, CheckerInfrastructureError> {
        let mut dependencies = BTreeSet::new();

        // Each ordered destruction may invalidate the observations needed by its successors.
        let mut state = state.clone();

        for plan in self
            .asynchronous
            .scope_exits()
            .iter()
            .filter(|plan| plan.scope() == scope && plan.exit() == exit)
        {
            if plan.is_recovered() {
                return Ok(None);
            }

            for access in plan.lifecycle_resolution() {
                for (_, part) in self.cleanup_targets(*access) {
                    if part.is_some_and(|part| part.release().is_some()) {
                        return Ok(None);
                    }

                    if part.is_some_and(|part| {
                        !part.phases().includes_lifecycle()
                            || part_was_moved(self.storage, plan, *access, part)
                    }) {
                        continue;
                    }

                    let path = part.map_or(&[][..], StorageCleanupPart::projections);

                    let Some(ty) = path
                        .last()
                        .map(|projection| projection.result_type())
                        .or_else(|| {
                            self.storage
                                .access(*access)
                                .map(|access| access.reached_type())
                        })
                    else {
                        return Ok(None);
                    };

                    let receiver = match completion_receiver(self, &state, *access, path)? {
                        CompletionReceiver::Absent => continue,
                        CompletionReceiver::Unknown => return Ok(None),
                        CompletionReceiver::Value(receiver) => receiver,
                    };

                    if self.input.finalizer(ty).is_some() {
                        let Some(proof) =
                            self.finalizer_completion_dependencies(&state, ty, receiver, exit)?
                        else {
                            return Ok(None);
                        };

                        dependencies.extend(proof);
                    }

                    let Some(proof) =
                        self.destruction_dependencies(&state, ty, receiver, exit, property)?
                    else {
                        return Ok(None);
                    };

                    dependencies.extend(proof);

                    if property != ExecutionProperty::Pure {
                        if let Some(proof) = self.destruction_dependencies(
                            &state,
                            ty,
                            receiver,
                            exit,
                            ExecutionProperty::Pure,
                        )? {
                            dependencies.extend(proof);
                        } else {
                            self.invalidate_observations(&mut state);
                        }
                    }
                }
            }
        }

        Ok(Some(dependencies.into_iter().collect()))
    }
}
