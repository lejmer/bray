use crate::analysis::storage_flow::memory::{Origin, Place, ReachedAccess, State, Target};
use crate::CheckerRequestContext;
use bray_bound_tree::{StorageAccessId, StorageAddressValue, StoragePlan, StorageProjection, StorageReferencePath};

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

    pub(in crate::analysis::storage_flow) fn inline_reference_regions(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        value: &Place,
    ) -> Option<Vec<(crate::analysis::storage_flow::memory::Target, bray_symbols::DependencyReferenceKind)>> {
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
                let slot = crate::analysis::storage_flow::memory::Target::storage(path.clone());
                let kind = if memory.reached_borrow_capability(&slot).is_some() {
                    bray_symbols::DependencyReferenceKind::Borrow
                } else {
                    bray_symbols::DependencyReferenceKind::Address
                };

                targets.extend(memory.referenced_regions(&slot)?.into_iter().map(|target| (target, kind)));

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
        if let Some(target) = self.direct_reference_target(storage, access) {
            return Some(target);
        }

        let mut targets = self.reference_targets(values, memory, storage, access)?;

        (targets.len() == 1).then(|| targets.pop()).flatten()
    }

    pub(in crate::analysis::storage_flow) fn direct_reference_target(
        &self,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Target> {
        let (_, path) = storage.reference_route(access)?;

        if !path.steps().is_empty() || path.projections().iter().any(|projection|
            matches!(projection, StorageProjection::Element(_))) {
            return None;
        }

        Some(Target::storage(storage.logical_path(access)?.clone()))
    }

    pub(in crate::analysis::storage_flow) fn reference_targets(
        &self,
        values: &bray_symbols::SemanticValueStore,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Vec<crate::analysis::storage_flow::memory::Target>> {
        let (start, path, root_borrow) = self.reference_start(memory, storage, access)?;

        memory.reference_locations(values, start, path, root_borrow.map(|(target, _)| target))
    }

    pub(in crate::analysis::storage_flow) fn reached_access(
        &self,
        values: &bray_symbols::SemanticValueStore,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
        mut index: impl FnMut(bray_bound_tree::BoundExpressionId) -> Option<StorageProjection>,
    ) -> Option<ReachedAccess> {
        let (start, path, root_borrow) = self.reference_start(memory, storage, access)?;

        let mut bind = |projection| match projection {
            StorageProjection::Element(selector) => index(selector).unwrap_or(projection),
            _ => projection,
        };

        if path.steps().is_empty() {
            let reached = StorageProjection::bind_selectors(path.projections(), bind);

            return Some(ReachedAccess { address: StorageAddressValue::one(start.project(&reached)), loan: None });
        }

        memory.reached_access(values, start, path, root_borrow, &mut bind)
    }

    fn reference_start<'storage>(
        &self,
        memory: &State,
        storage: &'storage StoragePlan,
        access: StorageAccessId,
    ) -> Option<(Target, &'storage StorageReferencePath, Option<(Target, bray_bound_tree::BorrowCapabilityId)>)> {
        let logical = storage.logical_path(access)?;

        let (_, path) = storage.reference_route(access)?;

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
            (Some(capability), Some((0, 0, ty, bray_symbols::DependencyReferenceKind::Borrow))) =>
                Some((memory.borrow_target(capability, ty)?, capability)),
            _ => None,
        };

        Some((Target::storage(Place::root(logical.root)), path, root_borrow))
    }

    pub(in crate::analysis::storage_flow) fn index_observation_accesses<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
    ) {
        for (access, reached) in storage.access_entries() {
            let Some(path) = storage.logical_path(access) else {
                continue;
            };

            let stored_loan = matches!(reached.root(), bray_bound_tree::StorageAccessRoot::Storage(_))
                && crate::representation::reference_kind(request, reached.reached_type())
                    == Some(bray_symbols::DependencyReferenceKind::Borrow);

            if storage.reference_route(access).is_some_and(|(_, route)| route.steps().is_empty()) && !stored_loan {
                self.direct_observation_accesses.entry(path.root).or_default().push(access);
            } else {
                self.indirect_observation_accesses.push(access);
            }
        }
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

impl<C: CheckerRequestContext + ?Sized> super::super::core::StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow) fn evaluated_access(
        &self,
        memory: &State,
        execution: &crate::analysis::guarantee::ExecutionState,
        access: StorageAccessId,
    ) -> Option<ReachedAccess> {
        self.input.reached_access(self.request.semantic_values(), memory, self.storage, access, |selector| {
            let value = self.execution.operand_value(execution, selector);
            let element = self.evaluated_element(&value);

            element
        })
    }

    pub(in crate::analysis::storage_flow) fn evaluated_element(&self, value: &crate::ExecutionCondition) -> Option<StorageProjection> {
        use crate::ExecutionConditionKind as Kind;

        let value = match value.computed_value().kind() {
            Kind::ScalarValue(data) => data,
            _ => value.computed_value(),
        };

        match value.kind() {
            Kind::Literal(value) => {
                let bray_symbols::ConstantValueKind::Integer(index) = value.kind() else { return None; };

                let index = u32::try_from(index.to_u64()?).ok()?;

                Some(StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index)))
            }
            Kind::Input(place) if place.projections.is_empty() => {
                let input = place.reference()?;
                let ordinal = self.request.trusted_contracts()?.inputs.iter().position(|candidate| *candidate == input)?;
                let ordinal = u32::try_from(ordinal).expect("checked input ordinal fits the callable signature");

                Some(StorageProjection::ElementFromInput(bray_symbols::SymbolOrdinal::new(ordinal)))
            }
            _ => None,
        }
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
    pub(in crate::analysis::storage_flow::check::analysis) fn observe_stored_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        location: crate::analysis::storage_flow::memory::Target,
        logical: Place,
        pointee: bray_symbols::TypeId,
    ) {

        if !location.is_exact() || memory.referenced_regions(&location).is_some() {
            return;
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
                Origin::BackingRegion => unreachable!("an opaque backing region has no exact descriptor"),
            }

            container
        });

        let inputs = memory
            .effect_origins(&container)
            .0
            .cloned()
            .collect::<Vec<_>>();

        memory.observe_reference(location, logical, pointee, inputs);
    }
}
