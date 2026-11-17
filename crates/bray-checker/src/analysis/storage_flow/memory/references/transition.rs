use super::super::model::{Contents, DependencyCompleteness, Origin, Place, AddressValue, Retirement, State, Target, Transfer, ValueBinding};
use super::slots::{storage_subtree, take_subtree};
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageIdentityId};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn begin_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.pending.insert(call, Default::default());
    }

    pub(in crate::analysis::storage_flow) fn capture_reference_inputs(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        roots: impl IntoIterator<Item = Target>,
    ) {
        let inputs = &mut self.pending.get_mut(&call)
            .expect("prepared invocation retains its input transfer").inputs;

        for root in roots {
            inputs.extend(storage_subtree(&self.values, &root)
                .map(|(slot, references)| (slot.clone(), references.clone())));
        }
    }

    pub(in crate::analysis::storage_flow) fn defer_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        effects: Option<super::super::EffectOutcomes>,
    ) {
        self.pending.entry(call).or_default().deferred_effects = effects;
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
                let binding = self
                    .pending
                    .get(&call)
                    .and_then(|pending| pending.values.get(&Target::storage(destination.clone())));

                if binding.is_some_and(ValueBinding::is_no_payload) { return None; }

                let forwarded = binding.and_then(|value| value.address.as_ref());

                if forwarded.is_some_and(|references| {
                    !references.is_unknown()
                        && (references.is_displaced()
                            || !references.is_bounded())
                }) {
                    return None;
                }

                Some((destination, ty, binding.and_then(|value| value.reference_regions()),
                    forwarded.is_some_and(AddressValue::has_empty_alternative)))
            })
            .collect::<Vec<_>>();

        let opaque_bounds = references
            .iter()
            .any(|(_, _, bounds, _)| bounds.is_none())
            .then(|| self.unbounded_reference_regions(self.transitive_regions(inputs, |_, _| true).0));

        for (destination, ty, forwarded, nullable) in references {
            let bounds = match forwarded {
                Some(regions) => self.expand_returned_regions(regions, self.pending.get(&call)),
                None => {
                    // Each output retains independent bounds across later joins and retirement.
                    opaque_bounds
                        .as_ref()
                        .expect("opaque results prepare their input bounds")
                        .clone()
                }
            };

            if let Some(pending) = self.pending.get_mut(&call) {
                if let Some(value) = pending.values.get_mut(&Target::storage(destination.clone())) {
                    value.clear_address();
                }
            }

            self.stage_bounded_reference(
                call,
                Target::storage(destination.clone()),
                destination.clone(),
                ty,
                bounds,
            );

            if nullable { self.stage_empty_reference(call, Target::storage(destination)); }
        }
    }

    fn expand_returned_regions(
        &self,
        regions: impl IntoIterator<Item = Target>,
        staged: Option<&Transfer>,
    ) -> AddressValue {
        let mut bounds = AddressValue::default();

        for target in regions {
            match &target.origin {
                Origin::Returned { reference, projections } => {
                    // Output alternatives belong to their pending producer. Ordinary
                    // inputs and historical aliases retain their committed bounds.
                    let returned = staged.filter(|_| !target.previous)
                        .and_then(|transfer| transfer.returned_inputs.get(reference))
                        .unwrap_or_else(|| self.returned_reference_bounds(reference, target.previous));

                    bounds.merge(returned);

                    if target.previous || projections.contains(&bray_bound_tree::StorageProjection::OwnedTarget) {
                        bounds.forget_value();
                    }
                }
                _ => {
                    bounds.targets.insert(target);
                }
            }
        }

        bounds
    }

    fn unbounded_reference_regions(&self, inputs: impl IntoIterator<Item = Target>) -> AddressValue {
        // An external result can reach inline storage already published at its
        // introduction. Later publication cannot change an older reference's origins.
        let exposed = self.contents.iter().filter_map(|(target, contents)| {
            (!target.previous
                && contents.escaped
                && target
                    .storage_path()
                    .is_some_and(|path| self.is_owned_inline_storage(path)))
            .then(|| target.clone())
        });

        let mut bounds = self.expand_returned_regions(inputs.into_iter().chain(exposed), None);

        bounds.forget_value();

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

    pub(in crate::analysis::storage_flow) fn stage_reference_regions(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        reference: Place,
        ty: TypeId,
        inputs: impl IntoIterator<Item = Target>,
    ) {
        if let Some(parent) = super::slots::selector_parent(&destination) {
            let affected = super::slots::selected_subtree(&self.values, &destination)
                .map(|(slot, _)| slot.clone()).collect::<Vec<_>>();

            let affected_payloads = super::slots::selected_subtree(&self.contents, &destination)
                .map(|(slot, _)| slot.clone()).collect::<Vec<_>>();

            let exact = self.pending.get(&call).into_iter().flat_map(|pending| pending.values.iter())
                .filter(|(_, value)| value.reference().is_some_and(AddressValue::is_exact))
                .map(|(slot, _)| slot.clone()).collect::<std::collections::BTreeSet<_>>();

            let established_payloads = self.pending.get(&call).into_iter().flat_map(|pending| pending.payloads.keys())
                .cloned().collect::<std::collections::BTreeSet<_>>();

            self.stage_preserved_subtree(call, &parent);

            let pending = self.pending.get_mut(&call).expect("preserved selection stages its transition");

            for slot in affected {
                if !exact.contains(&slot)
                    && let Some(references) = pending.values.get_mut(&slot).and_then(|value| value.address.as_mut())
                {
                    references.bound();
                }
            }

            for slot in affected_payloads {
                if !established_payloads.contains(&slot)
                    && let Some(contents) = pending.payloads.get_mut(&slot)
                {
                    contents.initialized.clear();
                }
            }
        }

        let mut bounds = self.expand_returned_regions(inputs, None);

        // Checked regions are closed physical bounds, not forwarded addresses.
        bounds.bound();

        self.stage_bounded_reference(call, destination, reference, ty, bounds);
    }

    pub(in crate::analysis::storage_flow) fn observe_reference<C: crate::CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        destination: Target,
        reference: Place,
        ty: TypeId,
        inputs: impl IntoIterator<Item = Target>,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        if self.values.get(&destination).is_some_and(ValueBinding::is_no_payload) { return Ok(()); }

        // A bounded view reads its existing field values. Its container is not
        // an origin of the pointers stored in those fields.
        if let Some(contents) = self.bounded_reference_contents(request, &destination)? {
            *self.values.entry(destination).or_default().produced_address() = contents;

            return Ok(());
        }

        let bounds = self.unbounded_reference_regions(inputs);
        let target = Self::returned_reference(reference.clone());

        // A new read after replacement must not equal a pointer retained from the old value.
        self.retire_region(&target, Retirement::Producer);
        self.returned_inputs.insert((reference, false), bounds);

        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type = Some(ty);

        *self.values.entry(destination).or_default().produced_address() = AddressValue::one(target);

        Ok(())
    }

    fn returned_reference(reference: Place) -> Target {
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
        bounds: AddressValue,
    ) {
        let target = Self::returned_reference(reference.clone());

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
            .produced_address()
            .merge(&AddressValue::one(target));
    }

    pub(in crate::analysis::storage_flow) fn capture_inputs(
        &mut self,
        destination: Place,
        captures: impl IntoIterator<Item = Target>,
    ) {
        // Opaque frames retain reachable inputs, without being references to those inputs.
        self.values.entry(Target::storage(destination)).or_default().capture_inputs(captures);
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
            .and_then(|value| value.address.as_mut())
        {
            references.bound();
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_empty_reference(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        // Empty alternatives contribute no target and cannot erase another origin.
        self.pending.entry(call).or_default().values.entry(destination).or_default()
            .produced_address().nullable = true;
    }

    pub(in crate::analysis::storage_flow) fn stage_unresolved_value<'a>(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        complete: bool,
        references: impl IntoIterator<Item = &'a [bray_bound_tree::StorageProjection]>,
    ) {
        // Unknown replacement carries no previous contents or reference bounds.
        // Incomplete generic layouts retain whole-value uncertainty separately.
        let root = self.pending.entry(call).or_default().values.entry(destination.clone()).or_default();

        root.make_present();
        root.completeness = if complete { DependencyCompleteness::Untracked } else { DependencyCompleteness::Partial };

        for path in references {
            self.stage_unknown(call, destination.project(path));
        }
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
            .produced_address().forget_value();

        for (_, contents) in pending
            .payloads
            .range_mut(destination.clone()..)
            .take_while(|(target, _)| destination.contains(target))
            .filter(|(target, _)| destination.contains_storage(target))
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
            .produced_address()
            .merge(references);
    }

    pub(in crate::analysis::storage_flow) fn stage_address(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: Target,
        destination: Target,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        let relation = reference.relation();
        let ty = reference.addressed_type().expect("address formation retains its checked target type");

        let mut references = if relation.is_exact() {
            self.addressed_target(source, ty, relation.kind())
        } else {
            // A widened target bounds an allocation, not one declared cell.
            // Retain exposure without assigning the bound's view type to its storage.
            self.contents.entry(source.clone()).or_default().exposed |=
                relation.kind() == bray_symbols::DependencyReferenceKind::Address;

            AddressValue::one(source)
        };

        if !relation.is_exact() { references.bound(); }

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .produced_address()
            .merge(&references);
    }

    pub(in crate::analysis::storage_flow) fn capture_failure_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        effects: super::super::CapturedEffects,
    ) {
        self.pending.entry(call).or_default().failure_effects = Some(std::sync::Arc::new(effects));
    }

    pub(in crate::analysis::storage_flow) fn take_failure_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) -> Option<std::sync::Arc<super::super::CapturedEffects>> {
        self.pending.get_mut(&call)?.failure_effects.take()
    }

    pub(in crate::analysis::storage_flow) fn weaken_transition(&mut self, call: bray_bound_tree::BoundExecutionSite) {
        self.pending.get_mut(&call).expect("a weak assignment retains its captured transition").weak_outputs = true;
    }

    pub(in crate::analysis::storage_flow) fn produce_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        if let Some(transfer) = self.pending.get_mut(&call).map(Transfer::take_outputs) {
            if transfer.weak_outputs {
                self.commit_weak_value(transfer);
                return;
            }

            for slot in transfer.values.keys() {
                self.clear_value(slot);
            }

            self.commit_value(transfer);
        }
    }

    pub(in crate::analysis::storage_flow) fn complete_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.produce_transition(call);
        self.pending.remove(&call);
    }

    pub(in crate::analysis::storage_flow) fn fresh(
        &mut self,
        slot: Place,
        allocation: BoundExpressionId,
    ) -> Target {
        // A new execution cannot equate old aliases with this incarnation.
        let current = Target {
            origin: Origin::Allocation { allocation: slot.clone(), projections: [].into() },
            previous: false,
        };

        self.retire_region(&current, Retirement::Producer);

        let contents = Contents {
            allocations: [allocation].into_iter().collect(),
            ..Contents::default()
        };

        self.contents.insert(current.clone(), contents);

        self.values
            .insert(Target::storage(slot), AddressValue::one(current.clone()).into());

        current
    }

    pub(in crate::analysis::storage_flow) fn stage_allocation_reference(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
        allocation: Place,
        projections: &[bray_bound_tree::StorageProjection],
    ) {
        let target = Target { previous: false,
            origin: Origin::Allocation { allocation, projections: [].into() } };

        let pending = self.pending.entry(call).or_default();

        pending.payloads.entry(target.clone()).or_default().allocations.extend(call.expression());

        pending.values.entry(destination).or_default()
            .produced_address()
            .merge(&AddressValue::one(target.project(projections)));
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

    fn owned_value(
        &self,
        owner: &Target,
        pointer: bray_bound_tree::StorageProjection,
    ) -> Option<&ValueBinding> {
        self.values
            .get(&owner.project(&[pointer]))
            .filter(|value| value.reference().is_some())
            .or_else(|| self.values.get(owner).filter(|value| value.reference().is_some_and(|references| !references.is_unknown())))
    }

    pub(in crate::analysis::storage_flow) fn stage_owned_buffer_reference(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        owner: Target,
        destination: Target,
        reference: Place,
        element: TypeId,
    ) {
        // A produced owning descriptor names its backing region, just as an
        // owning formal does. This declares neither freshness nor initialization.
        let backing = owner.project(&[bray_bound_tree::StorageProjection::OwnedTarget]);

        self.stage_bounded_reference(call, destination, reference, element, AddressValue::one(backing));
    }

    pub(in crate::analysis::storage_flow) fn declare_owned_buffer(
        &mut self,
        owner: Target,
        pointer: Target,
        element: TypeId,
    ) {
        if [&owner, &pointer].into_iter().any(|slot| self.values.get(slot).is_some_and(ValueBinding::is_no_payload)) {
            return;
        }

        if self.values.get(&pointer).is_some_and(|value| value.reference().is_some()) {
            return;
        }

        if let Some(recorded) = self.values.get(&owner).and_then(|value| value.address.clone()) {
            *self.values.entry(pointer).or_default().produced_address() = recorded;

            return;
        }

        // A complete owning descriptor supplies a region, not a fresh allocation.
        // Later moves retain this pointer record without deriving another origin.
        let backing = owner.project(&[bray_bound_tree::StorageProjection::OwnedTarget]);

        let declared = self.contents
            .entry(backing.clone())
            .or_default()
            .storage_type.get_or_insert(element);

        assert_eq!(*declared, element, "an owned buffer keeps its declared backing element type");

        *self.values.entry(pointer).or_default().produced_address() = AddressValue::one(backing);
    }

    pub(in crate::analysis::storage_flow) fn owned_regions(
        &self,
        owner: &Target,
        pointer: bray_bound_tree::StorageProjection,
    ) -> Option<Vec<Target>> {
        self.owned_value(owner, pointer)?.reference()?.regions()
    }

    pub(in crate::analysis::storage_flow) fn owned_mutation_targets(
        &self,
        owner: &super::super::MutationTarget,
        pointer: bray_bound_tree::StorageProjection,
    ) -> Option<Vec<super::super::MutationTarget>> {
        let mut targets = self.owned_value(owner.footprint(), pointer)?.mutation_targets()?;

        if owner.address.is_none() {
            for target in &mut targets { target.address = None; }
        }

        Some(targets)
    }

    pub(in crate::analysis::storage_flow) fn owned_region(
        &mut self,
        owners: &[Target],
        destination: Target,
        ty: TypeId,
        pointer: bray_bound_tree::StorageProjection,
        inexact: bool,
    ) {
        let mut references = AddressValue::default();

        if inexact { references.bound(); }

        for owner in owners {
            if let Some(owned) = self.owned_value(owner, pointer).and_then(ValueBinding::reference) {
                references.merge(owned);
            } else {
                references.forget_value();
            }
        }

        for target in &references.targets {
            self.contents
                .entry(target.clone())
                .or_default()
                .storage_type
                .get_or_insert(ty);
        }

        self.values.insert(destination, references.into());
    }

    pub(in crate::analysis::storage_flow) fn point_at(
        &mut self,
        target: Target,
        destination: Place,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) {
        let references = self.addressed_target(target, ty, kind);

        self.replace_value(destination, references.into());
    }

    pub(super) fn retire(&mut self, current: &Target) {
        self.retire_region(current, Retirement::Storage);
    }

    pub(in crate::analysis::storage_flow::memory) fn retire_region(
        &mut self,
        current: &Target,
        retirement: Retirement,
    ) {
        for bounds in self.returned_inputs.values_mut() {
            bounds.retire(current, retirement);
        }

        if !self
            .contents
            .range(current.clone()..)
            .take_while(|(target, _)| current.contains(target))
            .any(|(target, _)| retirement.contains(current, target))
            && !self.values.range(current.clone()..).take_while(|(target, _)| current.contains(target))
                .any(|(target, value)| retirement.contains(current, target) && value.whole_sources().is_some())
        {
            return;
        }

        for references in self.values.values_mut().flat_map(|value| value.references_mut())
            .chain(self.borrows.values_mut()) {
            references.retire(current, retirement);
        }

        for transfer in self.pending.values_mut() {
            transfer.retire_dependencies(current, retirement);
        }

        for entries in self.deferred.values_mut() {
            for transfer in entries.values_mut() {
                if transfer.references(current, retirement) {
                    std::sync::Arc::make_mut(transfer).retire_dependencies(current, retirement);
                }
            }
        }

        if let Origin::Returned { reference, projections } = &current.origin
            && projections.is_empty()
            && let Some(bounds) = self.returned_inputs.remove(&(reference.clone(), false)) {
            self.returned_inputs.entry((reference.clone(), true)).or_default().merge(&bounds);
        }

        let values = take_subtree(&mut self.values, current, retirement).collect::<Vec<_>>();

        for (target, mut value) in values {
            for references in value.references_mut() {
                references.forget_value();
            }

            value.completeness.lose_completeness();

            self.values
                .entry(Target {
                    previous: true,
                    ..target
                })
                .or_default()
                .merge(&value);
        }

        let contents = take_subtree(&mut self.contents, current, retirement).collect::<Vec<_>>();

        for (target, mut contents) in contents {
            // Older instances share a summary target, so no definite contents survive.
            contents.initialized.clear();

            let previous = Target {
                previous: true,
                ..target
            };

            self.contents
                .entry(previous.clone())
                .or_default()
                .merge(&contents, &previous);
        }
    }

    pub(in crate::analysis::storage_flow) fn begin(&mut self, identity: StorageIdentityId) {
        // Descriptor storage ends independently of separately allocated backing.
        self.retire(&Target::storage(Place::root(identity)));
    }
}
