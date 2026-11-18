use bray_bound_tree::{
    StorageCapturedValue, StorageMutationTarget, StorageProjection, StorageReferencePath,
    StorageReferenceStep,
};
use bray_symbols::TypeId;

use super::super::model::{AddressValue, State, Target};

enum ReferenceRouteValue {
    Capture(StorageCapturedValue),
    Locations(Vec<Target>),
}

impl ReferenceRouteValue {
    fn project(self, projections: &[StorageProjection]) -> Self {
        match self {
            Self::Capture(value) => Self::Capture(projections.iter().copied()
                .fold(value, |value, projection| value.project(projection))),
            Self::Locations(targets) => Self::Locations(targets.into_iter()
                .map(|target| target.project(projections)).collect()),
        }
    }
}

impl State {
    pub(in crate::analysis::storage_flow) fn reference_locations(
        &self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        root_borrow: Option<Target>,
    ) -> Option<Vec<Target>> {
        self.reference_locations_in(values, target, path, root_borrow,
            |target| self.values.get(target).and_then(|value| value.address.as_ref()))
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
                .and_then(|value| value.address.as_ref())
        })
    }

    pub(in crate::analysis::storage_flow) fn captured_input_target(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
        root: &Target,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Target> {
        let captured = &self.pending.get(&invocation)?.inputs;

        let lookup = |target: &Target| {
            (if root.contains_storage(target) { captured.get(target) } else { self.values.get(target) }).and_then(|value| value.address.as_ref())
        };

        let locations = self.reference_locations_in(values, root.clone(), path, None, lookup)?;

        let [location] = locations.as_slice() else {
            return None;
        };

        let references = lookup(location)?;

        references.is_exact().then(|| references.targets.first()
            .expect("exact captured input has one referent").clone())
    }

    pub(in crate::analysis::storage_flow) fn captured_effect_targets(
        &self,
        values: &bray_symbols::SemanticValueStore,
        value: &StorageCapturedValue,
        path: &StorageReferencePath,
        indirect: bool,
    ) -> Option<Vec<StorageMutationTarget>> {
        let lookup = |target: &Target| self.values.get(target).and_then(|value| value.reference());
        let root = ReferenceRouteValue::Capture(value.clone());

        match self.follow_reference_route(values, root, path, None, lookup)? {
            ReferenceRouteValue::Capture(value) if indirect => value.mutation_targets(),
            ReferenceRouteValue::Capture(_) => None,
            ReferenceRouteValue::Locations(locations) => StorageMutationTarget::resolve_locations(
                locations, indirect, |location| self.values.get(&location)?.mutation_targets()),
        }
    }

    fn reference_locations_in<'a>(
        &'a self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &StorageReferencePath,
        root_borrow: Option<Target>,
        lookup: impl Fn(&Target) -> Option<&'a AddressValue>,
    ) -> Option<Vec<Target>> {
        let start = ReferenceRouteValue::Locations(vec![target]);

        let ReferenceRouteValue::Locations(locations) =
            self.follow_reference_route(values, start, path, root_borrow, lookup)?
        else {
            unreachable!("an actual storage route remains in memory");
        };

        Some(locations)
    }

    fn follow_reference_route<'a>(
        &'a self,
        values: &bray_symbols::SemanticValueStore,
        mut reached: ReferenceRouteValue,
        path: &StorageReferencePath,
        mut root_borrow: Option<Target>,
        lookup: impl Fn(&Target) -> Option<&'a AddressValue>,
    ) -> Option<ReferenceRouteValue> {
        let mut selected = 0;

        for (step, operation) in path.steps().iter().copied().enumerate() {
            let index = operation.index();

            if selected != index {
                reached = reached.project(&path.projections()[selected..index]);
            }

            let (pointer, consumes) = match operation {
                StorageReferenceStep::Read { kind, .. } =>
                    (None, kind == bray_symbols::DependencyReferenceKind::Address),
                StorageReferenceStep::Backing { pointer: Some(pointer), .. } =>
                    (Some(StorageProjection::ProductField(pointer)), true),
                StorageReferenceStep::Backing { pointer: None, .. } => {
                    if matches!(reached, ReferenceRouteValue::Capture(_)) {
                        return None;
                    }

                    reached = reached.project(&[StorageProjection::OwnedTarget]);
                    selected = index + 1;
                    continue;
                }
            };

            if let Some(root) = root_borrow.take().filter(|_| step == 0 && index == 0 && !consumes) {
                reached = ReferenceRouteValue::Locations(vec![root]);
            } else {
                if let Some(pointer) = pointer {
                    reached = reached.project(&[pointer]);
                }

                let targets = match reached {
                    ReferenceRouteValue::Capture(value) => {
                        let reference = value.binding()?.reference()?;

                        self.reference_step_targets(values, operation, reference)?.iter().cloned().collect()
                    }
                    ReferenceRouteValue::Locations(locations) => {
                        let mut targets = std::collections::BTreeSet::new();

                        for location in locations {
                            let reference = lookup(&location)?;

                            targets.extend(self.reference_step_targets(values, operation, reference)?.iter().cloned());
                        }

                        targets.into_iter().collect()
                    }
                };

                // A captured descriptor is read once. Further steps read activation memory.
                reached = ReferenceRouteValue::Locations(targets);
            }

            selected = index + usize::from(consumes);
        }

        Some(if selected == path.projections().len() { reached } else {
            reached.project(&path.projections()[selected..])
        })
    }

    fn reference_step_targets<'a>(
        &self,
        values: &bray_symbols::SemanticValueStore,
        operation: StorageReferenceStep,
        reference: &'a AddressValue,
    ) -> Option<&'a std::collections::BTreeSet<Target>> {
        match operation {
            StorageReferenceStep::Read { target, .. } => self.route_pointer_targets(values, reference, target),
            StorageReferenceStep::Backing { pointer: Some(_), .. } => {
                // Backing selection follows the actual allocation descriptor without loading its elements.
                reference.has_values().then_some(&reference.targets)
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
