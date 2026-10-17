use super::super::model::{Contents, Effect, Origin, Place, References, State, Target};
use super::slots::take_subtree;
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageIdentityId};
use bray_symbols::TypeId;
use std::collections::BTreeSet;

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
        self.deferred_effects
            .insert(call, effects.map(|effects| effects.into_iter().collect()));
    }

    pub(in crate::analysis::storage_flow) fn deferred_effects(
        &self,
        call: bray_bound_tree::BoundExecutionSite,
    ) -> Option<Vec<Effect>> {
        self.deferred_effects
            .get(&call)?
            .as_ref()
            .map(|effects| effects.iter().cloned().collect())
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
            };

            previous.merge(&staged);
            staged = previous;
        }

        pending.values.extend(staged.values);
        pending.payloads.extend(staged.payloads);
    }

    pub(in crate::analysis::storage_flow) fn stage_returned_references(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        references: impl IntoIterator<Item = (Place, TypeId)>,
        inputs: impl IntoIterator<Item = Target>,
    ) {
        // An inconclusive forwarded reference still has the ordinary call's reachability
        // bounds. Only a complete relation replaces those bounds with a precise target.
        let references = references
            .into_iter()
            .filter(|(destination, _)| {
                !self.pending.get(&call).is_some_and(|pending| {
                    pending
                        .values
                        .get(&Target::storage(destination.clone()))
                        .is_some_and(|references| !references.unknown)
                })
            })
            .collect::<Vec<_>>();

        if references.is_empty() {
            return;
        }

        let mut roots = BTreeSet::new();

        for target in self.reachable_targets(inputs) {
            match target.origin {
                Origin::Storage(path) => {
                    roots.insert(path.root);
                }
                Origin::Returned { reference, .. } => {
                    roots.extend(
                        self.returned_roots
                            .get(&reference)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                }
                Origin::Entry { .. } | Origin::Allocation(_) => {}
            }
        }

        for (destination, ty) in references {
            let slot = Target::storage(destination.clone());

            // This finite union also bounds older executions of a call site at loop joins.
            self.returned_roots
                .entry(destination.clone())
                .or_default()
                .extend(roots.iter().copied());

            let target = Target {
                previous: false,
                origin: Origin::Returned {
                    reference: destination,
                    projections: std::sync::Arc::from([]),
                },
            };

            self.retire(&target);

            self.contents
                .entry(target.clone())
                .or_default()
                .storage_type = Some(ty);

            self.pending
                .entry(call)
                .or_default()
                .values
                .insert(slot, References::one(target));
        }
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

    pub(in crate::analysis::storage_flow) fn stage_null(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        let pending = self.pending.entry(call).or_default();

        take_subtree(&mut pending.values, &destination).for_each(drop);
        take_subtree(&mut pending.payloads, &destination).for_each(drop);
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

    pub(in crate::analysis::storage_flow) fn complete_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.deferred_effects.remove(&call);

        if let Some(transfer) = self.pending.remove(&call) {
            for slot in transfer.values.keys() {
                self.clear_value(slot);
            }

            self.commit_value(transfer);
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate_target(
        &mut self,
        target: &Target,
        kind: bray_symbols::DependencyMutationKind,
    ) {
        let references = References::one(target.clone());

        for affected in self.affected_targets(&references) {
            // Atomic effects change protected payload observations, not ordinary
            // reference slots that merely may alias the atomic's address.
            if kind != bray_symbols::DependencyMutationKind::Atomic {
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
        let unexposed = match invalidation {
            bray_bound_tree::StorageInvalidation::All {
                stable_descriptors, ..
            } => stable_descriptors
                .iter()
                .filter(|path| path.projections.is_empty())
                .map(|path| path.root)
                .collect::<std::collections::BTreeSet<_>>(),
            _ => Default::default(),
        };

        // An opaque operation can change the referent without replacing a reference
        // stored in an invocation-owned value whose address was never available to it.
        self.values.retain(|target, _| {
            kind == bray_symbols::DependencyMutationKind::Atomic
                || match target.storage_path() {
                    Some(slot) => {
                        unexposed.contains(&slot.root)
                            && !slot
                                .projections
                                .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                            || !invalidation.invalidates_path(slot, storage)
                    }
                    None => !matches!(
                        invalidation,
                        bray_bound_tree::StorageInvalidation::All { .. }
                    ),
                }
        });

        for (target, contents) in &mut self.contents {
            let affected = match &target.origin {
                Origin::Storage(path) => invalidation.invalidates_path(path, storage),
                Origin::Entry { .. } | Origin::Returned { .. } | Origin::Allocation(_) => {
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
    ) {
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
            .insert(Target::storage(slot), References::one(current));
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

    pub(in crate::analysis::storage_flow) fn owned_region(
        &mut self,
        owner: Target,
        destination: Place,
        ty: TypeId,
    ) {
        let region = owner.project(&[bray_bound_tree::StorageProjection::OwnedTarget]);

        // Owner moves carry the same region independently of the header's later address.
        // This identifies storage without claiming an initialized value or extent.
        let references = self
            .values
            .entry(owner)
            .or_insert_with(|| References::one(region))
            .clone();

        for target in &references.targets {
            self.contents
                .entry(target.clone())
                .or_default()
                .storage_type
                .get_or_insert(ty);
        }

        self.values.insert(Target::storage(destination), references);
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

        for references in self
            .values
            .values_mut()
            .chain(self.borrows.values_mut())
            .chain(
                self.pending
                    .values_mut()
                    .flat_map(|transfer| transfer.values.values_mut()),
            )
        {
            references.retire(current);
        }

        for effects in self.deferred_effects.values_mut().flatten() {
            *effects = std::mem::take(effects)
                .into_iter()
                .map(|mut effect| {
                    if let Effect::Mutation(target, _) = &mut effect
                        && current.contains(target)
                    {
                        target.previous = true;
                    }

                    effect
                })
                .collect();
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
