use super::super::model::{Effect, Origin, Place, References, State, Target};
use bray_bound_tree::StorageInvalidation;
use bray_symbols::DependencyMutationKind;

impl State {
    pub(in crate::analysis::storage_flow) fn stable_reference_slot(
        target: &Target,
        invalidation: &StorageInvalidation,
    ) -> bool {
        if invalidation.preserves_reference_slot(target) { return true; }

        let (descriptors, values) = match invalidation {
            StorageInvalidation::All {
                stable_descriptors, stable_values, ..
            }
            | StorageInvalidation::Observed {
                stable_descriptors, stable_values, ..
            } => (stable_descriptors, stable_values),
            StorageInvalidation::Paths(_) => return false,
        };

        // A retained whole descriptor protects its inline reference values. It does
        // not protect the storage reached through those references or owned backing.
        target.observation_path().is_some_and(|slot| {
            values.get(&slot) == Some(&bray_bound_tree::StorageObservationKind::Address)
                || descriptors.contains(&slot) || !slot
                .projections
                .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                && descriptors.contains(&Place::root(slot.root))
        })
    }

    pub(in crate::analysis::storage_flow) fn cleanup_effects(
        &self,
        receiver: Option<&Target>,
        ty: bray_symbols::TypeId,
    ) -> Vec<Effect> {
        let kind = DependencyMutationKind::Dispose(ty);
        let Some(receiver) = receiver else {
            return vec![Effect::unbounded(kind, None, false)];
        };

        // The target is the disposed value, not each differently typed referent.
        // Consumers follow its stored references while the value is still live.
        vec![Effect::Mutation(receiver.clone(), kind, Some(ty))]
    }

    /// Normal ambient effects share external roots and this call's escaped inputs.
    /// A whole invocation escape has no finite reachability bound.
    pub(in crate::analysis::storage_flow) fn ambient_regions(&self, effects: &[Effect]) -> Option<std::collections::BTreeSet<Target>> {
        if effects.contains(&Effect::Escape) { return None; }
        let escaped = effects.iter().filter_map(|effect| match effect {
            Effect::Mutation(target, DependencyMutationKind::Escape, _) => Some(target.clone()),
            _ => None,
        });
        Some(self.reachable_targets(self.escaped_regions(escaped)))
    }

    pub(in crate::analysis::storage_flow) fn apply_effects(
        &mut self,
        effects: Option<&[Effect]>,
        invalidation: &StorageInvalidation,
        storage: &bray_bound_tree::StoragePlan,
    ) {
        let Some(effects) = effects.filter(|effects| {
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Unknown))
        }) else {
            self.invalidate(invalidation, storage, DependencyMutationKind::Retire);
            return;
        };

        for effect in effects {
            match effect {
                Effect::Escape | Effect::Mutation(_, DependencyMutationKind::Escape, _) => {}
                Effect::Mutation(target, DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_), _) => {
                    let (targets, _) = self.transitive_regions([target.clone()], |_, _| true);
                    self.invalidate(invalidation, storage, DependencyMutationKind::Retire);

                    for target in targets {
                        self.invalidate_target(&target, DependencyMutationKind::Retire, invalidation, false);
                    }
                }
                Effect::Mutation(target, kind, access_type) => {
                    self.invalidate_target(target, *kind, invalidation, access_type.is_none())
                }
                Effect::UnboundedMutation { kind, ambient: false, .. } => {
                    self.invalidate(invalidation, storage, *kind);
                }
                effect if effect.is_ambient() => {
                    let kind = match effect {
                        Effect::UnboundedMutation { kind, .. } => *kind,
                        _ => DependencyMutationKind::Retire,
                    };

                    if let Some(targets) = self.ambient_regions(effects) {
                        for target in targets {
                            self.invalidate_target(
                                &target,
                                kind,
                                invalidation,
                                false,
                            );
                        }
                    } else {
                        self.invalidate(invalidation, storage, kind);
                    }
                }
                effect if effect.is_atomic() => self.invalidate(
                    &StorageInvalidation::unknown(),
                    storage,
                    DependencyMutationKind::Atomic,
                ),
                Effect::Unknown => {
                    unreachable!("opaque effects handled above")
                }
                Effect::Ambient | Effect::Atomic | Effect::UnboundedMutation { .. } => {
                    unreachable!("classified effects handled above")
                }
            }
        }
    }

    fn invalidate_target(
        &mut self,
        target: &Target,
        kind: DependencyMutationKind,
        invalidation: &StorageInvalidation,
        complete_cell: bool,
    ) {
        let references = References::one(target.clone());

        for affected in self.affected_targets(&references) {
            // Atomic effects change protected payload observations, not ordinary
            // reference slots that merely may alias the atomic's address.
            if kind != DependencyMutationKind::Atomic
                && !Self::stable_reference_slot(&affected, invalidation)
            {
                self.values.remove(&affected);
            }

            if let Some(contents) = self.contents.get_mut(&affected) {
                // An alias bound does not prove a complete typed update. Preserve
                // only the ordinary selected cell, never a merely overlapping view.
                contents.initialized.retain(|ty| {
                    contents.storage_type == Some(*ty)
                        && (kind == DependencyMutationKind::Atomic
                            || complete_cell && target.is_exact() && *target == affected
                                && match kind {
                                    DependencyMutationKind::Write | DependencyMutationKind::Clear => true,
                                    DependencyMutationKind::Replace(written) => written == *ty,
                                    _ => false,
                                })
                });
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate(
        &mut self,
        invalidation: &StorageInvalidation,
        storage: &bray_bound_tree::StoragePlan,
        kind: DependencyMutationKind,
    ) {
        // An opaque operation can change the referent without replacing a reference
        // stored in an unexposed owner or reached through a protected shared entry borrow.
        // The established observation identifies the slot independently of its referent.
        let unaddressed = &self.unaddressed_storage;

        let unknown = matches!(
            invalidation,
            StorageInvalidation::All { .. }
        );

        self.values.retain(|target, _| {
            kind == DependencyMutationKind::Atomic
                || unknown && Self::unaddressed_inline_slot(unaddressed, target)
                || Self::stable_reference_slot(target, invalidation)
                || match target.observation_path() {
                    Some(slot) => !invalidation.invalidates_path(&slot, storage),
                    None => !matches!(
                        invalidation,
                        StorageInvalidation::All { .. }
                    ),
                }
        });

        for (target, contents) in &mut self.contents {
            if unknown && Self::unaddressed_inline_slot(unaddressed, target) {
                continue;
            }

            let affected = match &target.origin {
                Origin::Storage(path) => invalidation.invalidates_path(path, storage),
                Origin::Entry { .. }
                | Origin::Returned { .. }
                | Origin::Allocation { .. }
                | Origin::Static { .. } => {
                    matches!(
                        invalidation,
                        StorageInvalidation::All { .. }
                    )
                }
            };

            if affected {
                // Unbounded mutations establish no complete type-correct cell update.
                // Semantic ownership framing retains the precise transition separately.
                contents.initialized.retain(|ty| {
                    kind == DependencyMutationKind::Atomic && contents.storage_type == Some(*ty)
                });
            }
        }
    }

}
