use bray_bound_tree::{CheckedMemoryOperationKind as Operation, StorageAddressValue, StorageInvalidation, StorageObservationKind as Observation, StorageRelationship};
use bray_symbols::DependencyMutationKind as Mutation;

use super::core::{OperationAccesses, StorageFlowCollector};
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Effect, Target};
use crate::analysis::storage_flow::model::StorageFlowState;

pub(super) fn location_effects(
    address: Option<&StorageAddressValue>,
    mutation: Mutation,
    ty: Option<bray_symbols::TypeId>,
) -> impl Iterator<Item = Effect> + '_ {
    let exact = address.is_some_and(StorageAddressValue::is_exact);
    let regions = address.and_then(StorageAddressValue::regions);
    let unbounded = regions.is_none().then(|| Effect::unbounded(mutation, ty, false));

    regions.into_iter().flatten().map(move |target| {
        if exact {
            Effect::at(target, mutation, ty)
        } else {
            Effect::bounded(target, mutation, ty)
        }
    }).chain(unbounded)
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn capture_transfer_accesses(
        &self,
        state: &StorageFlowState,
        kind: crate::analysis::model::AnalysisOperationKind,
    ) -> OperationAccesses {
        kind.storage_plans(self.storage).filter(|plan| {
            plan.purpose().transfers_value()
                && !self.storage.access_is_produced_at(plan.access(), kind.occurrence())
        }).map(|plan| {
            let reached = self.input.reached_access(self.request.semantic_values(), &state.memory, self.storage, plan.access());

            (plan, reached)
        }).collect()
    }

    pub(super) fn value_transfer_effects(
        &self, kind: crate::analysis::model::AnalysisOperationKind, accesses: &OperationAccesses,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();

        for plan in kind.storage_plans(self.storage).filter(|plan| {
            self.storage.effective_purpose(*plan) == bray_bound_tree::StorageAccessPurpose::Move
                && !self.storage.access_is_produced_at(plan.access(), kind.occurrence())
        }) {
            let ty = self.storage.access(plan.access()).expect("a checked move retains its type").reached_type();
            let targets = accesses.get(&plan).expect("input transfer retains its resolved access");

            effects.extend(location_effects(targets.as_ref().map(|targets| &targets.address), Mutation::Consume, Some(ty)));
        }

        // Evaluated result handles retire after their destination is published.
        // They are separate from mutations of the source values used here.
        effects
    }

    pub(super) fn preserve_storage_values(
        &mut self,
        state: &StorageFlowState,
        operation: Option<&bray_bound_tree::CheckedMemoryOperation>,
        effects: Option<&[Effect]>,
        invalidation: &mut StorageInvalidation,
    ) {
        if invalidation.observation_transition() == bray_bound_tree::StorageObservationTransition::Unchanged {
            return;
        }

        let memory = &state.memory;

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
                Effect::Mutation { access_type: Some(ty), .. } | Effect::UnboundedMutation { access_type: Some(ty), .. } => crate::representation::reference_kind(self.request, *ty)
                    == Some(bray_symbols::DependencyReferenceKind::Address),
                _ => false,
            };

            let ty = match effect {
                Effect::Escape | Effect::Atomic | Effect::Mutation { kind: Mutation::Escape | Mutation::Atomic, .. }
                | Effect::UnboundedMutation { kind: Mutation::Escape | Mutation::Atomic, .. } => continue,
                Effect::Mutation { kind: Mutation::Replace(ty), .. } | Effect::UnboundedMutation { kind: Mutation::Replace(ty), .. } => Some(*ty),
                Effect::Mutation { kind: Mutation::Write | Mutation::Consume | Mutation::Clear, access_type: Some(ty), .. }
                | Effect::UnboundedMutation { kind: Mutation::Write | Mutation::Consume | Mutation::Clear, access_type: Some(ty), .. } if raw => {
                    crate::representation::reference_target_type(self.request, *ty)
                }
                Effect::Mutation { address: Some(target), kind: Mutation::Write | Mutation::Consume | Mutation::Clear, .. } => {
                    if let Some(ty) = typed_transfer { Some(ty) } else {
                        match memory.projected_type(self.request, target) {
                            Ok(ty) => ty,
                            Err(error) => {
                                self.fail_query(error);

                                None
                            }
                        }
                    }
                }
                _ => typed_transfer,
            };

            transfers.push((effect, ty, raw));
        }

        let ambient = std::cell::OnceCell::new();

        let transfers_owner = |owner: Option<bray_symbols::TypeId>, ty: Option<bray_symbols::TypeId>| {
            owner.and_then(|owner| self.input.footprint.inline_carriers(owner))
                .zip(ty.and_then(|ty| self.input.footprint.inline_carriers(ty)))
                .is_none_or(|(owner, transfer)| owner.is_empty() || !owner.is_disjoint(transfer))
        };

        let survives = |target: &Target, observation: Observation, owner: Option<bray_symbols::TypeId>, borrow: Option<bray_symbols::BorrowKind>| {
            transfers.iter().all(|(effect, ty, raw)| {
                // Ordinary code cannot transfer a retained shared loan's inline
                // owner. Raw operations still retain their actual reached region.
                if borrow == Some(bray_symbols::BorrowKind::Shared) && !raw { return true; }

                let transports = match observation {
                    Observation::Marker(marker) => ty.and_then(|ty| self.input.footprint.inline_carriers(ty))
                        .is_none_or(|markers| markers.contains(&marker)),
                    Observation::Identity => !raw || transfers_owner(owner, *ty),
                    Observation::Value | Observation::InlineBits | Observation::Mutable => {
                        // A typed move can retire only the nominal carriers it moves.
                        // Scalar fields alone provide no such ownership bound. Writes
                        // still change data through aliases of different nominal types.
                        let moving = matches!(effect, Effect::Mutation { kind: Mutation::Consume, .. }
                            | Effect::UnboundedMutation { kind: Mutation::Consume, .. });

                        !moving || !raw || transfers_owner(owner, *ty)
                    },
                    // Tags are actual stored data. A surviving marker or owner
                    // identity alone cannot preserve an overwritten variant.
                    Observation::Discriminant | Observation::Address => true,
                    _ => false,
                };

                if !transports { return true; }

                let retained = match effect {
                    Effect::Mutation { footprint: reached, .. } => {
                        let writes_inside_payload = !raw && observation == Observation::Discriminant
                            && reached.projections().len() > target.projections().len()
                            && target.contains_storage(reached);

                        writes_inside_payload || memory.target_relationship(reached, target) == StorageRelationship::Disjoint
                    }
                    Effect::UnboundedMutation { ambient: true, .. } | Effect::Ambient => ambient
                        .get_or_init(|| memory.ambient_regions(effects.unwrap_or(&unknown)))
                        .as_ref().is_some_and(|(regions, unknown)| {
                            (!*unknown || !memory.unknown_reference_may_reach(target))
                                && regions.iter().all(|reached| memory.target_relationship(reached, target) == StorageRelationship::Disjoint)
                        }),
                    _ => false,
                };

                retained
            })
        };

        match invalidation {
            StorageInvalidation::All { stable_values, stable_descriptors, .. }
            | StorageInvalidation::Observed { stable_values, stable_descriptors, .. } => {
                for observations in self.input.footprint.owned_observations.values() {
                    for (path, observation) in observations {
                        if !matches!(observation, Observation::Value | Observation::InlineBits | Observation::Discriminant | Observation::Address) { continue; }

                        if survives(&Target::storage(path.clone()), *observation, None, None) {
                            observation.record_preservation(path.clone(), stable_descriptors, stable_values);
                        }
                    }
                }

                for (path, marker) in &self.input.footprint.empty_markers {
                    if !survives(&Target::storage(path.clone()), Observation::Marker(*marker), None, None) { continue; }

                    stable_values.insert(path.clone(), bray_bound_tree::StorageObservationKind::Value);
                }
            }
            StorageInvalidation::Unchanged => return,
        }

        let shared = |target: &Target, observation, ty| survives(target, observation, Some(ty), Some(bray_symbols::BorrowKind::Shared));
        let active = |target: &Target, observation, ty, shared| survives(target, observation, Some(ty), Some(if shared { bray_symbols::BorrowKind::Shared } else { bray_symbols::BorrowKind::Mutable }));

        self.preserve_active_borrows(state, memory, &active, invalidation);

        self.preserve_shared_views(memory, &shared,
            &|slot| survives(slot, Observation::Address, None, None), invalidation);

        let pointees = match invalidation {
            StorageInvalidation::All { stable_pointees, .. }
            | StorageInvalidation::Observed { stable_pointees, .. } => stable_pointees,
            StorageInvalidation::Unchanged => unreachable!("physical preservation follows flow resolution"),
        };

        for (target, ty) in memory.typed_storage().filter(|(target, _)| target.value_observation_path().is_none()) {
            let observations = self.input.footprint.typed_observations(ty)
                .filter(|(path, kind)| kind.is_ordinary_inline()
                    && survives(&target.project(path), *kind, Some(ty), None))
                .map(|(path, kind)| (std::sync::Arc::<[bray_bound_tree::StorageProjection]>::from(path), kind));

            pointees.entry((target.clone(), ty)).or_default().extend(observations);
        }

    }
}
