use bray_bound_tree::{CheckedMemoryOperationKind as Operation, StorageInvalidation, StorageObservationKind as Observation, StorageRelationship};
use bray_symbols::DependencyMutationKind as Mutation;

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Effect, State, Target};
use crate::analysis::storage_flow::model::StorageFlowState;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn preserve_storage_values(
        &mut self,
        state: &StorageFlowState,
        memory: &State,
        operation: Option<&bray_bound_tree::CheckedMemoryOperation>,
        effects: Option<&[Effect]>,
        invalidation: &mut StorageInvalidation,
    ) {
        let typed_transfer = operation.and_then(|operation| match operation.kind() {
            Operation::Read { pointee, kind: bray_bound_tree::MemoryReadKind::Move }
            | Operation::VolatileRead { pointee, kind: bray_bound_tree::MemoryReadKind::Move, .. }
            | Operation::Write { pointee } | Operation::Copy { pointee, .. }
            | Operation::VolatileWrite { pointee, .. } => Some(*pointee),
            Operation::UninitMove { element } | Operation::UninitAssumeInitialized { element }
            | Operation::UninitWrite { element } => Some(*element),
            _ => None,
        });

        let raw_operation = operation.is_some_and(|operation| matches!(operation.kind(),
            Operation::Read { .. } | Operation::VolatileRead { .. } | Operation::Write { .. }
            | Operation::Copy { .. } | Operation::VolatileWrite { .. }));

        // Classify once per resolved effect. Keep its target and scope until the
        // observation decision, rather than reducing all transfers to a type set.
        let unknown = [Effect::Unknown];
        let mut transfers = Vec::new();

        for effect in effects.unwrap_or(&unknown) {
            let raw = raw_operation || match effect {
                Effect::Mutation(_, _, Some(ty)) | Effect::UnboundedMutation { access_type: Some(ty), .. } => crate::representation::reference_kind(self.request, *ty)
                    == Some(bray_symbols::DependencyReferenceKind::Address),
                _ => false,
            };

            let ty = match effect {
                Effect::Escape | Effect::Atomic | Effect::Mutation(_, Mutation::Escape | Mutation::Atomic, _)
                | Effect::UnboundedMutation { kind: Mutation::Escape | Mutation::Atomic, .. } => continue,
                Effect::Mutation(_, Mutation::Replace(ty), _) | Effect::UnboundedMutation { kind: Mutation::Replace(ty), .. } => Some(*ty),
                Effect::Mutation(_, Mutation::Write | Mutation::Consume | Mutation::Clear, Some(ty))
                | Effect::UnboundedMutation { kind: Mutation::Write | Mutation::Consume | Mutation::Clear, access_type: Some(ty), .. } if raw => {
                    crate::representation::reference_target_type(self.request, *ty)
                }
                Effect::Mutation(target, Mutation::Write | Mutation::Consume | Mutation::Clear, _) => {
                    if let Some(ty) = typed_transfer { Some(ty) } else {
                        match memory.projected_type(self.request, target) {
                            Ok(ty) => ty,
                            Err(error) => {
                                self.query_failure = Some(error);

                                None
                            }
                        }
                    }
                }
                _ => typed_transfer,
            };

            transfers.push((effect, ty, raw));
        }

        let exposed = memory.raw_exposed_regions();

        let survives = |target: &Target, observation: Observation, owner: Option<bray_symbols::TypeId>, shared: bool| {
            transfers.iter().all(|(effect, ty, raw)| {
                // Ordinary code cannot transfer a retained shared loan's inline
                // owner. Raw operations still retain their actual reached region.
                if shared && !raw { return true; }

                let transports = match observation {
                    Observation::Marker(marker) => ty.and_then(|ty| self.input.footprint.inline_carriers(ty))
                        .is_none_or(|markers| markers.contains(&marker)),
                    Observation::Value | Observation::Mutable | Observation::Identity => {
                        // A typed move can retire only the nominal carriers it moves.
                        // Scalar fields alone provide no such ownership bound. Writes
                        // still change data through aliases of different nominal types.
                        let moving = matches!(effect, Effect::Mutation(_, Mutation::Consume, _)
                            | Effect::UnboundedMutation { kind: Mutation::Consume, .. });

                        !moving || !raw || owner.and_then(|owner| self.input.footprint.inline_carriers(owner))
                            .zip(ty.and_then(|ty| self.input.footprint.inline_carriers(ty)))
                            .is_none_or(|(owner, transfer)| owner.is_empty() || !owner.is_disjoint(transfer))
                    },
                    _ => false,
                };

                if !transports { return true; }

                let retained = match effect {
                    Effect::Mutation(reached, _, _) => memory.target_relationship(reached, target) == StorageRelationship::Disjoint,
                    Effect::UnboundedMutation { ambient: true, .. } | Effect::Ambient => exposed.iter()
                        .all(|reached| memory.target_relationship(reached, target) == StorageRelationship::Disjoint),
                    _ => false,
                };

                retained
            })
        };

        match invalidation {
            StorageInvalidation::All { stable_values, stable_descriptors, .. }
            | StorageInvalidation::Observed { stable_values, stable_descriptors, .. } => {
                for (path, marker) in &self.input.footprint.empty_markers {
                    if !survives(&Target::storage(path.clone()), Observation::Marker(*marker), None, false) { continue; }

                    stable_values.insert(path.clone(), bray_bound_tree::StorageObservationKind::Value);

                    // A surviving marker retains its typed carrier identity. Its
                    // enclosing snapshots may still have different contents.
                    stable_descriptors.extend((0..path.projections.len()).map(|length| bray_bound_tree::StoragePath {
                        root: path.root,
                        projections: path.projections[..length].into(),
                    }));
                }
            }
            StorageInvalidation::Paths(_) => return,
        }

        let shared = |target: &Target, observation, ty| survives(target, observation, Some(ty), true);
        let active = |target: &Target, observation, ty, shared| survives(target, observation, Some(ty), shared);

        self.preserve_active_borrows(state, memory, &active, invalidation);
        self.preserve_shared_views(memory, &shared, invalidation);

    }
}
