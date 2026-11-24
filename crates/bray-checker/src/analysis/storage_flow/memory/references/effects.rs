use std::sync::Arc;
use super::super::model::{Origin, Place, AddressValue, State, Target};
use super::super::Effect;
use bray_bound_tree::{BoundExecutionSite, StorageInvalidation, StorageProjection};
use bray_symbols::DependencyMutationKind;

#[derive(Clone, Copy, PartialEq)]
enum BackingRetirement {
    Possible { origin: BoundExecutionSite, bounded: bool },
    Performed(BoundExecutionSite),
    DuringCleanup(BoundExecutionSite),
}

impl BackingRetirement {
    fn unlocated(effect: &Effect, origin: BoundExecutionSite) -> Option<Self> {
        if effect.is_unlocated_deallocation() {
            return Some(Self::Possible { origin, bounded: matches!(effect, Effect::Mutation { .. }) });
        }

        match effect {
            Effect::CapturedCleanup { .. } => Some(Self::DuringCleanup(origin)),
            Effect::UnboundedMutation { kind, .. } if kind.cleanup_type().is_some() => Some(Self::DuringCleanup(origin)),
            _ => None,
        }
    }

    fn possible_origin(self, target: &Target, contents: &bray_bound_tree::StorageContents) -> Option<BoundExecutionSite> {
        // A named footprint can reach a backing without an allocation site in
        // this body. An unlocated effect can only mark tracked backing as dead.
        let tracked = !contents.allocations.is_empty() || target.projections().last() == Some(&StorageProjection::OwnedTarget);

        match self {
            Self::Possible { origin, bounded: true } => Some(origin),
            Self::Possible { origin, bounded: false } | Self::DuringCleanup(origin) if tracked => Some(origin),
            _ => None,
        }
    }
}

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
        values: &bray_symbols::SemanticValueStore,
        receiver: Option<&Target>,
        ty: bray_symbols::TypeId,
    ) -> Vec<Effect> {
        let kind = DependencyMutationKind::Dispose(ty);

        let Some(receiver) = receiver else {
            return vec![Effect::unbounded(kind, None, false)];
        };

        if !values.type_is_concrete(ty)
            && let Some((sources, complete)) = self.values.get(receiver).and_then(super::super::model::ValueBinding::whole_sources)
        {
            let uses_captured_value = if complete {
                sources.iter().any(|source| source != receiver)
            } else {
                match &receiver.origin {
                    Origin::Storage(path) => usize::try_from(path.root.ordinal()).ok()
                        .and_then(|index| self.storage_slots.get(index))
                        .is_some_and(bray_bound_tree::StorageIdentity::is_initialized_at_entry),
                    Origin::Allocation { .. } => false,
                    Origin::Entry { .. } | Origin::Returned { .. } | Origin::Static { .. } | Origin::BackingRegion => true,
                }
            };

            if uses_captured_value {
                return self.capture_cleanup(receiver, ty, false).into_iter().collect();
            }
        }

        // The target is the disposed value, not each differently typed referent.
        // Consumers follow its stored references while the value is still live.
        vec![Effect::at(receiver.clone(), kind, Some(ty))]
    }

    /// Normal ambient effects share external roots and this call's escaped inputs.
    /// A whole invocation escape has no finite reachability bound.
    pub(in crate::analysis::storage_flow) fn ambient_regions(
        &self, effects: &[Effect], _occurrence: Option<BoundExecutionSite>,
    ) -> Option<(std::collections::BTreeSet<Target>, bool)> {
        if effects.contains(&Effect::Escape(None)) { return None; }

        let escaped = effects.iter().flat_map(Effect::escape_regions).cloned()
            .chain(Effect::borrowed_reach_regions(effects).cloned());

        let (targets, unknown) = self.reachable_regions(escaped
            .chain(Effect::captured_cleanup_regions(effects).cloned()));

        Some((targets, unknown || Effect::incomplete_captured_cleanup(effects)))
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
                let retirement = BackingRetirement::unlocated(effect, origin);
                let scope = (kind == DependencyMutationKind::Atomic, retirement, kind.preserves_discharge());

                if previous_unbounded != Some(scope) {
                    self.invalidate_unknown_targets(invalidation, kind, retirement);
                    previous_unbounded = Some(scope);
                }

                continue;
            }

            previous_unbounded = None;

            let bound = match effect {
                Effect::Mutation { address: None, footprint, kind, .. }
                    if !matches!(kind, DependencyMutationKind::Atomic | DependencyMutationKind::Dispose(_)
                        | DependencyMutationKind::Cleanup(_) | DependencyMutationKind::Escape) => {
                    Some((footprint, kind.preserves_discharge(), *kind == DependencyMutationKind::Retire))
                }
                _ => None,
            };

            // These effects only forget overlapping values and initialization.
            // Their checked types still remain on the transition for proof.
            if bound.is_some() && previous_bound == bound {
                continue;
            }

            previous_bound = bound;

            match effect {
                Effect::Escape(_) | Effect::BorrowedReach(_)
                | Effect::Mutation { kind: DependencyMutationKind::Escape, .. } => {}
                Effect::Mutation { address, footprint: target, kind: kind @ (DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_)), .. } => {
                    let backing = if matches!(kind, DependencyMutationKind::Dispose(_)) {
                        address.as_ref().and_then(|owner| self.owned_backing(owner)).cloned()
                    } else { None };

                    let (mut targets, unknown) = self.transitive_regions([target.clone()], |_, _| true);

                    self.include_unknown_targets(&mut targets, unknown);

                    self.invalidate(invalidation, *kind);

                    self.invalidate_cleanup_regions(&targets, invalidation);

                    if let Some(backing) = backing {
                        self.invalidate_target(&backing, DependencyMutationKind::Retire, invalidation, true,
                            Some(BackingRetirement::Performed(origin)));
                    }
                }
                Effect::Mutation { address, footprint: target, kind, access_type, .. } => {
                    let retiring = (*kind == DependencyMutationKind::Retire).then_some(address.as_ref()).flatten();

                    if let Some(address) = retiring.filter(|address| *address != target) {
                        self.invalidate_target(address, *kind, invalidation, true, Some(BackingRetirement::Performed(origin)));
                    }

                    let retirement = (*kind == DependencyMutationKind::Retire).then(|| {
                        if retiring == Some(target) { BackingRetirement::Performed(origin) }
                        else { BackingRetirement::Possible { origin, bounded: true } }
                    });

                    self.invalidate_target(target, *kind, invalidation,
                        access_type.is_none() && address.as_ref() == Some(target), retirement)
                }
                effect if effect.is_ambient() => {
                    let kind = match effect {
                        Effect::UnboundedMutation { kind, .. } => *kind,
                        _ => DependencyMutationKind::Write,
                    };

                    let retirement = BackingRetirement::unlocated(effect, origin);

                    if let Effect::CapturedCleanup { inline, .. } = effect {
                        for target in inline.iter() {
                            self.invalidate_target(target, kind, invalidation, false, None);
                        }
                    }

                    if let Some((mut targets, unknown)) = self.ambient_regions(effects, Some(origin)) {
                        self.include_unknown_targets(&mut targets, unknown);

                        for target in targets {
                            if matches!(retirement, Some(BackingRetirement::Possible { .. })) && !self.unlocated_deallocation_may_reach(&target) {
                                continue;
                            }

                            self.invalidate_target(
                                &target,
                                kind,
                                invalidation,
                                false,
                                retirement,
                            );
                        }
                    } else {
                        self.invalidate_unknown_targets(invalidation, kind, retirement);
                    }
                }
                effect if effect.is_atomic() => self.invalidate(
                    &StorageInvalidation::unknown(),
                    DependencyMutationKind::Atomic,
                ),
                Effect::Unknown => {}
                Effect::Ambient | Effect::Atomic | Effect::UnboundedMutation { .. }
                | Effect::CapturedCleanup { .. } => {
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
        retirement: Option<BackingRetirement>,
    ) {
        if target.is_staged() {
            // Private outputs already describe the resulting values. A backing
            // retirement still ends the lifetime of the affected result regions.
            if let Some(retirement) = retirement {
                let occurrence = match retirement {
                    BackingRetirement::Performed(origin) | BackingRetirement::DuringCleanup(origin)
                    | BackingRetirement::Possible { origin, .. } => origin,
                };

                let slots = &self.storage_slots;
                let staged = &mut self.pending.get_mut(&occurrence).expect("private retirement retains its output").payloads;

                if matches!(retirement, BackingRetirement::Performed(_)) {
                    staged.entry(target.clone()).or_default();
                }

                for (reached, contents) in staged.iter_mut().filter(|(reached, _)| {
                    target.relationship_in_slots(reached, slots) != bray_bound_tree::StorageRelationship::Disjoint
                }) {
                    if let Some(origin) = match retirement {
                        BackingRetirement::Performed(origin) => Some(origin),
                        _ => retirement.possible_origin(reached, contents),
                    } {
                        contents.retire(origin);
                    }
                }
            }

            return;
        }

        let references = AddressValue::one(target.clone());

        for affected in self.affected_targets(&references) {
            if matches!(retirement, Some(BackingRetirement::Possible { .. })) && !self.unlocated_deallocation_may_reach(&affected) {
                continue;
            }

            // Atomic effects change protected payload observations, not ordinary
            // reference slots that merely may alias the atomic's address.
            if kind != DependencyMutationKind::Atomic
                && !Self::stable_reference_slot(&affected, invalidation)
                && !self.values.get(&affected).is_some_and(|value| {
                    value.is_no_payload() && stable_no_payload(&affected, invalidation)
                })
            {
                if complete_cell && target.is_exact() && *target == affected {
                    Arc::make_mut(&mut self.values).remove(&affected);
                } else if let Some(value) = Arc::make_mut(&mut self.values).get_mut(&affected) {
                    invalidate_value(value, affected == *target || !affected.contains_storage(target));
                }
            }

            if let Some(BackingRetirement::Performed(origin)) = retirement {
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

                if let Some(origin) = retirement.and_then(|retirement| retirement.possible_origin(&affected, contents)) {
                    contents.retire(origin);
                }
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
                && !self.values.get(&slot).is_some_and(|value| {
                    value.is_no_payload() && stable_no_payload(&slot, invalidation)
                })
                && let Some(value) = Arc::make_mut(&mut self.values).get_mut(&slot)
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
        self.invalidate_unknown_targets(invalidation, kind, None);
    }

    fn invalidate_unknown_targets(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
        retirement: Option<BackingRetirement>,
    ) {
        // An opaque operation can change the referent without replacing a reference
        // stored in an unexposed owner or reached through a protected shared entry borrow.
        // The established observation identifies the slot independently of its referent.
        let unaddressed = &self.unaddressed_storage;
        let owned = &self.owned_storage;

        let unaffected = |target: &Target| matches!(retirement, Some(BackingRetirement::Possible { .. }))
            && !Self::deallocation_may_reach(owned, target);

        let unknown = matches!(
            invalidation,
            StorageInvalidation::All { .. }
        );

        Arc::make_mut(&mut self.values).retain(|target, value| {
            let retained = kind == DependencyMutationKind::Atomic
                || unaffected(target)
                || unknown && Self::unaddressed_inline_slot(unaddressed, target)
                || Self::stable_reference_slot(target, invalidation)
                || value.is_no_payload() && stable_no_payload(target, invalidation)
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
                | Origin::BackingRegion
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

                if let Some(origin) = retirement.and_then(|retirement| retirement.possible_origin(target, contents)) {
                    contents.retire(origin);
                }
            }
        }
    }

}

fn invalidate_value(value: &mut super::super::model::ValueBinding, replaces_backing: bool) {
    if value.is_no_payload() {
        value.lose_information();
        return;
    }

    value.clear_address();
    value.completeness.lose_completeness();

    if replaces_backing {
        value.forget_owned_backing();
    }
}

fn stable_no_payload(target: &Target, invalidation: &StorageInvalidation) -> bool {
    let Some(path) = target.storage_path() else { return false; };

    let Some(index) = path.projections.iter().rposition(|projection| matches!(projection,
        bray_bound_tree::StorageProjection::NullableValue
            | bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }))
    else { return false; };

    let tag = Place { root: path.root, projections: path.projections[..index].into() };

    !invalidation.invalidates_discriminant(&tag, true)
}
