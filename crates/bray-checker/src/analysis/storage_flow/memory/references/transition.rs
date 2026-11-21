use super::super::model::{Contents, DependencyCompleteness, Origin, Place, AddressValue, ReachedAccess, Retirement, State, Target, Transfer, ValueBinding};
use super::slots::take_subtree;
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageIdentityId};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn capture_assignment_destination(
        &mut self,
        assignment: BoundExpressionId,
        destination: Option<ReachedAccess>,
    ) {
        let destination = destination.unwrap_or_else(|| ReachedAccess {
            address: AddressValue::unknown(), loan: None,
        });

        self.assignment_destinations.insert(assignment, destination);
    }

    pub(in crate::analysis::storage_flow) fn assignment_destination(
        &self,
        assignment: BoundExpressionId,
    ) -> Option<&ReachedAccess> {
        self.assignment_destinations.get(&assignment)
    }

    pub(in crate::analysis::storage_flow) fn finish_assignment(&mut self, assignment: BoundExpressionId) {
        self.assignment_destinations.remove(&assignment);
    }

    pub(in crate::analysis::storage_flow) fn begin_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.stage_transition(call, Transfer::default());
    }

    pub(in crate::analysis::storage_flow) fn stage_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        transition: Transfer,
    ) {
        self.pending.insert(call, transition);
    }

    pub(in crate::analysis::storage_flow) fn take_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) -> Option<Transfer> {
        self.pending.remove(&call)
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
                    contents.clear_initialization();
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

    pub(in crate::analysis::storage_flow) fn capture_inputs<'capture>(
        &mut self,
        destination: Place,
        captures: impl IntoIterator<Item = &'capture bray_bound_tree::StorageCapturedValue>,
    ) {
        // Opaque frames retain reachable inputs, without being references to those inputs.
        let value = self.values.entry(Target::storage(destination)).or_default();

        value.capture_inputs([]);

        for capture in captures {
            value.capture_inputs(capture.carried_regions());

            for support in capture.dependency_support() {
                value.include_dependency_support(support);
            }
        }
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
            contents.clear_initialization();
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_borrow(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: BorrowCapabilityId,
        destination: Target,
    ) {
        let Some(references) = self.borrow_address(source).cloned() else {
            return;
        };

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .include_value(&ValueBinding::borrowed(references, source));
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

    pub(in crate::analysis::storage_flow) fn stage_exposed_borrow(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: super::Source,
        destination: Target,
    ) {
        let reached = match source {
            super::Source::Value(slot) => self.values.get(&slot).map(ValueBinding::exposed_address),
            super::Source::Borrow(loan) => self.borrow_address(loan)
                .map(|address| ValueBinding::from(address.clone()).exposed_address()),
            super::Source::Captured(value) => value.binding().map(ValueBinding::exposed_address),
        };

        let Some(reached) = reached else {
            self.stage_unknown(call, destination);
            return;
        };

        self.pending.entry(call).or_default().values.entry(destination).or_default().include_value(&reached);
    }

    pub(in crate::analysis::storage_flow) fn capture_failure_effects(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        outcome: (bray_symbols::DependencyFailureKind, Option<usize>),
        effects: std::sync::Arc<super::super::CapturedEffects>,
    ) {
        self.pending.entry(call).or_default().failure_effects.insert(outcome, effects);
    }

    pub(in crate::analysis::storage_flow) fn fail_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        outcome: (bray_symbols::DependencyFailureKind, Option<usize>),
        keep_outputs: bool,
    ) -> Option<std::sync::Arc<super::super::CapturedEffects>> {
        let mut effects = if keep_outputs {
            std::mem::take(&mut self.pending.get_mut(&call)?.failure_effects)
        } else {
            self.pending.remove(&call)?.failure_effects
        };

        if outcome.1.is_some() {
            return effects.remove(&outcome);
        }

        let mut prefixes = effects.into_iter()
            .filter_map(|((kind, _), effects)| (kind == outcome.0).then_some(effects));

        let mut possible = prefixes.next()?;

        for prefix in prefixes {
            if possible != prefix {
                std::sync::Arc::make_mut(&mut possible).merge(&prefix);
            }
        }

        Some(possible)
    }

    pub(in crate::analysis::storage_flow) fn weaken_transition(&mut self, call: bray_bound_tree::BoundExecutionSite) {
        self.pending.get_mut(&call).expect("a weak assignment retains its captured transition").weak_outputs = true;
    }

    pub(in crate::analysis::storage_flow) fn produce_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        if let Some(mut transfer) = self.pending.get_mut(&call).map(Transfer::take_outputs) {
            if transfer.weak_outputs {
                self.commit_weak_value(transfer);
                return;
            }

            for slot in transfer.values.keys() {
                self.clear_value(slot);
            }

            let cells = std::mem::take(&mut transfer.cell_changes);

            self.commit_value(transfer);

            self.apply_cell_changes(cells, call);
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_cell_changes(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        effects: &[super::super::Effect],
    ) {
        let cells = self.checked_cell_changes(effects);

        if !cells.is_empty() {
            self.pending.entry(call).or_default().cell_changes.extend(cells);
        }
    }

    pub(in crate::analysis::storage_flow) fn checked_cell_changes(
        &self,
        effects: &[super::super::Effect],
    ) -> Vec<(Target, bray_symbols::DependencyMutationKind)> {
        let cells = effects.iter().filter_map(super::super::Effect::cell_change).collect::<Vec<_>>();

        cells.iter().copied().filter(|(target, kind)| {
            cells.iter().all(|(other, other_kind)| {
                (other == target && other_kind == kind)
                    || self.target_relationship(target, other) == bray_bound_tree::StorageRelationship::Disjoint
            })
        }).map(|(target, kind)| (target.clone(), kind)).collect()
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
        let current = self.allocate_current(&slot, allocation);

        self.values.insert(Target::storage(slot), AddressValue::one(current.clone()).into());

        current
    }

    pub(in crate::analysis::storage_flow) fn acquire_owned_backing(&mut self, owner: Place, producer: BoundExpressionId) {
        let backing = self.allocate_current(&owner, producer);

        self.values.entry(Target::storage(owner)).or_default().attach_owned_backing(AddressValue::one(backing));
    }

    pub(in crate::analysis::storage_flow) fn declare_entry_backing(&mut self, owner: Target, ty: TypeId) {
        if self.values.get(&owner).is_some_and(|value| value.owned_backing().is_some()) { return; }

        let backing = owner.project(&[bray_bound_tree::StorageProjection::OwnedTarget]);

        if backing.is_exact() {
            let contents = self.contents.entry(backing.clone()).or_default();

            contents.storage_type = Some(ty);
            contents.observe_initialized(ty);
        }

        self.values.entry(owner).or_default().attach_owned_backing(AddressValue::one(backing));
    }

    pub(in crate::analysis::storage_flow) fn owned_backing(&self, owner: &Target) -> Option<&Target> {
        let backing = self.values.get(owner)?.owned_backing()?;

        backing.is_exact().then(|| backing.targets.first()).flatten()
    }

    pub(in crate::analysis::storage_flow) fn stage_owned_backing(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        owner: Target,
        produced: Place,
        initialized: Option<TypeId>,
        allocated: bool,
    ) -> Option<Target> {
        let backing = Target {
            previous: false,
            origin: if allocated {
                Origin::Allocation { allocation: produced, projections: [].into() }
            } else {
                Origin::Returned { reference: produced, projections: [].into() }
            },
        };

        let pending = self.pending.entry(call).or_default();
        let value = pending.values.entry(owner).or_default();

        if value.is_no_payload() || value.owned_backing().is_some() { return None; }

        value.attach_owned_backing(AddressValue::one(backing.clone()));

        if let Origin::Returned { reference, .. } = &backing.origin {
            pending.returned_inputs.entry(reference.clone()).or_insert_with(AddressValue::unknown);
        }

        let contents = pending.payloads.entry(backing.clone()).or_default();

        contents.allocations.extend(call.expression());

        if let Some(ty) = initialized {
            contents.storage_type = Some(ty);
            contents.record_write(ty);
        }

        Some(backing)
    }

    pub(in crate::analysis::storage_flow) fn allocate_current(&mut self, slot: &Place, allocation: BoundExpressionId) -> Target {
        // A new execution cannot equate old aliases with this incarnation.
        let current = Target {
            origin: Origin::Allocation { allocation: slot.clone(), projections: [].into() },
            previous: false,
        };

        self.retire_region(&current, Retirement::Producer);

        let mut contents = Contents::default();

        contents.allocations.insert(allocation);

        self.contents.insert(current.clone(), contents);

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

        let mut contents = Contents::default();

        contents.storage_type = Some(element);

        self.contents.insert(target, contents);
    }

    fn owned_value(
        &self,
        owner: &Target,
        pointer: bray_bound_tree::StorageProjection,
    ) -> Option<&ValueBinding> {
        self.values
            .get(owner)
            .filter(|value| owner.is_exact() && !owner.previous && value.is_no_payload())
            .or_else(|| self.values.get(&owner.project(&[pointer])))
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
        let value = self.owned_value(owner, pointer)?;

        if value.is_no_payload() { Some(Vec::new()) } else { value.reference()?.regions() }
    }

    pub(in crate::analysis::storage_flow) fn owned_mutation_targets(
        &self,
        owner: &super::super::MutationTarget,
        pointer: bray_bound_tree::StorageProjection,
    ) -> Option<Vec<super::super::MutationTarget>> {
        let value = match &owner.address {
            Some(address) => self.owned_value(address, pointer),
            None => self.values.get(&owner.footprint().project(&[pointer]))
                .filter(|value| value.reference().is_some()),
        }?;
        let mut targets = value.mutation_targets()?;

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
            let value = self.owned_value(owner, pointer);

            if value.is_some_and(ValueBinding::is_no_payload) { continue; }

            if let Some(owned) = value.and_then(ValueBinding::reference) {
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
        self.point_at_addresses(AddressValue::one(target), destination, ty, kind);
    }

    pub(in crate::analysis::storage_flow) fn point_at_addresses(
        &mut self,
        references: AddressValue,
        destination: Place,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) {
        if !references.is_unknown() {
            for target in &references.targets {
                self.observe_addressed_target(target, ty, kind);
            }
        }

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
        if current.previous { return; }

        // A projected view can survive as a historical alias while other parts
        // of the returned region still belong to the current incarnation.
        let has_returned_region = if let Origin::Returned { reference, projections } = &current.origin {
            let key = (reference.clone(), false);

            let bounds = if projections.is_empty() && retirement == Retirement::Producer {
                self.returned_inputs.remove(&key)
            } else {
                self.returned_inputs.get(&key).cloned()
            };

            // First publication has no prior evaluation for retained captures to retire.
            let Some(bounds) = bounds else { return; };

            self.returned_inputs.entry((reference.clone(), true)).or_default().merge(&bounds);

            true
        } else {
            // Returned bounds are flattened to their original input regions at publication.
            for bounds in self.returned_inputs.values_mut() {
                bounds.retire(current, retirement);
            }

            false
        };

        self.retirements.insert((current.clone(), retirement));

        for destination in self.assignment_destinations.values_mut() {
            destination.retire(current, retirement);
        }

        if !has_returned_region && !self
            .contents
            .range(current.clone()..)
            .take_while(|(target, _)| current.contains(target))
            .any(|(target, _)| retirement.contains(current, target))
            && !self.values.range(current.clone()..).take_while(|(target, _)| current.contains(target))
                .any(|(target, value)| retirement.contains(current, target)
                    && (value.whole_sources().is_some() || value.report_dependencies().is_some()
                        || value.dependency_support().is_some()))
        {
            return;
        }

        for value in self.values.values_mut() {
            value.retire_references(current, retirement);
        }

        for references in self.borrows.values_mut() {
            references.retire(current, retirement);
        }

        for transfer in self.pending.values_mut() {
            transfer.retire_dependencies(current, retirement);
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
            contents.clear_initialization();

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
