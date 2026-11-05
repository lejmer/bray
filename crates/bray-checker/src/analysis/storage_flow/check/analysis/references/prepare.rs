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

    pub(in crate::analysis::storage_flow) fn reference_free(
        &self,
        storage: &bray_bound_tree::StoragePlan,
        value: &Place,
    ) -> Option<bool> {
        let ty = if value.projections.is_empty() {
            storage.storage_type(value.root)
        } else {
            value
                .access_id(storage)
                .and_then(|access| storage.access(access).map(|access| access.reached_type()))
        };

        self.footprint.complete_reference_types.get(&ty?).copied()
    }

    pub(in crate::analysis::storage_flow) fn inline_regions(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        value: &Place,
    ) -> Option<Vec<crate::analysis::storage_flow::memory::Target>> {
        if self.reference_free(storage, value)? {
            // Loading a scalar or a reference-free field does not publish its
            // owner's address. Its evaluation loan is not a stored pointer.
            return Some(Vec::new());
        }

        if !value.projections.is_empty() {
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
            let mut incomplete = std::collections::BTreeSet::new();

            for relation in
                dependencies.lifecycle_dependencies()[&lifecycle.call.callable].borrows()
            {
                let subject = if relation.provenance()
                    == bray_symbols::DependencyReferenceProvenance::InputRegions
                {
                    relation.exit()
                } else {
                    relation.entry().unwrap_or_else(|| relation.exit())
                };

                if relation
                    .entry()
                    .is_none_or(|entry| types.contains_key(entry))
                    && let Some(ty) = types.get(subject)
                    && let Some(reference) =
                        crate::dependency::prepare_reference(request, relation, *ty)?
                {
                    references.push(reference);
                } else {
                    incomplete.insert(relation.exit().clone());
                }
            }

            references.retain(|reference| !incomplete.contains(reference.relation().exit()));

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

        let (_, path) = self.reference_paths.get(&access)?;

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
                        (
                            reached.reached_type(),
                            bray_bound_tree::StorageReferencePath::new(
                                path.projections.clone(),
                                std::sync::Arc::from([]),
                            ),
                        ),
                    );
                }

                continue;
            };

            if let Some(prepared) = crate::storage::reference_path(
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

            if let Some(bray_bound_tree::BoundExpression::Structured(array)) =
                request.view().expression(expression)
                && array.kind() == bray_bound_tree::BoundStructuredExpressionKind::Array
                && let bray_symbols::TypeData::Array { element, .. } =
                    request.semantic_values().type_data(owner).as_ref()
            {
                for (ordinal, operand) in array.operands().iter().enumerate() {
                    let ordinal = bray_symbols::SymbolOrdinal::new(
                        u32::try_from(ordinal)
                            .expect("checked array element count fits storage ordinals"),
                    );

                    let mut path = Place::root(identity);

                    path.projections = [bray_bound_tree::StorageProjection::ElementFromStart(
                        ordinal,
                    )]
                    .into();

                    self.reference_constructions
                        .entry(expression)
                        .or_default()
                        .push((*operand, path, *element));
                }
            }

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

impl<C: CheckerRequestContext + ?Sized> super::super::core::StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn observe_call_reference_inputs(
        &self,
        state: &mut crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {
        use crate::analysis::storage_flow::memory::Source;

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return;
        };

        for subject in contracts.reference_subjects() {
            if !matches!(
                subject.subject_root(),
                bray_symbols::DependencySubjectRoot::Receiver
                    | bray_symbols::DependencySubjectRoot::Parameter(_)
            ) {
                continue;
            }

            let Some(ty) = contracts.reference_type(subject) else {
                continue;
            };

            let Some(pointee) = crate::representation::reference_target_type(self.request, ty)
            else {
                continue;
            };

            let Some(mut logical) = self.invocation_place(occurrence, subject.subject_root())
            else {
                continue;
            };

            let path = contracts
                .reference_path(subject)
                .expect("checked reference input retains its prepared path");

            logical.projections = logical
                .projections
                .iter()
                .chain(path.projections())
                .copied()
                .collect();

            let Some(sources) = self.invocation_sources(state, occurrence, subject) else {
                continue;
            };

            let [Source::Value(location)] = sources.as_slice() else {
                continue;
            };

            self.observe_stored_reference(&mut state.memory, location.clone(), logical, pointee);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn observe_stored_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        location: crate::analysis::storage_flow::memory::Target,
        logical: Place,
        pointee: bray_symbols::TypeId,
    ) {
        use crate::analysis::storage_flow::memory::Origin;

        if !location.is_exact() || memory.referenced_regions(&location).is_some() {
            return;
        }

        let container = location.allocation_region().unwrap_or_else(|| {
            let mut container = location.clone();

            match &mut container.origin {
                Origin::Entry { projections, .. }
                | Origin::Returned { projections, .. }
                | Origin::Static { projections, .. } => {
                    *projections = std::sync::Arc::from([]);
                }
                Origin::Storage(place) | Origin::Allocation(place) => {
                    place.projections = std::sync::Arc::from([]);
                }
            }

            container
        });

        let inputs = memory
            .effect_origins(&container)
            .cloned()
            .collect::<Vec<_>>();

        memory.observe_reference(location, logical, pointee, inputs);
    }
}
