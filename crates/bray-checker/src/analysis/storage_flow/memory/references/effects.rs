use super::super::model::{Origin, Place, AddressValue, State, Target};
use super::super::Effect;
use bray_bound_tree::{BoundExecutionSite, StorageInvalidation};
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
            StorageInvalidation::Unchanged => return false,
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
        vec![Effect::at(receiver.clone(), kind, Some(ty))]
    }

    /// Normal ambient effects share external roots and this call's escaped inputs.
    /// A whole invocation escape has no finite reachability bound.
    pub(in crate::analysis::storage_flow) fn ambient_regions(&self, effects: &[Effect]) -> Option<(std::collections::BTreeSet<Target>, bool)> {
        if effects.contains(&Effect::Escape(None)) { return None; }

        let escaped = effects.iter().flat_map(Effect::escape_regions).cloned();

        Some(self.reachable_regions(self.escaped_regions(escaped)))
    }

    pub(in crate::analysis::storage_flow) fn apply_effects(
        &mut self,
        effects: Option<&[Effect]>,
        invalidation: &StorageInvalidation,
        origin: BoundExecutionSite,
    ) {
        let Some(effects) = effects else {
            self.invalidate(invalidation, DependencyMutationKind::Write);
            return;
        };

        if effects.contains(&Effect::Unknown) {
            self.invalidate(invalidation, DependencyMutationKind::Write);
        }

        let unbounded_ambient = effects.contains(&Effect::Escape(None));
        let mut previous_unbounded = None;
        let mut previous_bound = None;

        for effect in effects {
            let unbounded = match effect {
                Effect::UnboundedMutation { kind, ambient, .. } if !*ambient || unbounded_ambient => Some(*kind),
                Effect::Ambient if unbounded_ambient => Some(DependencyMutationKind::Write),
                _ => None,
            };

            if let Some(kind) = unbounded {
                previous_bound = None;

                // Without an address, the access type still informs proof framing.
                // Physical weakening only distinguishes atomic and allocation scope.
                let deallocation = effect.is_unlocated_deallocation();
                let scope = (kind == DependencyMutationKind::Atomic, deallocation, kind.preserves_discharge());

                if previous_unbounded != Some(scope) {
                    self.invalidate_unknown_targets(invalidation, kind, deallocation);
                    previous_unbounded = Some(scope);
                }

                continue;
            }

            previous_unbounded = None;

            let bound = match effect {
                Effect::Mutation { address: None, footprint, kind, .. }
                    if !matches!(kind, DependencyMutationKind::Atomic | DependencyMutationKind::Dispose(_)
                        | DependencyMutationKind::Cleanup(_) | DependencyMutationKind::Escape) => Some((footprint, kind.preserves_discharge())),
                _ => None,
            };

            // These effects only forget overlapping values and initialization.
            // Their checked types still remain on the transition for proof.
            if bound.is_some() && previous_bound == bound {
                continue;
            }

            previous_bound = bound;

            match effect {
                Effect::Escape(_) | Effect::Mutation { kind: DependencyMutationKind::Escape, .. } => {}
                Effect::Mutation { address, footprint: target, kind: kind @ (DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_)), .. } => {
                    let backing = if matches!(kind, DependencyMutationKind::Dispose(_)) {
                        address.as_ref().and_then(|owner| self.owned_backing(owner)).cloned()
                    } else { None };

                    let (mut targets, unknown) = self.transitive_regions([target.clone()], |_, _| true);

                    self.include_unknown_targets(&mut targets, unknown);

                    self.invalidate(invalidation, *kind);

                    self.invalidate_cleanup_regions(&targets, invalidation);

                    if let Some(backing) = backing {
                        self.invalidate_target(&backing, DependencyMutationKind::Retire, invalidation, true, Some(origin), false);
                    }
                }
                Effect::Mutation { address, footprint: target, kind, access_type, .. } => {
                    let retiring = (*kind == DependencyMutationKind::Retire).then_some(address.as_ref()).flatten();

                    if let Some(address) = retiring.filter(|address| *address != target) {
                        self.invalidate_target(address, *kind, invalidation, true, Some(origin), false);
                    }

                    self.invalidate_target(target, *kind, invalidation,
                        access_type.is_none() && address.as_ref() == Some(target),
                        retiring.filter(|address| *address == target).map(|_| origin), false)
                }
                effect if effect.is_ambient() => {
                    let kind = match effect {
                        Effect::UnboundedMutation { kind, .. } => *kind,
                        _ => DependencyMutationKind::Write,
                    };

                    if let Some((mut targets, unknown)) = self.ambient_regions(effects) {
                        self.include_unknown_targets(&mut targets, unknown);

                        for target in targets {
                            if effect.is_unlocated_deallocation() && !self.unlocated_deallocation_may_reach(&target) {
                                continue;
                            }

                            self.invalidate_target(
                                &target,
                                kind,
                                invalidation,
                                false,
                                None,
                                effect.is_unlocated_deallocation(),
                            );
                        }
                    } else {
                        self.invalidate_unknown_targets(invalidation, kind, effect.is_unlocated_deallocation());
                    }
                }
                effect if effect.is_atomic() => self.invalidate(
                    &StorageInvalidation::unknown(),
                    DependencyMutationKind::Atomic,
                ),
                Effect::Unknown => {}
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
        retirement: Option<BoundExecutionSite>,
        deallocation: bool,
    ) {
        let references = AddressValue::one(target.clone());

        for affected in self.affected_targets(&references) {
            if deallocation && !self.unlocated_deallocation_may_reach(&affected) {
                continue;
            }

            // Atomic effects change protected payload observations, not ordinary
            // reference slots that merely may alias the atomic's address.
            if kind != DependencyMutationKind::Atomic
                && !Self::stable_reference_slot(&affected, invalidation)
            {
                if complete_cell && target.is_exact() && *target == affected {
                    self.values.remove(&affected);
                } else if let Some(value) = self.values.get_mut(&affected) {
                    invalidate_value(value, affected == *target || !affected.contains_storage(target));
                }
            }

            if let Some(origin) = retirement {
                self.retire_backing(affected, origin);
            } else if let Some(contents) = self.contents.get_mut(&affected) {
                // An alias bound does not prove a complete typed update. Preserve
                // only the ordinary selected cell, never a merely overlapping view.
                let storage_type = contents.storage_type;

                contents.retain_initialization(kind.preserves_discharge(), |ty| {
                    storage_type == Some(ty)
                        && (kind == DependencyMutationKind::Atomic
                            || complete_cell && target.is_exact() && *target == affected
                                && match kind {
                                    DependencyMutationKind::Write | DependencyMutationKind::Clear => true,
                                    DependencyMutationKind::Replace(written) => written == ty,
                                    _ => false,
                                })
                });
            }
        }
    }

    fn invalidate_cleanup_regions(
        &mut self,
        regions: &std::collections::BTreeSet<Target>,
        invalidation: &StorageInvalidation,
    ) {
        let slots = self.values.keys().chain(self.contents.keys()).collect::<std::collections::BTreeSet<_>>();

        let affected = slots.into_iter().filter_map(|slot| {
            if regions.contains(slot) {
                return Some((slot.clone(), true));
            }

            let mut overlapping = regions.iter().filter(|region| {
                self.target_relationship(region, slot) != bray_bound_tree::StorageRelationship::Disjoint
            });

            let first = overlapping.next()?;
            let replaces_backing = !slot.contains_storage(first) || overlapping.any(|region| !slot.contains_storage(region));

            Some((slot.clone(), replaces_backing))
        }).collect::<Vec<_>>();

        for (slot, replaces_backing) in affected {
            if !Self::stable_reference_slot(&slot, invalidation)
                && let Some(value) = self.values.get_mut(&slot)
            {
                invalidate_value(value, replaces_backing);
            }

            if let Some(contents) = self.contents.get_mut(&slot) {
                contents.clear_initialization();
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
    ) {
        self.invalidate_unknown_targets(invalidation, kind, false);
    }

    fn invalidate_unknown_targets(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
        deallocation: bool,
    ) {
        // An opaque operation can change the referent without replacing a reference
        // stored in an unexposed owner or reached through a protected shared entry borrow.
        // The established observation identifies the slot independently of its referent.
        let unaddressed = &self.unaddressed_storage;
        let owned = &self.owned_storage;
        let unaffected = |target: &Target| deallocation && !Self::deallocation_may_reach(owned, target);

        let unknown = matches!(
            invalidation,
            StorageInvalidation::All { .. }
        );

        self.values.retain(|target, value| {
            let retained = kind == DependencyMutationKind::Atomic
                || unaffected(target)
                || unknown && Self::unaddressed_inline_slot(unaddressed, target)
                || Self::stable_reference_slot(target, invalidation)
                || match target.observation_path() {
                    Some(slot) => !invalidation.invalidates_path(&slot),
                    None => !matches!(
                        invalidation,
                        StorageInvalidation::All { .. }
                    ),
                };

            if !retained && (value.completeness.tracked() || value.owned_backing().is_some()) {
                value.clear_address();
                value.completeness.lose_completeness();
                value.forget_owned_backing();

                true
            } else {
                retained
            }
        });

        for (target, contents) in &mut self.contents {
            if unaffected(target) || unknown && Self::unaddressed_inline_slot(unaddressed, target) {
                continue;
            }

            let affected = match &target.origin {
                Origin::Storage(path) => invalidation.invalidates_path(path),
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
                let storage_type = contents.storage_type;

                contents.retain_initialization(kind.preserves_discharge(), |ty| {
                    kind == DependencyMutationKind::Atomic && storage_type == Some(ty)
                });
            }
        }
    }

}

fn invalidate_value(value: &mut super::super::model::ValueBinding, replaces_backing: bool) {
    value.clear_address();
    value.completeness.lose_completeness();

    if replaces_backing {
        value.forget_owned_backing();
    }
}
