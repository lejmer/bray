use crate::analysis::storage_flow::memory::{Origin, Source, Target};
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Place;
use crate::analysis::guarantee::CleanupMutation;
use bray_symbols::{DependencyStorageEffect, DependencySubjectRoot};

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
                .filter(move |path| *path != payload && payload.contains_storage(path))
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

    pub(in crate::analysis::storage_flow) fn reference_target(
        &self,
        values: &bray_symbols::SemanticValueStore,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<crate::analysis::storage_flow::memory::Target> {
        let targets = self.reference_targets(values, memory, storage, access)?;

        let [target] = targets.as_slice() else {
            return None;
        };

        Some(target.clone())
    }

    pub(in crate::analysis::storage_flow) fn reference_targets(
        &self,
        values: &bray_symbols::SemanticValueStore,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Vec<crate::analysis::storage_flow::memory::Target>> {
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

        let root_borrow = match (entry, path.reads().next()) {
            (Some(capability), Some((0, 0, ty, bray_symbols::DependencyReferenceKind::Borrow))) => Some(memory.borrow_target(capability, ty)?),
            _ => None,
        };

        memory.reference_locations(values, Target::storage(Place::root(logical.root)), path, root_borrow)
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
        selections: &bray_bound_tree::CheckedSemanticSelections,
        types: &bray_bound_tree::CheckedExpressionTypes,
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
                        .push(((*operand).into(), types.expression(*operand).expect("array operand has its checked type").ty(), path, *element));
                }
            }

            if let Some(selection @ bray_bound_tree::SemanticSelection::Operation(
                bray_bound_tree::SelectedOperation::Construction(construction),
            )) = selections.expression(expression)
            {
                for input in construction.inputs() {
                    let (field, ordinal) = match input {
                        bray_bound_tree::SelectedConstructionInput::Explicit { input, ordinal, .. }
                        | bray_bound_tree::SelectedConstructionInput::Default { input, ordinal, .. } => (input, *ordinal),
                    };

                    let projection = match field {
                        bray_bound_tree::ConstructionInputId::StructField(field) => bray_symbols::DependencyProjection::ProductField(*field),
                        bray_bound_tree::ConstructionInputId::UnionPayloadField(field) => bray_symbols::DependencyProjection::UnionPayloadField(*field),
                        bray_bound_tree::ConstructionInputId::CallableParameter(_) => continue,
                    };

                    let Some(path) = project(request, Place::root(identity), &[projection])? else { continue; };

                    let (source, source_type) = if selection.stages_inputs() {
                        (bray_bound_tree::BoundExecutionSite::CallInput { expression, ordinal: bray_symbols::SymbolOrdinal::new(ordinal) }, input.ty())
                    } else {
                        let bray_bound_tree::SelectedConstructionInput::Explicit { expression: operand, .. } = input else {
                            unreachable!("unstaged construction has explicit inputs");
                        };

                        ((*operand).into(), types.expression(*operand).expect("construction input has its checked type").ty())
                    };

                    self.reference_constructions.entry(expression).or_default().push((source, source_type, path, input.ty()));
                }

                // Selected inputs own conversion and default placement. Dependency
                // projection suffixes describe their original, unconverted sources.
                continue;
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
                    .push((operand.into(), types.expression(operand).expect("aggregate operand has its checked type").ty(), path, ty));
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
        &mut self,
        state: &mut crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {

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

            let Some(logical) = self.invocation_logical_path(occurrence, subject)
            else {
                continue;
            };

            let Some(sources) = self.invocation_sources(state, occurrence, subject) else {
                continue;
            };

            let [Source::Value(location)] = sources.as_slice() else {
                continue;
            };

            if let Err(error) = self.observe_stored_reference(&mut state.memory, location.clone(), logical, pointee) {
                self.query_failure = Some(error);
                return;
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn observe_stored_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        location: crate::analysis::storage_flow::memory::Target,
        logical: Place,
        pointee: bray_symbols::TypeId,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {

        if !location.is_exact() || memory.referenced_regions(&location).is_some() {
            return Ok(());
        }

        let container = location.allocation_region().unwrap_or_else(|| {
            let mut container = location.clone();

            match &mut container.origin {
                Origin::Entry { projections, .. }
                | Origin::Returned { projections, .. }
                | Origin::Static { projections, .. } | Origin::Allocation { projections, .. } => {
                    *projections = std::sync::Arc::from([]);
                }
                Origin::Storage(place) => {
                    place.projections = std::sync::Arc::from([]);
                }
            }

            container
        });

        let inputs = memory
            .effect_origins(&container)
            .0
            .cloned()
            .collect::<Vec<_>>();

        memory.observe_reference(self.request, location, logical, pointee, inputs)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn observe_cleanup_reference_inputs(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        receiver: &Target,
        mutation: &CleanupMutation<'_>,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let logical = match mutation {
            CleanupMutation::Opaque { path, .. } => *path,
            CleanupMutation::Lifecycle(call) => call.path,
            CleanupMutation::Unchanged | CleanupMutation::Recovery => unreachable!("only an actual cleanup has a receiver"),
        }.expect("resolved cleanup receiver retains its logical path");

        let effects = match mutation {
            CleanupMutation::Opaque { ty, .. } => self.input.buffer_cleanup_effects.get(ty).map(Vec::as_slice),
            CleanupMutation::Lifecycle(call) => Some(self.dependencies.lifecycle_dependencies()[&call.lifecycle.call.callable].effects()),
            CleanupMutation::Unchanged | CleanupMutation::Recovery => None,
        };

        let subjects = effects.into_iter().flatten().filter_map(|effect| match effect {
            DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
            _ => None,
        }).collect::<std::collections::BTreeSet<_>>();

        let mut inputs = subjects.into_iter().filter_map(|subject| {
            let prepared = match mutation {
                CleanupMutation::Opaque { ty, .. } => self.input.buffer_cleanup_paths.get(&(*ty, subject.clone())),
                CleanupMutation::Lifecycle(call) => self.input.lifecycle_paths.get(&(call.lifecycle.call.callable, subject.clone())),
                CleanupMutation::Unchanged | CleanupMutation::Recovery => None,
            }?;

            Some((subject, prepared))
        }).collect::<Vec<_>>();

        // A parent descriptor supplies the current backing for nested owned paths.
        inputs.sort_by_key(|(_, (_, path))| path.projections().len());

        for (subject, (ty, path)) in inputs {
            let Some(pointee) = crate::representation::reference_target_type(self.request, *ty) else {
                continue;
            };

            let Some(locations) = self.cleanup_subject_locations(memory, receiver, subject, Some(path)) else {
                continue;
            };

            for location in locations {
                let logical = if subject.subject_root() == DependencySubjectRoot::Receiver {
                    Place { root: logical.root, projections: logical.projections.iter().chain(path.projections()).copied().collect() }
                } else if let Some(logical) = location.observation_path() {
                    logical
                } else {
                    continue;
                };

                self.observe_stored_reference(memory, location, logical, pointee)?;
            }
        }

        Ok(())
    }
}
