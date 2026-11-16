use std::collections::BTreeSet;
use super::demands::Demands;
use std::sync::Arc;

use crate::CheckerRequestContext;

use crate::analysis::storage_flow::memory::Place;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_entry_references<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        mut demands: Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let paths = self.reference_paths.iter().filter_map(|(access, (_, prepared))| {
            let owner = storage.root_identity(*access).and_then(|root| storage.identity_type(root))?;
            let reached = storage.access(*access).expect("prepared reference has a checked access").reached_type();

            Some((owner, Arc::from(prepared.projections()), reached))
        }).collect::<Vec<_>>();

        for (owner, path, reached) in paths {
            self.prepare_path_demands(request, owner, path, Some(reached), &mut demands)?;
        }

        self.prepare_cleanup_target_demands(request, cleanup, dependencies, &mut demands)?;
        Self::close_entry_demands(request, &mut demands)?;

        self.install_reference_paths(request, storage, &demands)?;

        self.prepare_entry_pointee_fields(request, storage, &demands)
    }

    fn prepare_entry_pointee_fields<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        demands: &Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        // Prepare demanded fields of the original incoming addresses, not an unbounded
        // pointer-type closure. Writes retire these records through the ordinary value store.
        let incoming = self.entry_references.iter().filter_map(|((slot, _), (path, pointee, kind))| {
            (*kind == bray_symbols::DependencyReferenceKind::Address
                && self.entry_buffer_pointer(path, *pointee).is_none())
                .then(|| pointee.map(|pointee| (slot.clone(), pointee))).flatten()
        }).collect::<Vec<_>>();

        for (slot, pointee) in incoming {
            let owner = request.semantic_values().unborrowed_type(pointee);

            for (projections, reached) in demands.get(&owner).into_iter().flatten() {
                let projections = slot.projections.iter().copied()
                    .chain([bray_bound_tree::StorageProjection::AddressTarget])
                    .chain(projections.iter().copied()).collect::<Arc<[_]>>();

                let root = storage.storage_type(slot.root).expect("incoming reference has its checked storage type");

                let Some((reached, prepared)) = crate::storage::reference_path(request, root,
                    Arc::clone(&projections), Some(*reached))? else { continue; };

                self.prepare_entry_reference_path(request, storage, Place { root: slot.root, projections }, reached, &prepared);
            }
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn initialize_entry_references(
        &self,
        values: &bray_symbols::SemanticValueStore,
        memory: &mut crate::analysis::storage_flow::memory::State,
        initialized: &BTreeSet<bray_bound_tree::StorageIdentityId>,
    ) {
        let mut references = self.entry_references.iter().collect::<Vec<_>>();

        references.sort_by_key(|((_, layer), (path, _, _))| (
            path.borrows().len() + path.projections().iter().filter(|projection| **projection == bray_bound_tree::StorageProjection::OwnedTarget).count(),
            path.projections().len(), *layer,
        ));

        for ((slot, layer), (path, ty, kind)) in references {
            if let Some(fields) = self.entry_buffer_pointer(path, *ty) {
                let owner_path = path.prefix(path.projections().len() - 1, path.borrows().len());

                let owners = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(slot.root)), &owner_path, None);

                for owner in owners.into_iter().flatten() {
                    let pointer = bray_bound_tree::StorageProjection::ProductField(fields.pointer);

                    memory.declare_owned_buffer(owner.clone(), owner.project(&[pointer]), fields.element);

                    if self.footprint.complete_reference_types.get(&fields.element) != Some(&true) {
                        for region in memory.owned_regions(&owner, pointer).into_iter().flatten() {
                            let element = region.project(&[bray_bound_tree::StorageProjection::AnyElement]);

                            memory.seed_whole_source(element.clone(), element);
                        }
                    }
                }

                continue;
            }

            let locations = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(slot.root)), path, None);

            for location in locations.into_iter().flatten() {
                memory.entry_reference_at(slot.clone(), *layer, location, *ty, *kind);
            }
        }

        for (root, path) in &self.entry_whole_sources {
            if !initialized.contains(root) {
                continue;
            }

            let locations = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(*root)), path, None);

            for location in locations.into_iter().flatten() {
                memory.seed_whole_source(location.clone(), location);
            }
        }
    }

    fn install_reference_paths<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        demands: &Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (identity, source) in storage.identity_entries() {
            let entry = source.is_initialized_at_entry();

            let Some(ty) = storage.storage_type(identity) else {
                continue;
            };

            let owner = request.semantic_values().unborrowed_type(ty);

            for (projections, reached) in demands.get(&owner).into_iter().flatten() {
                let Some((reached, prepared)) = crate::storage::reference_path(
                    request,
                    ty,
                    Arc::clone(projections),
                    Some(*reached),
                )?
                else {
                    continue;
                };

                let reference = prepared
                    .borrows()
                    .first()
                    .map(|(index, target)| {
                        (
                            Place {
                                root: identity,
                                projections: projections[..*index].into(),
                            },
                            *target,
                        )
                    })
                    .or_else(|| {
                        crate::representation::reference_target_type(request, reached).map(
                            |target| {
                                (
                                    Place {
                                        root: identity,
                                        projections: Arc::clone(projections),
                                    },
                                    target,
                                )
                            },
                        )
                    });

                // Owning elements have separate storage. Whole-value reference
                // publication covers the descriptor's physical fields.
                if let Some((path, target)) = reference
                    && bray_bound_tree::StorageProjection::remains_in_allocation(&path.projections)
                {
                    self.value_references
                        .entry(identity)
                        .or_default()
                        .insert(path, target);
                }

                if !entry {
                    continue;
                }

                self.prepare_entry_reference_path(
                    request,
                    storage,
                    Place {
                        root: identity,
                        projections: Arc::clone(projections),
                    },
                    reached,
                    &prepared,
                );
            }
        }

        Ok(())
    }

    fn prepare_entry_reference_path<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        path: Place,
        ty: bray_symbols::TypeId,
        prepared: &bray_bound_tree::StorageReferencePath,
    ) {
        if !storage.identity(path.root).is_some_and(bray_bound_tree::StorageIdentity::is_initialized_at_entry) {
            return;
        }

        for (step, (index, target)) in prepared
            .borrows()
            .iter()
            .enumerate()
            .filter(|(step, (index, _))| *step != 0 || *index != 0)
        {
            let prefix = Place {
                root: path.root,
                projections: path.projections[..*index].into(),
            };

            self.entry_references
                .entry((
                    prefix.clone(),
                    1 + prepared.borrows()[..step]
                        .iter()
                        .rev()
                        .take_while(|(previous, _)| previous == index)
                        .count(),
                ))
                .or_insert_with(|| {
                    (
                        prepared.prefix(*index, step),
                        Some(*target),
                        if prepared.is_address_load(step) {
                            bray_symbols::DependencyReferenceKind::Address
                        } else { bray_symbols::DependencyReferenceKind::Borrow },
                    )
                });
        }

        let layer = 1 + prepared
            .borrows()
            .iter()
            .rev()
            .take_while(|(index, _)| *index == path.projections.len())
            .count();

        if let Some(kind) = crate::representation::reference_kind(request, ty)
            && (kind == bray_symbols::DependencyReferenceKind::Address
                || !path.projections.is_empty() || layer > 1) {
            let target = crate::representation::reference_target_type(request, ty);

            self.entry_references
                .insert((path, layer), (prepared.clone(), target, kind));
        }
    }
}

