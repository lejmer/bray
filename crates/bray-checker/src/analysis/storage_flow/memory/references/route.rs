use std::borrow::Cow;
use std::collections::BTreeSet;

use bray_bound_tree::{
    BorrowCapabilityId, StorageCapturedValue, StorageMutationTarget, StorageProjection, StorageReferencePath,
    StorageReferenceStep,
};
use bray_symbols::TypeId;

use super::super::model::{AddressValue, ReachedAccess, State, Target, ValueBinding};

enum ReferenceRouteValue {
    Capture(StorageCapturedValue),
    Locations(Vec<Target>, Option<BorrowCapabilityId>),
    Regions(Vec<Target>),
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum ReferencePrecision {
    Region,
    TypedRegion,
    Location,
}

impl ReferenceRouteValue {
    fn project(self, projections: &[StorageProjection]) -> Self {
        match self {
            Self::Capture(value) => Self::Capture(projections.iter().copied()
                .fold(value, |value, projection| value.project(projection))),
            Self::Locations(targets, loan) => Self::Locations(targets.into_iter()
                .map(|target| target.project(projections)).collect(), loan),
            regions @ Self::Regions(_) => regions,
        }
    }

    fn project_reached(
        self,
        projections: &[StorageProjection],
        bind: &mut Option<&mut dyn FnMut(StorageProjection) -> StorageProjection>,
    ) -> Self {
        if let Some(bind) = bind
            && let Self::Locations(targets, _) = &self
            && projections.iter().any(|projection| matches!(projection, StorageProjection::Element(_)))
            && targets.iter().all(|target| target.is_exact() && target.value_observation_path().is_some())
        {
            // An inline reached value can name an exact indexed cell. A slice
            // backing continues to use its prepared region and descriptor paths.
            let projections = StorageProjection::bind_selectors(projections, bind);

            return self.project(&projections);
        }

        self.project(projections)
    }

    fn project_descriptor<'a>(
        mut self,
        projections: &[StorageProjection],
        bind: &mut Option<&mut dyn FnMut(StorageProjection) -> StorageProjection>,
        lookup: &impl Fn(&Target) -> Option<&'a ValueBinding>,
    ) -> Self {
        let mut start = 0;

        for (index, projection) in projections.iter().enumerate() {
            if matches!(projection, StorageProjection::NullableValue
                | StorageProjection::ActiveUnionPayloadField { .. } | StorageProjection::UninitPayload)
            {
                let end = index + 1;

                self = self.project_reached(&projections[start..end], bind).without_inactive(lookup);
                start = end;
            }
        }

        if start < projections.len() {
            self.project_reached(&projections[start..], bind).without_inactive(lookup)
        } else { self }
    }

    fn without_inactive<'a>(self, lookup: &impl Fn(&Target) -> Option<&'a ValueBinding>) -> Self {
        match self {
            Self::Capture(value) if value.binding().is_some_and(ValueBinding::is_no_payload) =>
                Self::Locations(Vec::new(), None),
            Self::Locations(mut targets, loan) => {
                targets.retain(|target| target.previous || !target.is_exact()
                    || !lookup(target).is_some_and(ValueBinding::is_no_payload));

                Self::Locations(targets, loan)
            }
            route => route,
        }
    }
}

impl State {
    pub(in crate::analysis::storage_flow) fn follow_source_reference_reads(
        &self,
        values: &bray_symbols::SemanticValueStore,
        source: super::Source,
        path: &StorageReferencePath,
    ) -> Option<Vec<super::Source>> {
        let locations = match source {
            super::Source::Captured(value) => return self.captured_reference_sources(values, &value, path),
            super::Source::Value(location) => self.reference_locations(values, location, path, None)?,
            super::Source::Borrow(capability) => {
                let mut reached = Vec::new();

                for target in self.borrow_targets(capability)? {
                    reached.extend(self.reference_locations(values, target.clone(), path, Some(target))?);
                }

                reached
            }
        };

        Some(locations.into_iter().map(super::Source::Value).collect())
    }

    pub(in crate::analysis::storage_flow) fn captured_reference_sources(
        &self,
        values: &bray_symbols::SemanticValueStore,
        value: &StorageCapturedValue,
        path: &StorageReferencePath,
    ) -> Option<Vec<super::Source>> {
        Some(match self.captured_route(values, value, path)? {
            ReferenceRouteValue::Capture(value) => vec![super::Source::Captured(value)],
            ReferenceRouteValue::Locations(locations, _) => locations.into_iter().map(super::Source::Value).collect(),
            ReferenceRouteValue::Regions(..) => return None,
        })
    }

