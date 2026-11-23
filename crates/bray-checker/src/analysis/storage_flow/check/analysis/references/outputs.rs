use bray_bound_tree::{
    BoundCallDependencyContracts, BoundCallReference, BoundCallStorage, BoundExecutionSite,
    StorageObservedTarget, StorageReferencePath,
};
use bray_symbols::{
    CapturedValueOutcome, DependencyStoredValue, DependencyStoredValuePath, DependencySubject,
    DependencySubjectRoot,
};
use std::collections::BTreeMap;

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::storage::storage_projections;

pub(in crate::analysis::storage_flow::check::analysis) enum InvocationOutputs<'a> {
    Call {
        contracts: &'a BoundCallDependencyContracts,
        entry: Option<&'a crate::ExecutionCallEvidence>,
        roots: &'a [Target],
    },
    Lifecycle {
        storage: &'a BoundCallStorage,
        receiver: Option<&'a Target>,
        path: Option<&'a Place>,
    },
    Frame {
        storage: &'a BoundCallStorage,
        entry: &'a crate::ExecutionCallEvidence,
        has_receiver: bool,
        result: &'a Place,
    },
}

impl InvocationOutputs<'_> {
    pub(super) fn storage(&self) -> &BoundCallStorage {
        match self {
            Self::Call { contracts, .. } => contracts.storage(),
            Self::Lifecycle { storage, .. } | Self::Frame { storage, .. } => storage,
        }
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn stage_invocation_outputs<'outcome>(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: InvocationOutputs<'_>,
        references: &mut [BoundCallReference],
        outcomes: impl Iterator<Item = &'outcome CapturedValueOutcome> + Clone,
        mut returned_referents: Option<&mut BTreeMap<Place, StorageObservedTarget>>,
    ) {
        state.memory.begin_transition(occurrence);

        if matches!(invocation, InvocationOutputs::Lifecycle { receiver: None, .. }) {
            return;
        }

        let inactive = self.inactive_output_paths(occurrence, &invocation, outcomes.clone());

        for payload in &inactive {
            state.memory.stage_no_payload_alternative(occurrence, Target::storage(payload.clone()));
        }

        self.stage_owned_result(state, occurrence, &invocation, &inactive, outcomes.clone());

        // Allocation anchors must be available before binding their payload routes.
        references.sort_by_key(|reference| {
            let relation = reference.relation();

            let depth = invocation.storage().subject(relation.exit())
                .map_or(0, |(_, path)| path.steps().len());

            (depth, relation.entry().is_some())
        });

        for reference in references {
            self.stage_invocation_reference(state, occurrence, &invocation, reference, returned_referents.as_deref_mut());
        }

        match &invocation {
            InvocationOutputs::Call { roots, .. } => {
                if let Some(result) = self.invocation_result(occurrence) {
                    self.stage_result_reference_bounds(state, occurrence, &result, &inactive, |_, _| roots.iter().cloned());
                }
            }
            InvocationOutputs::Frame { result, entry, .. } => {
                self.stage_result_reference_bounds(state, occurrence, result, &inactive, |memory, _| {
                    Self::invocation_roots(memory, Some(entry))
                });
            }
            InvocationOutputs::Lifecycle { .. } => {}
        }

        self.stage_stored_outputs(state, occurrence, &invocation, outcomes);
    }

    /// An inline conversion publishes its evaluated operand independently of a callee contract.
    pub(in crate::analysis::storage_flow::check::analysis) fn stage_inline_input(
        &self, state: &mut StorageFlowState, occurrence: BoundExecutionSite,
        entry: Option<&crate::ExecutionCallEvidence>,
    ) -> bool {
        let BoundExecutionSite::CallInput { expression, ordinal } = occurrence else { return false; };

        let Some(selection) = self.execution.semantics.selections().expression(expression) else { return false; };

        let input = selection.input(ordinal).expect("input materialization retains its selection");

        let Some(projections) = input.inline_materialization_path() else { return false; };

        state.memory.begin_transition(occurrence);

        let source = entry.and_then(|entry| entry.argument_storage(bray_symbols::SymbolOrdinal::new(0)));

        if let Some(source) = source {
            let destination = self.invocation_result(occurrence).expect("inline materialization retains its input slot");

            Source::Captured(source.clone()).stage(&mut state.memory, occurrence, Target::storage(destination).project(projections));
        }

        true
    }

    fn inactive_output_paths<'outcome>(
        &self,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        outcomes: impl Iterator<Item = &'outcome CapturedValueOutcome>,
    ) -> Vec<Place> {
        let inactive = CapturedValueOutcome::common_inactive_payloads(outcomes, DependencySubject::contains);

        inactive.iter().filter(|subject| subject.subject_root() == DependencySubjectRoot::Result)
            .filter_map(|subject| match invocation {
                InvocationOutputs::Call { .. } => self.invocation_logical_path(occurrence, subject),
                InvocationOutputs::Frame { result, .. } => {
                    let projections = storage_projections(self.request.semantic_values(), self.dependencies, subject.projections())?;

                    Some(Place { root: result.root, projections: result.projections.iter().chain(&projections).copied().collect() })
                }
                InvocationOutputs::Lifecycle { .. } => None,
            }).collect()
    }

    fn stage_owned_result<'outcome>(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        inactive: &[Place],
        outcomes: impl Iterator<Item = &'outcome CapturedValueOutcome>,
    ) {
        let destination = match invocation {
            InvocationOutputs::Call { .. } => self.invocation_result(occurrence),
            InvocationOutputs::Frame { result, .. } => Some((*result).clone()),
            InvocationOutputs::Lifecycle { .. } => None,
        };

        let Some(destination) = destination else { return; };

        let transfers = bray_symbols::DependencyOwnedResult::common_transfers(outcomes.map(|row| &row.ownership), &[]);

        self.stage_returned_values(state, occurrence, invocation, &destination, transfers);
        self.stage_result_backings(state, occurrence, invocation, &destination, inactive);
    }

    fn stage_result_backings(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        destination: &Place,
        inactive: &[Place],
    ) {
        let values = self.request.semantic_values();
        let root = Target::storage(Place::root(destination.root));

        for (owner, (route, ty)) in self.input.owned_backings.range(destination.clone()..)
            .take_while(|(owner, _)| destination.contains(owner))
        {
            if inactive.iter().any(|absent| absent.contains(owner))
                || route.reads().next().is_some()
            {
                continue;
            }

            let Some(locations) = state.memory.staged_reference_locations(occurrence, values, &root, &root, route) else { continue; };

            let [location] = locations.as_slice() else { continue; };

            if !location.is_exact() { continue; }

            let heap = owner == destination && matches!(invocation, InvocationOutputs::Call { .. }) && self.is_heap_construction(occurrence);

            let source = match invocation {
                InvocationOutputs::Call { entry, .. } if heap => {
                    let root = DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0));

                    entry.and_then(|entry| self.execution.invocation_capture(entry, occurrence, root))
                        .cloned().map(Source::Captured)
                }
                _ => None,
            };

            if let Some(backing) = state.memory.stage_owned_backing(occurrence, location.clone(), owner.clone(), Some(*ty), heap)
                && let Some(source) = source
            {
                source.stage(&mut state.memory, occurrence, backing);
            }
        }
    }

    fn is_heap_construction(&self, occurrence: BoundExecutionSite) -> bool {
        use bray_bound_tree::{ConstructionTarget, SelectedOperation, SemanticSelection};
        use bray_symbols::TraitCallableFulfillmentSymbolId;

        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) = occurrence.expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression)) else { return false; };

        let ConstructionTarget::TypeForm { callable, .. } = construction.target() else { return false; };

        let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new("HeapStorageCreate")
            .expect("the compiler-provided heap constructor has a known declaration");

        self.request.available_compiler_known_symbols().declaration_symbol::<TraitCallableFulfillmentSymbolId>(&key)
            .is_some_and(|symbol| callable.definition().symbol() == symbol.into())
    }

    fn stage_invocation_reference(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        reference: &BoundCallReference,
        returned_referents: Option<&mut BTreeMap<Place, StorageObservedTarget>>,
    ) {
        let relation = reference.relation();
        let output = relation.exit();
        let root = output.subject_root();
        let storage = invocation.storage();

        let Some((ty, path)) = storage.subject(output) else { return; };

        let writes = match invocation {
            InvocationOutputs::Call { .. } | InvocationOutputs::Frame { .. } => {
                root == DependencySubjectRoot::Result || !path.steps().is_empty()
            }
            InvocationOutputs::Lifecycle { .. } => root == DependencySubjectRoot::Receiver && !output.projections().is_empty(),
        };

        let observed = returned_referents.and_then(|referents| {
            if !relation.is_exact() || relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                || relation.entry() != Some(output) || relation.dereferences() != 0
                || !relation.target_projections().is_empty() {
                return None;
            }

            self.invocation_place(occurrence, root)?.access_id(self.storage)?;

            let path = self.invocation_logical_path(occurrence, output)?;

            let pointee = crate::representation::borrow_target_type(self.request.semantic_values(), ty)
                .expect("a checked borrow relation binds a borrow type");

            Some((referents, path, pointee))
        });

        let sources = if writes || observed.is_some() {
            relation.entry().and_then(|entry| self.output_sources(state, occurrence, invocation, entry))
        } else {
            None
        };

        let pointer_regions = (writes && sources.is_none())
            .then(|| relation.entry().and_then(|entry| {
                self.output_pointer_regions(state, occurrence, invocation, entry)
            })).flatten();

        if let Some((referents, path, pointee)) = observed
            && let Some([source]) = sources.as_deref()
            && let Some(targets) = source.targets(&state.memory)
            && let [target] = targets.as_slice()
        {
            referents.insert(path, StorageObservedTarget::borrowed(target.clone(), pointee));
        }

        if !writes {
            return;
        }

        let Some(destinations) = self.output_locations(state, occurrence, invocation, output, path, true) else {
            if let Some(root) = self.output_root(occurrence, invocation, root) {
                state.memory.stage_whole_sources(occurrence, root, [], false, false);
            }

            return;
        };

        let logical = self.output_logical_path(occurrence, invocation, output, path);

        let allocation = relation.allocation().and_then(|allocation| {
            let (_, path) = storage.subject(allocation)?;

            let allocation = self.output_logical_path(occurrence, invocation, allocation, path)?;

            let projections = storage_projections(
                self.request.semantic_values(), self.dependencies, relation.target_projections(),
            ).expect("prepared allocation projections remain available");

            Some((allocation, projections))
        });

        for destination in destinations {
            let logical = logical.clone().or_else(|| destination.inline_state_path());

            if relation.source_subject().is_none() {
                self.stage_independent_reference(&mut state.memory, occurrence, destination, logical, reference);
                continue;
            }

            let Some(logical) = logical else {
                state.memory.stage_unknown(occurrence, destination);
                continue;
            };

            if let Some((allocation, projections)) = &allocation {
                state.memory.stage_allocation_reference(occurrence, destination, allocation.clone(), projections);
            } else if relation.allocation().is_some() {
                state.memory.stage_unknown(occurrence, destination);
            } else {
                self.stage_reference_sources(
                    &mut state.memory, occurrence, destination, &logical, ty, reference,
                    sources.iter().flatten().cloned(), pointer_regions.as_deref(),
                );
            }
        }
    }

    fn output_logical_path(
        &self,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        subject: &DependencySubject,
        route: &StorageReferencePath,
    ) -> Option<Place> {
        match invocation {
            InvocationOutputs::Call { .. } => self.invocation_logical_path(occurrence, subject),
            InvocationOutputs::Lifecycle { path, .. } if subject.subject_root() == DependencySubjectRoot::Receiver => {
                let path = (*path)?;

                Some(Place {
                    root: path.root,
                    projections: path.projections.iter().chain(route.projections()).copied().collect(),
                })
            }
            InvocationOutputs::Lifecycle { .. } => None,
            InvocationOutputs::Frame { result, .. } if subject.subject_root() == DependencySubjectRoot::Result => {
                Some(Place {
                    root: result.root,
                    projections: result.projections.iter().chain(route.projections()).copied().collect(),
                })
            }
            InvocationOutputs::Frame { .. } => None,
        }
    }

    fn output_locations(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        subject: &DependencySubject,
        path: &StorageReferencePath,
        output: bool,
    ) -> Option<Vec<Target>> {
        match invocation {
            InvocationOutputs::Call { entry, .. } => {
                let root = subject.subject_root();

                if root.is_static() || root == DependencySubjectRoot::Result {
                    return self.invocation_stored_locations(state, occurrence, subject, path, output);
                }

                let captured = self.execution.invocation_capture((*entry)?, occurrence, root)?;
                let path = self.invocation_reference_path(occurrence, root, path)?;

                state.memory.captured_dependency_targets(self.request.semantic_values(), captured, &path)
                    .map(|(targets, _)| targets)
            }
            InvocationOutputs::Lifecycle { receiver, .. } => {
                self.cleanup_subject_locations(&state.memory, *receiver, subject, Some(path))
            }
            InvocationOutputs::Frame { entry, has_receiver, result, .. } => {
                let root = subject.subject_root();
                let values = self.request.semantic_values();

                if root == DependencySubjectRoot::Result && output {
                    let result = Target::storage((*result).clone());

                    return state.memory.staged_reference_locations(occurrence, values, &result, &result, path);
                }

                if root.is_static() {
                    let start = self.static_location(&DependencySubject::root(root))?;

                    return state.memory.reference_locations(values, start, path, None);
                }

                let captured = entry.argument_storage(root.input_ordinal(*has_receiver)?)?;
                let sources = state.memory.captured_reference_sources(values, captured, path)?;

                sources.into_iter().map(|source| match source {
                    Source::Value(location) => Some(location),
                    Source::Borrow(_) | Source::Captured(_) => None,
                }).collect()
            }
        }
    }

    fn output_sources(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        subject: &DependencySubject,
    ) -> Option<Vec<Source>> {
        match invocation {
            InvocationOutputs::Call { entry, .. } => {
                self.admitted_reference_sources(state, occurrence, *entry, subject)
            }
            InvocationOutputs::Lifecycle { .. } => {
                let (_, path) = invocation.storage().subject(subject)?;

                self.output_locations(state, occurrence, invocation, subject, path, false)
                    .map(|locations| locations.into_iter().map(Source::Value).collect())
            }
            InvocationOutputs::Frame { entry, has_receiver, .. } => {
                let root = subject.subject_root();

                let (_, path) = invocation.storage().subject(subject)?;

                if root.is_static() {
                    return self.output_locations(state, occurrence, invocation, subject, path, false)
                        .map(|locations| locations.into_iter().map(Source::Value).collect());
                }

                let captured = entry.argument_storage(root.input_ordinal(*has_receiver)?)?;

                state.memory.captured_reference_sources(self.request.semantic_values(), captured, path)
            }
        }
    }

    fn output_pointer_regions(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        subject: &DependencySubject,
    ) -> Option<Vec<Target>> {
        let root = subject.subject_root();

        let (_, path) = invocation.storage().subject(subject)?;

        let (captured, path) = match invocation {
            InvocationOutputs::Call { entry, .. } => (
                self.execution.invocation_capture((*entry)?, occurrence, root)?,
                self.invocation_reference_path(occurrence, root, path)?,
            ),
            InvocationOutputs::Frame { entry, has_receiver, .. } => (
                entry.argument_storage(root.input_ordinal(*has_receiver)?)?,
                std::borrow::Cow::Borrowed(path),
            ),
            InvocationOutputs::Lifecycle { .. } => return None,
        };

        // The route takes a value while the admission retains its immutable capture.
        state.memory.pointer_regions_before_last_read(
            self.request.semantic_values(), Source::Captured(captured.clone()), &path,
        )
    }

    fn output_root(
        &self,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        root: DependencySubjectRoot,
    ) -> Option<Target> {
        match invocation {
            InvocationOutputs::Call { .. } => self.invocation_stored_root(occurrence, root),
            InvocationOutputs::Lifecycle { receiver, .. } if root == DependencySubjectRoot::Receiver => receiver.cloned(),
            InvocationOutputs::Lifecycle { .. } => self.static_location(&DependencySubject::root(root)),
            InvocationOutputs::Frame { result, .. } if root == DependencySubjectRoot::Result => Some(Target::storage((*result).clone())),
            InvocationOutputs::Frame { .. } => self.static_location(&DependencySubject::root(root)),
        }
    }

    fn stage_stored_outputs<'outcome>(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        invocation: &InvocationOutputs<'_>,
        outcomes: impl Iterator<Item = &'outcome CapturedValueOutcome>,
    ) {
        let stored = CapturedValueOutcome::stored_dependencies(outcomes, DependencySubject::contains);

        let transfers = self.stored_transfers(
            &stored,
            |endpoint, output| {
                let subject = endpoint.subject();

                let (_, path) = invocation.storage().stored_value_path(endpoint)?;

                let inline_source = !output && endpoint.dereferences() == 0
                    && !path.projections().contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                    && !matches!(invocation, InvocationOutputs::Lifecycle { .. });

                let sources = if inline_source {
                    self.output_sources(state, occurrence, invocation, subject)?
                } else if !output && let InvocationOutputs::Frame { entry, has_receiver, .. } = invocation
                    && let Some(input) = subject.subject_root().input_ordinal(*has_receiver)
                {
                    let captured = entry.argument_storage(input)?;

                    state.memory.captured_reference_sources(self.request.semantic_values(), captured, path)?
                } else {
                    self.output_locations(state, occurrence, invocation, subject, path, output)?
                        .into_iter().map(Source::Value).collect()
                };

                Some(sources)
            },
            |endpoint| self.output_root(occurrence, invocation, endpoint.subject().subject_root()),
        );

        for (destination, sources, complete) in transfers {
            state.memory.stage_whole_sources(occurrence, destination, sources, complete, false);
        }
    }

    fn stored_transfers(
        &self,
        stored: &[DependencyStoredValue],
        resolve: impl Fn(&DependencyStoredValuePath, bool) -> Option<Vec<Source>>,
        containing: impl Fn(&DependencyStoredValuePath) -> Option<Target>,
    ) -> Vec<(Target, Vec<Source>, bool)> {
        let mut transfers = Vec::new();

        for value in stored {
            let Some(destinations) = resolve(value.destination(), true) else {
                if let Some(root) = containing(value.destination()) {
                    transfers.push((root, Vec::new(), false));
                }

                continue;
            };

            let mut sources = Vec::new();
            let mut complete = value.is_complete() && destinations.len() == 1;

            for source in value.sources() {
                match resolve(source, false) {
                    Some(resolved) => {
                        complete &= !resolved.is_empty();
                        sources.extend(resolved);
                    }
                    None => complete = false,
                }
            }

            for destination in destinations {
                let Source::Value(destination) = destination else {
                    panic!("stored output endpoint must resolve to a value location");
                };

                // Alternative outputs share captured source identities, never mutable value state.
                transfers.push((destination, sources.clone(), complete));
            }
        }

        transfers
    }
    fn stage_independent_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        destination: Target,
        logical: Option<Place>,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        if reference.relation().is_no_payload() {
            memory.stage_no_payload_alternative(occurrence, destination);
        } else if reference.relation().is_empty_reference() {
            memory.stage_empty_reference(occurrence, destination);
        } else if reference.relation().is_backing_region() {
            memory.stage_unresolved_backing_reference(occurrence, destination);
        } else if let Some(logical) = logical {
            memory.stage_returned_reference(occurrence, destination, logical,
                reference.addressed_type().expect("independent reference retains its target type"), []);
        } else {
            memory.stage_unknown(occurrence, destination);
        }
    }

    fn stage_reference_sources(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        destination: Target,
        logical: &Place,
        ty: bray_symbols::TypeId,
        reference: &bray_bound_tree::BoundCallReference,
        sources: impl IntoIterator<Item = crate::analysis::storage_flow::memory::Source>,
        pointer_regions: Option<&[Target]>,
    ) {
        let mut sources = sources.into_iter().peekable();

        if sources.peek().is_none() {
            self.stage_unresolved_reference(memory, occurrence, destination, logical, ty, reference, pointer_regions);

            return;
        }

        for source in sources {
            // Alternatives share the same destination projection.
            self.stage_reference(memory, occurrence, source, destination.clone(), logical, ty, reference);
        }
    }

    fn stage_unresolved_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        destination: Target,
        logical: &Place,
        ty: bray_symbols::TypeId,
        reference: &bray_bound_tree::BoundCallReference,
        pointer_regions: Option<&[Target]>,
    ) {
        if reference.target_path().is_some_and(|path| matches!(path.steps().last(), Some(bray_bound_tree::StorageReferenceStep::Backing { .. }))) {
            memory.stage_unresolved_backing_reference(occurrence, destination);

            return;
        }

        if let (Some(regions), Some(target)) = (pointer_regions,
            crate::representation::reference_target_type(self.request, ty)) {
            memory.stage_returned_reference(occurrence, destination, logical.clone(), target, regions.iter().cloned());
        } else {
            memory.stage_unknown(occurrence, destination);
        }
    }

    fn stage_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        source: crate::analysis::storage_flow::memory::Source,
        destination: Target,
        logical: &Place,
        ty: bray_symbols::TypeId,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        let relation = reference.relation();
        let values = self.request.semantic_values();

        let reached = if let Some(path) = reference.reads() {
            // The captured source shares its immutable snapshot while route reads use a value.
            match memory.follow_source_reference_reads(values, source.clone(), path) {
                Some(reached) => reached,
                None => {
                    let regions = (reference.target_path().is_none() && reference.addressed_type().is_none())
                        .then(|| memory.pointer_regions_before_last_read(values, source, path)).flatten();

                    self.stage_unresolved_reference(memory, occurrence, destination, logical, ty, reference, regions.as_deref());

                    return;
                }
            }
        } else {
            vec![source]
        };

        if let Some(path) = reference.target_path() {
            let targets = reached.iter().try_fold(Vec::new(), |mut targets, source| {
                targets.extend(memory.follow_effect_path(values, source.mutation_targets(memory)?, path)?);

                Some(targets)
            });

            let Some(targets) = targets else {
                self.stage_unresolved_reference(memory, occurrence, destination, logical, ty, reference, None);
                return;
            };

            let bounded = relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions;
            let mut regions = Vec::new();

            for target in targets {
                if !bounded && let Some(address) = target.address {
                    memory.stage_address(occurrence, address, destination.clone(), reference);
                } else {
                    regions.push(target.footprint);
                }
            }

            if !regions.is_empty() {
                let ty = reference.addressed_type().expect("projected reference retains its reached type");

                memory.stage_reference_regions(occurrence, destination, logical.clone(), ty, regions);
            }
        } else if relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions {
            let Some(regions) = reached.iter().map(|source| source.regions(memory)).collect::<Option<Vec<_>>>()
            else {
                memory.stage_unknown(occurrence, destination);
                return;
            };

            let ty = reference.addressed_type().expect("reference regions retain their output type");

            memory.stage_reference_regions(occurrence, destination, logical.clone(), ty, regions.into_iter().flatten());
        } else {
            for source in reached {
                self.stage_reference_value(memory, occurrence, source, destination.clone(), ty, reference);
            }
        }
    }

    fn stage_reference_value(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        source: crate::analysis::storage_flow::memory::Source,
        destination: Target,
        ty: bray_symbols::TypeId,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        let relation = reference.relation();

        if reference.exposes_borrow() {
            memory.stage_exposed_borrow(occurrence, source, destination.clone());

            if !relation.is_exact() {
                memory.bound_staged_reference(occurrence, &destination);
            }
        } else if reference.addressed_type().is_some() {
            let Source::Value(target) = source else {
                memory.stage_unknown(occurrence, destination);
                return;
            };

            memory.stage_address(occurrence, target, destination.clone(), reference);
        } else {
            if relation.kind() == bray_symbols::DependencyReferenceKind::Borrow
                && relation.provenance() == bray_symbols::DependencyReferenceProvenance::Alternatives
                && let Some(pointee) = crate::representation::borrow_target_type(self.request.semantic_values(), ty)
                && memory.stage_forwarded_borrow(occurrence, &destination, &source, pointee)
            {
                return;
            }

            source.stage(memory, occurrence, destination.clone());

            if !relation.is_exact() {
                memory.bound_staged_reference(occurrence, &destination);
            }
        }
    }

}
