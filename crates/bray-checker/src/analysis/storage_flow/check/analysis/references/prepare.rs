use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Place;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn reference_slots<'a>(
        &'a self,
        payload: &'a Place,
    ) -> impl Iterator<Item = Place> + 'a {
        std::iter::once(payload.clone()).chain(
            self.value_references
                .get(&payload.root)
                .into_iter()
                .flat_map(|paths| paths.keys())
                .filter(move |path| *path != payload && payload.contains(path))
                .cloned(),
        )
    }

    pub(in crate::analysis::storage_flow) fn has_complete_references(
        &self,
        storage: &bray_bound_tree::StoragePlan,
        value: &Place,
    ) -> bool {
        let ty = if value.projections.is_empty() {
            storage.storage_type(value.root)
        } else {
            value
                .access_id(storage)
                .and_then(|access| storage.access(access).map(|access| access.reached_type()))
        };

        ty.is_some_and(|ty| self.complete_reference_types.contains(&ty))
    }

    pub(in crate::analysis::storage_flow) fn inline_regions(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        value: &Place,
    ) -> Option<Vec<crate::analysis::storage_flow::memory::Target>> {
        if !value.projections.is_empty() || !self.has_complete_references(storage, value) {
            return None;
        }

        // The owning shape walk identifies every inline reference. Reuse its prepared paths.
        // Missing or unresolved fields retain the opaque fallback, never an empty origin bound.
        self.value_references
            .get(&value.root)
            .into_iter()
            .flat_map(|paths| paths.keys())
            .try_fold(Vec::new(), |mut targets, path| {
                targets.extend(memory.referenced_regions(
                    &crate::analysis::storage_flow::memory::Target::storage(path.clone()),
                )?);

                Some(targets)
            })
    }

    pub(in crate::analysis::storage_flow) fn prepare_buffer_fields<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        types: impl Iterator<Item = bray_symbols::TypeId>,
        demands: &mut super::entry::Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let types = types
            .map(|ty| request.semantic_values().unborrowed_type(ty))
            .collect::<std::collections::BTreeSet<_>>();

        for ty in types {
            let Some(fields) = crate::representation::raw_buffer_fields(request, ty)? else {
                continue;
            };

            let pointer_type = crate::storage::projected_value_type(
                request,
                ty,
                bray_symbols::DependencyProjection::ProductField(fields.pointer),
            )?;

            let pointer_type = pointer_type
                .value()
                .expect("recognized buffer pointer has its checked type");

            demands.entry(ty).or_default().insert((
                std::sync::Arc::from([bray_bound_tree::StorageProjection::ProductField(
                    fields.pointer,
                )]),
                pointer_type,
            ));

            let cleanup = cleanup_shapes.requirement(fields.element)?
                != bray_bound_tree::AsyncStorageCleanupRequirement::None;

            self.raw_buffers.insert(ty, (fields, cleanup));
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn prepare_lifecycle_references<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) -> crate::CheckerQueryResult<super::entry::Demands, C::UpstreamError> {
        let mut demands = super::entry::Demands::new();

        for ((ty, _), lifecycle) in &cleanup.lifecycles {
            if self
                .lifecycle_references
                .contains_key(&lifecycle.call.callable)
            {
                continue;
            }

            let mut types = std::collections::BTreeMap::new();

            for subject in
                dependencies.lifecycle_dependencies()[&lifecycle.call.callable].reference_subjects()
            {
                let key = (lifecycle.call.callable, subject.clone());

                if subject.subject_root() != bray_symbols::DependencySubjectRoot::Receiver
                    || self.lifecycle_paths.contains_key(&key)
                {
                    continue;
                }

                let Some(projections) =
                    crate::storage::resolve_storage_projections(request, subject.projections())?
                else {
                    continue;
                };

                // The actual cleanup receiver already denotes its physical storage.
                let owner = *ty;

                if let Some((ty, path)) =
                    crate::storage::reference_path(request, owner, projections.into(), None)?
                {
                    demands
                        .entry(owner)
                        .or_default()
                        .insert((std::sync::Arc::from(path.projections()), ty));

                    types.insert(subject.clone(), ty);
                    self.lifecycle_paths.insert(key, path);
                }
            }

            let mut references = Vec::new();

            for relation in
                dependencies.lifecycle_dependencies()[&lifecycle.call.callable].borrows()
            {
                if let Some(ty) = types.get(relation.entry().unwrap_or_else(|| relation.exit()))
                    && let Some(reference) =
                        crate::dependency::prepare_reference(request, relation, *ty)?
                {
                    references.push(reference);
                }
            }

            self.lifecycle_references
                .insert(lifecycle.call.callable, references);
        }

        Ok(demands)
    }

    pub(in crate::analysis::storage_flow) fn reference_target(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<crate::analysis::storage_flow::memory::Target> {
        let logical = storage.logical_path(access)?;
        let path = self.reference_paths.get(&access)?;

        let entry = storage
            .access(access)?
            .root()
            .borrow_capability()
            .filter(|borrow| {
                storage
                    .borrow_capability(*borrow)
                    .is_some_and(|borrow| borrow.entry_binding().is_some())
            });

        Self::reference_location(memory, logical.root, path, entry)
    }

    pub(in crate::analysis::storage_flow) fn reference_location(
        memory: &crate::analysis::storage_flow::memory::State,
        root: bray_bound_tree::StorageIdentityId,
        path: &bray_bound_tree::StorageReferencePath,
        entry: Option<bray_bound_tree::BorrowCapabilityId>,
    ) -> Option<crate::analysis::storage_flow::memory::Target> {
        let root_borrow = match (entry, path.borrows().first()) {
            (Some(capability), Some((0, ty))) => Some(memory.borrow_target(capability, *ty)?),
            _ => None,
        };

        memory.reference_location(
            crate::analysis::storage_flow::memory::Target::storage(Place::root(root)),
            path,
            root_borrow,
        )
    }

    pub(in crate::analysis::storage_flow) fn prepare_reference_paths<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (access, reached) in storage.access_entries() {
            let Some(path) = storage.logical_path(access) else {
                continue;
            };

            let Some(ty) = storage.identity_type(path.root) else {
                // A direct result slot has identity before its value type is installed.
                if path.projections.is_empty()
                    && matches!(
                        reached.root(),
                        bray_bound_tree::StorageAccessRoot::Storage(_)
                    )
                {
                    self.reference_paths.insert(
                        access,
                        bray_bound_tree::StorageReferencePath::new(
                            path.projections.clone(),
                            std::sync::Arc::from([]),
                        ),
                    );
                }

                continue;
            };

            if let Some((_, prepared)) = crate::storage::reference_path(
                request,
                ty,
                path.projections.clone(),
                Some(reached.reached_type()),
            )? {
                self.reference_paths.insert(access, prepared);
            }
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn prepare_reference_constructions<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        values: &crate::dependency::ValueInputs,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (identity, source) in storage.identity_entries() {
            let bray_bound_tree::StorageIdentity::Temporary(expression) = source else {
                continue;
            };

            let Some(owner) = storage.identity_type(identity) else {
                continue;
            };

            for (projection, operand, suffix) in values.constructed_fields(expression) {
                let Some(path) = project(request, Place::root(identity), &[projection])? else {
                    continue;
                };

                let ty = crate::storage::projected_value_type(request, owner, projection)?;

                let Some(mut ty) = *ty.value() else {
                    continue;
                };

                let Some(path) = project(request, path, suffix)? else {
                    continue;
                };

                for projection in suffix {
                    ty = crate::storage::projected_value_type(request, ty, *projection)?
                        .value()
                        .expect("selected input conversion retains its projected value type");
                }

                self.reference_constructions
                    .entry(expression)
                    .or_default()
                    .push((operand, path, ty));
            }
        }

        Ok(())
    }
}

pub(super) fn project<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    mut path: Place,
    projections: &[bray_symbols::DependencyProjection],
) -> crate::CheckerQueryResult<Option<Place>, C::UpstreamError> {
    let Some(suffix) = crate::storage::resolve_storage_projections(request, projections)? else {
        return Ok(None);
    };

    path.projections = path.projections.iter().copied().chain(suffix).collect();

    Ok(Some(path))
}
