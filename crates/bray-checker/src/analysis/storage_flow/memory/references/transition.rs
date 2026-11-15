use super::super::model::{Contents, Effect, Origin, Place, References, State, Target};
use super::slots::take_subtree;
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageIdentityId};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn null(&mut self, destination: Place) {
        self.clear(&destination);

        self.values
            .insert(Target::storage(destination), References::default());
    }

    pub(in crate::analysis::storage_flow) fn begin_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.pending.insert(call, Default::default());
    }

    pub(in crate::analysis::storage_flow) fn defer_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        effects: Option<Vec<Effect>>,
    ) {
        self.pending.entry(call).or_default().deferred_effects =
            effects.map(|effects| effects.into_iter().collect());
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn stage_slot(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &Place,
        destination: Place,
    ) {
        self.stage_value(
            call,
            &Target::storage(source.clone()),
            Target::storage(destination),
        );
    }

    pub(in crate::analysis::storage_flow) fn stage_value(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &Target,
        destination: Target,
    ) {
        let mut staged = self.capture_value(source, &destination);

        staged
            .values
            .entry(destination.clone())
            .or_insert_with(|| References {
                unknown: true,
                ..References::default()
            });

        let pending = self.pending.entry(call).or_default();

        if pending.values.contains_key(&destination) {
            // Alternative sources for one output must agree on its initialized payloads.
            let mut previous = super::super::model::Transfer {
                values: take_subtree(&mut pending.values, &destination).collect(),
                payloads: take_subtree(&mut pending.payloads, &destination).collect(),
                deferred: take_subtree(&mut pending.deferred, &destination).collect(),
                ..Default::default()
            };

            previous.merge(&staged);
            staged = previous;
        }

        pending.values.extend(staged.values);
        pending.payloads.extend(staged.payloads);
        pending.deferred.extend(staged.deferred);
        pending.returned_inputs.extend(staged.returned_inputs);
    }

    pub(in crate::analysis::storage_flow) fn stage_returned_references(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        references: impl IntoIterator<Item = (Place, TypeId)>,
        inputs: impl IntoIterator<Item = Target>,
    ) {
        // Established pointer values and known absence need no symbolic result.
        // Preserve displacement as well as exact identity. Opaque values receive
        // region bounds without asserting an input address.
        let references = references
            .into_iter()
            .filter_map(|(destination, ty)| {
                let forwarded = self
                    .pending
                    .get(&call)
                    .and_then(|pending| pending.values.get(&Target::storage(destination.clone())));

                if forwarded.is_some_and(|references| {
                    !references.unknown
                        && (references.displaced
                            || references.targets.is_empty()
                            || references.is_exact())
                }) {
                    return None;
                }

                Some((destination, ty, forwarded.and_then(References::regions)))
            })
            .collect::<Vec<_>>();

        let opaque_bounds = references
            .iter()
            .any(|(_, _, bounds)| bounds.is_none())
            .then(|| {
                self.unbounded_reference_regions(self.transitive_regions(inputs, |_| true).0)
            });

        for (destination, ty, forwarded) in references {
            let bounds = match forwarded {
                Some(regions) => self.expand_returned_regions(regions),
                None => {
                    // Each output retains independent bounds across later joins and retirement.
                    opaque_bounds
                        .as_ref()
                        .expect("opaque results prepare their input bounds")
                        .clone()
                }
            };

            if let Some(pending) = self.pending.get_mut(&call) {
                pending.values.remove(&Target::storage(destination.clone()));
            }

            self.stage_bounded_reference(
                call,
                Target::storage(destination.clone()),
                destination,
                ty,
                bounds,
            );
        }
    }

    fn expand_returned_regions(&self, regions: impl IntoIterator<Item = Target>) -> References {
        let mut bounds = References::default();

        for target in regions {
            match &target.origin {
                Origin::Returned { reference, .. } => {
                    bounds.merge(self.returned_reference_bounds(reference));
                }
                _ => {
                    bounds.targets.insert(target);
                }
            }
        }

        bounds
    }

    fn unbounded_reference_regions(&self, inputs: impl IntoIterator<Item = Target>) -> References {
        // An external result can reach inline storage already published at its
        // introduction. Later publication cannot change an older reference's origins.
        let exposed = self.contents.iter().filter_map(|(target, contents)| {
            (!target.previous && contents.escaped && target.storage_path().is_some_and(|path| self.is_owned_inline_storage(path)))
                .then(|| target.clone())
        });

        let mut bounds = self.expand_returned_regions(inputs.into_iter().chain(exposed));

        bounds.unknown = true;

        bounds
    }

    pub(in crate::analysis::storage_flow) fn stage_returned_reference(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        reference: Place,
        ty: TypeId,
        inputs: impl IntoIterator<Item = Target>,
    ) {
        // Store the established input regions, never another returned identity.
        // Repeated writes can otherwise make an output its own region bound.
        let bounds = self.unbounded_reference_regions(inputs);

        self.stage_bounded_reference(call, destination, reference, ty, bounds);
    }

    pub(in crate::analysis::storage_flow) fn observe_reference(
        &mut self,
        destination: Target,
        reference: Place,
        ty: TypeId,
        inputs: impl IntoIterator<Item = Target>,
    ) {
        let bounds = self.unbounded_reference_regions(inputs);

        let target = self.returned_reference(reference, &bounds);

        // A new read after replacement must not equal a pointer retained from the old value.
        self.retire(&target);

        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type = Some(ty);

        self.values.insert(destination, References::one(target));
    }

    fn returned_reference(&mut self, reference: Place, bounds: &References) -> Target {
        self.returned_inputs
            .entry(reference.clone())
            .or_default()
            .merge(bounds);

        Target {
            previous: false,
            origin: Origin::Returned {
                reference,
                projections: std::sync::Arc::from([]),
            },
        }
    }

    fn stage_bounded_reference(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        reference: Place,
        ty: TypeId,
        bounds: References,
    ) {
        let target = self.returned_reference(reference.clone(), &bounds);

        self.pending
            .entry(call)
            .or_default()
            .returned_inputs
            .entry(reference.clone())
            .or_default()
            .merge(&bounds);

        self.pending
            .entry(call)
            .or_default()
            .payloads
            .entry(target.clone())
            .or_default()
            .storage_type = Some(ty);

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .merge(&References::one(target));
    }

    pub(in crate::analysis::storage_flow) fn capture_inputs(
        &mut self,
        destination: Place,
        captures: impl IntoIterator<Item = Target>,
    ) {
        // Opaque frames retain reachable inputs, without being references to those inputs.
        let references = self.values.entry(Target::storage(destination)).or_default();

        references.unknown = true;
        references.targets.extend(captures);
    }

    pub(in crate::analysis::storage_flow) fn bound_staged_reference(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: &Target,
    ) {
        if let Some(references) = self
            .pending
            .get_mut(&call)
            .and_then(|pending| pending.values.get_mut(destination))
        {
            references.inexact = true;
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_null(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        let pending = self.pending.entry(call).or_default();

        take_subtree(&mut pending.values, &destination).for_each(drop);
        take_subtree(&mut pending.payloads, &destination).for_each(drop);
        take_subtree(&mut pending.deferred, &destination).for_each(drop);
        pending.values.insert(destination, References::default());
    }

    pub(in crate::analysis::storage_flow) fn stage_unknown(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        let pending = self.pending.entry(call).or_default();

        pending
            .values
            .entry(destination.clone())
            .or_default()
            .unknown = true;

        for (_, contents) in pending
            .payloads
            .range_mut(destination.clone()..)
            .take_while(|(target, _)| destination.contains(target))
        {
            contents.initialized.clear();
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_borrow(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: BorrowCapabilityId,
        destination: Target,
    ) {
        let Some(references) = self.borrows.get(&source) else {
            return;
        };

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .merge(references);
    }

    pub(in crate::analysis::storage_flow) fn stage_address(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: Target,
        destination: Target,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) {
        let references = self.addressed_target(source, ty, kind);

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .merge(&references);
    }

    pub(in crate::analysis::storage_flow) fn capture_failure_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        effects: super::super::model::CapturedEffects,
    ) {
        self.pending.entry(call).or_default().failure_effects = Some(std::sync::Arc::new(effects));
    }

    pub(in crate::analysis::storage_flow) fn take_failure_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) -> Option<std::sync::Arc<super::super::model::CapturedEffects>> {
        self.pending.get_mut(&call)?.failure_effects.take()
    }

    pub(in crate::analysis::storage_flow) fn complete_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        if let Some(transfer) = self.pending.remove(&call) {
            for slot in transfer.values.keys() {
                self.clear_value(slot);
            }

            self.commit_value(transfer);
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn stable_reference_slot(
        target: &Target,
        invalidation: &bray_bound_tree::StorageInvalidation,
    ) -> bool {
        let descriptors = match invalidation {
            bray_bound_tree::StorageInvalidation::All {
                stable_descriptors, ..
            }
            | bray_bound_tree::StorageInvalidation::Observed {
                stable_descriptors, ..
            } => stable_descriptors,
            bray_bound_tree::StorageInvalidation::Paths(_) => return false,
        };

        // A retained whole descriptor protects its inline reference values. It does
        // not protect the storage reached through those references or owned backing.
        target.observation_path().is_some_and(|slot| {
            !slot
                .projections
                .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                && descriptors.contains(&Place::root(slot.root))
        })
    }

    pub(in crate::analysis::storage_flow) fn invalidate_target(
        &mut self,
        target: &Target,
        kind: bray_symbols::DependencyMutationKind,
        invalidation: &bray_bound_tree::StorageInvalidation,
    ) {
        let references = References::one(target.clone());

        for affected in self.affected_targets(&references) {
            // Atomic effects change protected payload observations, not ordinary
            // reference slots that merely may alias the atomic's address.
            if kind != bray_symbols::DependencyMutationKind::Atomic
                && !Self::stable_reference_slot(&affected, invalidation)
            {
                self.values.remove(&affected);
            }

            if let Some(contents) = self.contents.get_mut(&affected) {
                contents.initialized.retain(|ty| {
                    matches!(
                        kind,
                        bray_symbols::DependencyMutationKind::Write
                            | bray_symbols::DependencyMutationKind::Clear
                            | bray_symbols::DependencyMutationKind::Replace(_)
                            | bray_symbols::DependencyMutationKind::Atomic
                    ) && contents.storage_type == Some(*ty)
                });
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate(
        &mut self,
        invalidation: &bray_bound_tree::StorageInvalidation,
        storage: &bray_bound_tree::StoragePlan,
        kind: bray_symbols::DependencyMutationKind,
    ) {
        // An opaque operation can change the referent without replacing a reference
        // stored in an unexposed owner or reached through a protected shared entry borrow.
        // The established observation identifies the slot independently of its referent.
        self.values.retain(|target, _| {
            kind == bray_symbols::DependencyMutationKind::Atomic
                || Self::stable_reference_slot(target, invalidation)
                || match target.observation_path() {
                    Some(slot) => !invalidation.invalidates_path(&slot, storage),
                    None => !matches!(
                        invalidation,
                        bray_bound_tree::StorageInvalidation::All { .. }
                    ),
                }
        });

        for (target, contents) in &mut self.contents {
            let affected = match &target.origin {
                Origin::Storage(path) => invalidation.invalidates_path(path, storage),
                Origin::Entry { .. } | Origin::Returned { .. } | Origin::Allocation(_)
                | Origin::Static { .. } => {
                    matches!(
                        invalidation,
                        bray_bound_tree::StorageInvalidation::All { .. }
                    )
                }
            };

            if affected {
                // A checked typed write preserves valid contents of the declared type.
                // Reinterpreted views and consumed contents have no such guarantee.
                contents.initialized.retain(|ty| {
                    matches!(
                        kind,
                        bray_symbols::DependencyMutationKind::Write
                            | bray_symbols::DependencyMutationKind::Clear
                            | bray_symbols::DependencyMutationKind::Replace(_)
                            | bray_symbols::DependencyMutationKind::Atomic
                    ) && contents.storage_type == Some(*ty)
                });
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn fresh(
        &mut self,
        slot: Place,
        allocation: BoundExpressionId,
    ) -> Target {
        // A new execution cannot equate old aliases with this incarnation.
        let current = Target {
            origin: Origin::Allocation(slot.clone()),
            previous: false,
        };

        self.retire(&current);

        let contents = Contents {
            allocations: [allocation].into_iter().collect(),
            ..Contents::default()
        };

        self.contents.insert(current.clone(), contents);

        self.values
            .insert(Target::storage(slot), References::one(current.clone()));

        current
    }

    pub(in crate::analysis::storage_flow) fn uninit(&mut self, target: Target, element: TypeId) {
        self.retire(&target);

        self.contents.insert(
            target,
            Contents {
                storage_type: Some(element),
                ..Contents::default()
            },
        );
    }

    fn owned_references(
        &self,
        owner: &Target,
        pointer: bray_bound_tree::StorageProjection,
    ) -> std::borrow::Cow<'_, References> {
        if let Some(references) = self.values.get(owner).filter(|references| !references.unknown) {
            return std::borrow::Cow::Borrowed(references);
        }

        let backing = owner.project(&[pointer]);
        let declared_backing = backing.observation_path();

        let pointer = self.values.get(&backing).filter(|references| {
            !references.unknown && (references.targets.is_empty() || !references.targets.iter().all(|target| {
                matches!(&target.origin, Origin::Entry { reference, layer: 1, projections }
                    if declared_backing.as_ref() == Some(reference) && projections.is_empty())
            }))
        });

        // An entry descriptor's own pointer snapshot denotes its owned region.
        // Returned and transferred pointers retain their established physical origins.
        match pointer {
            Some(references) => std::borrow::Cow::Borrowed(references),
            _ => std::borrow::Cow::Owned(References::one(
                owner.project(&[bray_bound_tree::StorageProjection::OwnedTarget]),
            )),
        }
    }

    pub(in crate::analysis::storage_flow) fn owned_regions(
        &self,
        owner: &Target,
        pointer: bray_bound_tree::StorageProjection,
    ) -> Option<Vec<Target>> {
        self.owned_references(owner, pointer).regions()
    }

    pub(in crate::analysis::storage_flow) fn owned_region(
        &mut self,
        owners: &[Target],
        destination: Target,
        ty: bray_symbols::TypeId,
        pointer: bray_bound_tree::StorageProjection,
        inexact: bool,
    ) {
        let mut references = References { inexact, ..References::default() };

        for owner in owners {
            let materialized = self.values.get(owner).is_some_and(|value| !value.unknown);
            let owned = self.owned_references(owner, pointer);

            references.merge(&owned);

            // Owners and their returned views retain independent records through moves.
            if !materialized {
                let owned = owned.into_owned();

                self.values.insert(owner.clone(), owned);
            }
        }

        for target in &references.targets {
            self.contents
                .entry(target.clone())
                .or_default()
                .storage_type
                .get_or_insert(ty);
        }

        self.values.insert(destination, references);
    }

    pub(in crate::analysis::storage_flow) fn point_at(
        &mut self,
        target: Target,
        destination: Place,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) {
        let references = self.addressed_target(target, ty, kind);

        self.clear(&destination);
        self.values.insert(Target::storage(destination), references);
    }

    pub(super) fn retire(&mut self, current: &Target) {
        if !self
            .contents
            .range(current.clone()..)
            .next()
            .is_some_and(|(target, _)| current.contains(target))
        {
            return;
        }

        for references in self.values.values_mut().chain(self.borrows.values_mut()) {
            references.retire(current);
        }

        for transfer in self.pending.values_mut() {
            transfer.retire_dependencies(current);
        }

        for entries in self.deferred.values_mut() {
            for transfer in entries.values_mut() {
                if transfer.references(current) {
                    std::sync::Arc::make_mut(transfer).retire_dependencies(current);
                }
            }
        }

        let values = take_subtree(&mut self.values, current).collect::<Vec<_>>();

        for (target, mut value) in values {
            value.unknown = true;

            self.values
                .entry(Target {
                    previous: true,
                    ..target
                })
                .or_default()
                .merge(&value);
        }

        let contents = take_subtree(&mut self.contents, current).collect::<Vec<_>>();

        for (target, mut contents) in contents {
            // Older instances share a summary target, so no definite contents survive.
            contents.initialized.clear();

            self.contents
                .entry(Target {
                    previous: true,
                    ..target
                })
                .or_default()
                .merge(&contents);
        }
    }

    pub(in crate::analysis::storage_flow) fn begin(&mut self, identity: StorageIdentityId) {
        // Retire a storage incarnation once, including all its addressed fields and payloads.
        self.retire(&Target::storage(Place::root(identity)));
    }
}