    pub(in crate::analysis::storage_flow) fn reference_locations(
        &self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        root_borrow: Option<Target>,
    ) -> Option<Vec<Target>> {
        self.reference_locations_in(values, target, path, root_borrow, |target| self.values.get(target))
    }

    pub(in crate::analysis::storage_flow) fn reached_access(
        &self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &StorageReferencePath,
        root_borrow: Option<(Target, BorrowCapabilityId)>,
        bind: &mut dyn FnMut(StorageProjection) -> StorageProjection,
    ) -> Option<ReachedAccess> {
        let start = ReferenceRouteValue::Locations(vec![target], None);
        let root_borrow = root_borrow.map(|(target, loan)| (target, Some(loan)));

        match self.follow_reference_route(values, start, path, root_borrow, |target| self.values.get(target), Some(bind))? {
            ReferenceRouteValue::Locations(locations, loan) => Some(ReachedAccess {
                address: AddressValue::from_targets(locations), loan,
            }),
            ReferenceRouteValue::Regions(regions) => {
                let mut address = AddressValue::from_targets(regions);

                address.bound();

                Some(ReachedAccess { address, loan: None })
            }
            ReferenceRouteValue::Capture(_) => unreachable!("an actual storage route remains in memory"),
        }
    }

    pub(in crate::analysis::storage_flow) fn staged_reference_locations(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
        root: &Target,
        start: &Target,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Vec<Target>> {
        let staged = self.pending.get(&invocation)?;

        self.reference_locations_in(values, start.clone(), path, None, |target| {
            staged.values.get(target).or_else(|| (!root.contains_storage(target)).then(|| self.values.get(target)).flatten())
        })
    }

    pub(in crate::analysis::storage_flow) fn captured_effect_targets(
        &self,
        values: &bray_symbols::SemanticValueStore,
        value: &StorageCapturedValue,
        path: &StorageReferencePath,
        indirect: bool,
    ) -> Option<Vec<StorageMutationTarget>> {
        self.mutation_route(self.captured_route(values, value, path)?, indirect)
    }

    /// The physical state resolves write destinations from the admitted capture.
    pub(in crate::analysis::storage_flow) fn reached_write_resolver<'a>(
        &'a self,
        values: &'a bray_symbols::SemanticValueStore,
    ) -> impl Fn(&StorageCapturedValue, &StorageReferencePath) -> Option<Vec<StorageMutationTarget>> + 'a {
        move |value, path| self.captured_effect_targets(values, value, path, false)
    }

    pub(in crate::analysis::storage_flow) fn reference_effect_targets(
        &self,
        values: &bray_symbols::SemanticValueStore,
        start: Target,
        path: Option<&StorageReferencePath>,
        indirect: bool,
    ) -> Option<Vec<StorageMutationTarget>> {
        let start = ReferenceRouteValue::Locations(vec![start], None);

        let reached = if let Some(path) = path {
            self.follow_reference_route(values, start, path, None, |target| self.values.get(target), None)?
        } else {
            start
        };

        self.mutation_route(reached, indirect)
    }

    pub(in crate::analysis::storage_flow) fn follow_effect_path(
        &self,
        values: &bray_symbols::SemanticValueStore,
        targets: Vec<StorageMutationTarget>,
        path: &StorageReferencePath,
    ) -> Option<Vec<StorageMutationTarget>> {
        if path.steps().is_empty() {
            return Some(targets.into_iter().map(|target| target.project(path.projections())).collect());
        }

        let owning = path.steps().iter().all(|step| matches!(step, StorageReferenceStep::Backing { .. }));
        let mut reached = Vec::new();

        for target in targets {
            if let Some(actual) = target.address.and_then(|address| self.reference_effect_targets(values, address, Some(path), false)) {
                reached.extend(actual);
            } else if owning {
                // Owning storage bounds its backing even when activation cannot
                // read the pointer. This footprint supplies no mutation operand.
                reached.push(StorageMutationTarget::bounded(target.footprint.project(path.projections())));
            } else {
                return None;
            }
        }

        Some(reached)
    }

    fn mutation_route(&self, reached: ReferenceRouteValue, indirect: bool) -> Option<Vec<StorageMutationTarget>> {
        match reached {
            ReferenceRouteValue::Capture(value) if indirect => value.mutation_targets(),
            ReferenceRouteValue::Capture(_) => None,
            ReferenceRouteValue::Locations(locations, _) => StorageMutationTarget::resolve_locations(
                locations, indirect, |location| self.values.get(&location)?.mutation_targets()),
            ReferenceRouteValue::Regions(regions) if !indirect => {
                Some(regions.into_iter().map(StorageMutationTarget::bounded).collect())
            }
            ReferenceRouteValue::Regions(..) => None,
        }
    }

    pub(in crate::analysis::storage_flow) fn captured_dependency_targets(
        &self,
        values: &bray_symbols::SemanticValueStore,
        value: &StorageCapturedValue,
        path: &StorageReferencePath,
    ) -> Option<(Vec<Target>, Option<BorrowCapabilityId>)> {
        match self.captured_route(values, value, path)? {
            ReferenceRouteValue::Locations(targets, loan) => Some((targets, loan)),
            ReferenceRouteValue::Capture(_) | ReferenceRouteValue::Regions(..) => None,
        }
    }

    fn captured_route(
        &self,
        values: &bray_symbols::SemanticValueStore,
        value: &StorageCapturedValue,
        path: &StorageReferencePath,
    ) -> Option<ReferenceRouteValue> {
        self.follow_reference_route(values, ReferenceRouteValue::Capture(value.clone()), path, None, |target| self.values.get(target), None)
    }

    fn reference_locations_in<'a>(
        &'a self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &StorageReferencePath,
        root_borrow: Option<Target>,
        lookup: impl Fn(&Target) -> Option<&'a ValueBinding>,
    ) -> Option<Vec<Target>> {
        let start = ReferenceRouteValue::Locations(vec![target], None);
        let root_borrow = root_borrow.map(|target| (target, None));

        match self.follow_reference_route(values, start, path, root_borrow, lookup, None)? {
            ReferenceRouteValue::Locations(locations, _) => Some(locations),
            ReferenceRouteValue::Regions(..) => None,
            ReferenceRouteValue::Capture(_) => unreachable!("an actual storage route remains in memory"),
        }
    }

    fn follow_reference_route<'a>(
        &'a self,
        values: &bray_symbols::SemanticValueStore,
        mut reached: ReferenceRouteValue,
        path: &StorageReferencePath,
        mut root_borrow: Option<(Target, Option<BorrowCapabilityId>)>,
        lookup: impl Fn(&Target) -> Option<&'a ValueBinding>,
        mut bind: Option<&mut dyn FnMut(StorageProjection) -> StorageProjection>,
    ) -> Option<ReferenceRouteValue> {
        let mut selected = 0;

        for (step, operation) in path.steps().iter().copied().enumerate() {
            let index = operation.index();

            if selected != index {
                reached = reached.project_descriptor(&path.projections()[selected..index], &mut bind, &lookup);
            } else if matches!(operation, StorageReferenceStep::Backing { pointer: Some(_), .. }) {
                reached = reached.without_inactive(&lookup);
            }

            let (pointer, consumes) = match operation {
                StorageReferenceStep::ReadBorrow { .. } => (None, false),
                StorageReferenceStep::ReadAddress { .. } => (None, true),
                StorageReferenceStep::Backing { pointer: Some(pointer), .. } =>
                    (Some(StorageProjection::ProductField(pointer)), true),
                StorageReferenceStep::Backing { pointer: None, .. } => {
                    reached = match reached {
                        ReferenceRouteValue::Capture(value) => {
                            let binding = value.binding()?;

                            if binding.is_no_payload() {
                                return Some(ReferenceRouteValue::Locations(Vec::new(), None));
                            }

                            let backing = binding.owned_backing()?;

                            if let Some(actual) = backing.actual_targets() {
                                ReferenceRouteValue::Locations(actual, None)
                            } else { ReferenceRouteValue::Regions(backing.regions()?) }
                        }
                        ReferenceRouteValue::Locations(owners, _) => {
                            let mut targets = std::collections::BTreeSet::new();
                            let mut actual = true;

                            for owner in owners {
                                let binding = lookup(&owner)?;

                                if binding.is_no_payload() {
                                    continue;
                                }

                                let backing = binding.owned_backing()?;

                                if let Some(reached) = backing.actual_targets() {
                                    targets.extend(reached);
                                } else {
                                    actual = false;
                                    targets.extend(backing.regions()?);
                                }
                            }

                            let targets = targets.into_iter().collect();

                            if actual { ReferenceRouteValue::Locations(targets, None) }
                            else { ReferenceRouteValue::Regions(targets) }
                        }
                        ReferenceRouteValue::Regions(..) => return None,
                    };

                    selected = index + 1;
                    continue;
                }
            };

            if let Some((root, loan)) = root_borrow.take().filter(|_| step == 0 && index == 0 && !consumes) {
                reached = ReferenceRouteValue::Locations(vec![root], loan);
            } else {
                if let Some(pointer) = pointer {
                    reached = reached.project(&[pointer]);
                }

                let borrow_read = matches!(operation, StorageReferenceStep::ReadBorrow { .. });

                let (targets, loan, precision) = match reached {
                    ReferenceRouteValue::Capture(value) => {
                        let binding = value.binding()?;

                        if binding.is_no_payload() {
                            return Some(ReferenceRouteValue::Locations(Vec::new(), None));
                        }

                        let (targets, precision) = self.reference_step_targets(values, operation, binding)?;

                        (targets.iter().cloned().collect::<Vec<_>>(), if borrow_read { binding.borrow_capability() } else { None }, precision)
                    }
                    ReferenceRouteValue::Locations(locations, _) => {
                        let mut targets = BTreeSet::new();
                        let mut common_loan = None;
                        let mut precision = ReferencePrecision::Location;

                        for location in locations {
                            let binding = lookup(&location)?;

                            if binding.is_no_payload() {
                                continue;
                            }

                            let loan = binding.borrow_capability();

                            let (reached, reached_precision) = self.reference_step_targets(values, operation, binding)?;

                            targets.extend(reached.iter().cloned());
                            precision = precision.min(reached_precision);

                            if borrow_read {
                                common_loan = Some(match common_loan {
                                    None => loan,
                                    Some(previous) if previous == loan => previous,
                                    Some(_) => None,
                                });
                            }
                        }

                        (targets.into_iter().collect(), common_loan.flatten(), precision)
                    }
                    ReferenceRouteValue::Regions(..) => return None,
                };

                // A captured descriptor is read once. Further steps read activation memory.
                if precision != ReferencePrecision::Location {
                    if step + 1 != path.steps().len() {
                        return None;
                    }

                    // An established layout supports inline projection of the bound.
                    // Reading another stored descriptor still requires a reached location.
                    let suffix = &path.projections()[index + usize::from(consumes)..];

                    let bounds = if precision == ReferencePrecision::TypedRegion && !suffix.is_empty() {
                        targets.into_iter().map(|target| target.project(suffix)).collect()
                    } else {
                        targets
                    };

                    return Some(ReferenceRouteValue::Regions(bounds));
                }

                reached = ReferenceRouteValue::Locations(targets, loan);
            }

            selected = index + usize::from(consumes);
        }

        Some(if selected == path.projections().len() { reached } else {
            reached.project_reached(&path.projections()[selected..], &mut bind)
        })
    }

    fn reference_step_targets<'a>(
        &self,
        values: &bray_symbols::SemanticValueStore,
        operation: StorageReferenceStep,
        binding: &'a ValueBinding,
    ) -> Option<(Cow<'a, BTreeSet<Target>>, ReferencePrecision)> {
        let Some(reference) = &binding.address else {
            return Some((Cow::Owned(binding.reference_regions()?.into_iter().collect()), ReferencePrecision::Region));
        };

        match operation {
            StorageReferenceStep::ReadBorrow { target, .. } | StorageReferenceStep::ReadAddress { target, .. } => {
                if let Some(targets) = self.route_pointer_targets(values, reference, target) {
                    let precision = if reference.has_values() {
                        ReferencePrecision::Location
                    } else {
                        ReferencePrecision::TypedRegion
                    };

                    Some((Cow::Borrowed(targets), precision))
                } else {
                    Some((Cow::Owned(reference.regions()?.into_iter().collect()), ReferencePrecision::Region))
                }
            }
            StorageReferenceStep::Backing { pointer: Some(_), .. } => {
                if reference.is_displaced() {
                    Some((Cow::Owned(reference.regions()?.into_iter().collect()), ReferencePrecision::Region))
                } else {
                    let precision = if reference.has_values() {
                        ReferencePrecision::Location
                    } else {
                        ReferencePrecision::Region
                    };

                    (!reference.is_unknown()).then_some((Cow::Borrowed(&reference.targets), precision))
                }
            }
            StorageReferenceStep::Backing { pointer: None, .. } =>
                unreachable!("structural backing has no descriptor read"),
        }
    }

    fn route_pointer_targets<'a>(
        &self,
        values: &bray_symbols::SemanticValueStore,
        reference: &'a AddressValue,
        ty: TypeId,
    ) -> Option<&'a std::collections::BTreeSet<Target>> {
        self.typed_targets(reference, ty).or_else(|| {
            let data = values.type_data(ty);

            let bray_symbols::TypeData::Slice(element) = data.as_ref() else { return None; };

            // A slice view keeps established element regions. Bounds cannot
            // supply a layout or turn a dynamic selector into one cell.
            (!reference.is_unknown() && !reference.is_displaced() && !reference.nullable && !reference.targets.is_empty()
                && reference.targets.iter().all(|reached| {
                    let declared = self.contents.get(reached).and_then(|contents| contents.storage_type);

                    !reached.previous && (!reference.is_bounded() || !reached.is_exact())
                        && (declared == Some(ty) || declared == Some(*element))
                })).then_some(&reference.targets)
        })
    }
}
