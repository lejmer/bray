use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Source, State, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExecutionSite, StorageProjection, StorageReferencePath};
use bray_symbols::{DependencyStoredValue, DependencyStoredValuePath, DependencySubjectRoot, TypeId};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_stored_values(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(contracts) = self.dependencies.calls().get(&occurrence) else { return; };

        let transfers = self.stored_transfers(
            contracts.stored_values(),
            |endpoint, output| {
                let subject = endpoint.subject();

                let (ty, path) = contracts.stored_value_path(endpoint)?;

                let sources = if !output && endpoint.dereferences() == 0
                    && !path.projections().contains(&bray_bound_tree::StorageProjection::OwnedTarget) {

                    self.invocation_sources(state, occurrence, subject)?
                } else {
                    self.invocation_stored_locations(state, occurrence, subject, path, output)?
                        .into_iter().map(Source::Value).collect()
                };

                Some((ty, sources))
            },
            |endpoint| self.invocation_stored_root(occurrence, endpoint.subject().subject_root()),
        );

        for (destination, sources, complete) in transfers {
            state.memory.stage_whole_sources(occurrence, destination, sources, complete, false);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_stored_locations(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        subject: &bray_symbols::DependencySubject,
        path: &bray_bound_tree::StorageReferencePath,
        output: bool,
    ) -> Option<Vec<Target>> {
        let staged_root = (output && subject.subject_root() == DependencySubjectRoot::Result)
            .then(|| self.invocation_result(occurrence).map(Target::storage)).flatten();

        self.stored_locations(&state.memory, occurrence, path, staged_root.as_ref(), |prefix| {
            if let Some(root) = &staged_root {
                Some(vec![state.memory.staged_reference_location(
                    occurrence, self.request.semantic_values(), root, root, prefix,
                )?])
            } else {
                self.invocation_path_locations(state, occurrence, subject, prefix)
            }
        })
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_stored_root(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Target> {
        if root == DependencySubjectRoot::Result {
            self.invocation_result(occurrence).map(Target::storage)
        } else {
            self.invocation_place(occurrence, root).map(Target::storage)
                .or_else(|| self.static_location(&bray_symbols::DependencySubject::root(root)))
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_lifecycle_stored_values(
        &self,
        memory: &mut State,
        occurrence: BoundExecutionSite,
        callable: bray_symbols::CallableInstanceData,
        receiver: &Target,
    ) {
        let paths = &self.input.lifecycle_stored_paths[&callable];

        let transfers = self.stored_transfers(
            self.dependencies.lifecycle_dependencies()[&callable].stored_values(),
            |endpoint, _| {
                let (ty, path) = paths.get(endpoint)?;

                let locations = self.stored_locations(memory, occurrence, path, None, |prefix| {
                    Some(vec![memory.reference_location(self.request.semantic_values(), receiver.clone(), prefix, None)?])
                })?;

                Some((*ty, locations.into_iter().map(Source::Value).collect()))
            },
            |_| Some(receiver.clone()),
        );

        for (destination, sources, complete) in transfers {
            memory.stage_whole_sources(occurrence, destination, sources, complete, false);
        }
    }

    // Resolve owning projections through the descriptor's established backing
    // reference. No ownership or exact alias authority follows from this lookup.
    pub(in crate::analysis::storage_flow::check::analysis) fn stored_locations(
        &self,
        memory: &State,
        occurrence: BoundExecutionSite,
        path: &bray_bound_tree::StorageReferencePath,
        staged_root: Option<&Target>,
        first: impl FnOnce(&bray_bound_tree::StorageReferencePath) -> Option<Vec<Target>>,
    ) -> Option<Vec<Target>> {

        let projections = path.projections();
        let mut borrows = path.borrows().iter().copied().peekable();
        let mut start = 0;
        let mut first = Some(first);
        let mut locations = Vec::new();

        loop {
            let end = projections[start..].iter().position(|projection| *projection == StorageProjection::OwnedTarget)
                .map_or(projections.len(), |index| start + index);

            let mut segment_borrows = Vec::new();

            while borrows.peek().is_some_and(|(index, _)| *index <= end) {
                let (index, ty) = borrows.next().expect("peeked stored-path borrow exists");

                segment_borrows.push((index.checked_sub(start)?, ty));
            }

            let segment = StorageReferencePath::new(projections[start..end].into(), segment_borrows.into());

            locations = if let Some(resolve) = first.take() {
                resolve(&segment)?
            } else {
                locations.into_iter().map(|target| {
                    if let Some(root) = staged_root {
                        memory.staged_reference_location(occurrence, self.request.semantic_values(), root, &target, &segment)
                    } else {
                        memory.reference_location(self.request.semantic_values(), target, &segment, None)
                    }
                }).collect::<Option<Vec<_>>>()?
            };

            if end == projections.len() {
                return Some(locations);
            }

            let mut reached = std::collections::BTreeSet::new();

            for owner in locations {
                let owner_type = memory.declared_type(&owner).map(|ty| self.request.semantic_values().unborrowed_type(ty));
                let fields = self.input.raw_buffers.iter().filter(|(ty, _)| owner_type.is_none_or(|owner_type| owner_type == **ty));
                let mut found = false;

                for (_, (fields, _)) in fields {
                    let pointer = StorageProjection::ProductField(fields.pointer);

                    let regions = if let Some(root) = staged_root {
                        memory.staged_reference_regions(occurrence, root, &owner.project(&[pointer]))
                    } else {
                        memory.owned_regions(&owner, pointer)
                    };

                    if let Some(regions) = regions {
                        found = true;
                        reached.extend(regions);
                    }
                }

                if !found {
                    if owner_type.is_some_and(|ty| !self.input.raw_buffers.contains_key(&ty)) {
                        reached.insert(owner.project(&[StorageProjection::OwnedTarget]));
                    } else {
                        return None;
                    }
                }
            }

            locations = reached.into_iter().collect();

            if locations.is_empty() {
                return None;
            }

            start = end + 1;
        }
    }

    fn stored_transfers(
        &self,
        stored: &[DependencyStoredValue],
        resolve: impl Fn(&DependencyStoredValuePath, bool) -> Option<(TypeId, Vec<Source>)>,
        containing: impl Fn(&DependencyStoredValuePath) -> Option<Target>,
    ) -> Vec<(Target, Vec<Source>, bool)> {
        let mut transfers = Vec::new();

        for value in stored {
            let Some((ty, destinations)) = resolve(value.destination(), true) else {
                if let Some(root) = containing(value.destination()) {
                    transfers.push((root, Vec::new(), false));
                }

                continue;
            };

            if self.input.footprint.complete_reference_types.get(&ty) == Some(&true) {
                continue;
            }

            let mut sources = Vec::new();
            let mut complete = value.is_complete() && destinations.len() == 1;

            for source in value.sources() {
                match resolve(source, false) {
                    Some((ty, resolved)) => {
                        if self.input.footprint.complete_reference_types.get(&ty) != Some(&true) {
                            complete &= !resolved.is_empty();
                            sources.extend(resolved);
                        }
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
}
