use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Source, State, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::BoundExecutionSite;
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

        if let Some(root) = &staged_root {
            state.memory.staged_reference_locations(
                occurrence, self.request.semantic_values(), root, root, path,
            )
        } else {
            self.invocation_path_locations(state, occurrence, subject, path)
        }
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

                let locations = self.cleanup_subject_locations(memory, receiver, endpoint.subject(), Some(path))?;

                Some((*ty, locations.into_iter().map(Source::Value).collect()))
            },
            |endpoint| {
                let root = endpoint.subject().subject_root();

                if root == DependencySubjectRoot::Receiver {
                    Some(receiver.clone())
                } else {
                    self.static_location(&bray_symbols::DependencySubject::root(root))
                }
            },
        );

        for (destination, sources, complete) in transfers {
            memory.stage_whole_sources(occurrence, destination, sources, complete, false);
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
