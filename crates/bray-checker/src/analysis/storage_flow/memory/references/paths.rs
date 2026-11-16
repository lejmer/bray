use super::super::model::{References, State, Target};
use super::slots::storage_subtree;
use bray_bound_tree::StorageProjection;
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn reference_locations(
        &self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        root_borrow: Option<Target>,
    ) -> Option<Vec<Target>> {
        self.reference_locations_in(values, target, path, root_borrow, false,
            |target| (self.values.get(target).and_then(|value| value.references.as_ref()), self.contents.get(target).and_then(|contents| contents.storage_type)))
    }

    pub(in crate::analysis::storage_flow) fn reference_target_regions(
        &self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        root_borrow: Option<Target>,
    ) -> Option<Vec<Target>> {
        self.reference_locations_in(values, target, path, root_borrow, true,
            |target| (self.values.get(target).and_then(|value| value.references.as_ref()), self.contents.get(target).and_then(|contents| contents.storage_type)))
    }

    pub(in crate::analysis::storage_flow) fn staged_reference_locations(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
        root: &Target,
        start: &Target,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Vec<Target>> {
        self.pending.get(&invocation)?;

        self.reference_locations_in(values, start.clone(), path, None, false, |target| {
            self.staged_reference_value(invocation, root, target)
        })
    }

    pub(in crate::analysis::storage_flow) fn staged_reference_target_regions(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
        root: &Target,
        start: Target,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Vec<Target>> {
        self.pending.get(&invocation)?;

        self.reference_locations_in(values, start, path, None, true,
            |target| self.staged_reference_value(invocation, root, target))
    }

    fn staged_reference_value(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        root: &Target,
        location: &Target,
    ) -> (Option<&References>, Option<TypeId>) {
        let staged = self.pending.get(&invocation);
        let outside = !root.contains_storage(location);

        let reference = staged.and_then(|staged| staged.values.get(location))
            .or_else(|| outside.then(|| self.values.get(location)).flatten())
            .and_then(|value| value.references.as_ref());
        let ty = staged.and_then(|staged| staged.payloads.get(location))
            .or_else(|| outside.then(|| self.contents.get(location)).flatten())
            .and_then(|contents| contents.storage_type);

        (reference, ty)
    }

    pub(in crate::analysis::storage_flow) fn staged_reference_regions(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        root: &Target,
        location: &Target,
    ) -> Option<Vec<Target>> {
        self.staged_reference_value(invocation, root, location).0?.regions()
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
            ((if root.contains_storage(target) { captured.get(target) } else { self.values.get(target) }).and_then(|value| value.references.as_ref()),
                self.contents.get(target).and_then(|contents| contents.storage_type))
        };

        let locations = self.reference_locations_in(values, root.clone(), path, None, false, lookup)?;
        let [location] = locations.as_slice() else {
            return None;
        };

        let references = lookup(location).0?;

        references.is_exact().then(|| references.targets.first()
            .expect("exact captured input has one referent").clone())
    }

    pub(in crate::analysis::storage_flow) fn staged_contained_regions(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        value: &Target,
    ) -> (std::collections::BTreeSet<Target>, bool) {
        let staged = self.pending.get(&invocation).expect("captured completion references retain their prepared transfer");
        let mut targets = std::collections::BTreeSet::new();
        let mut unknown = false;

        for references in storage_subtree(&staged.values, value).filter_map(|(_, value)| value.references.as_ref()) {
            targets.extend(references.targets.iter().cloned());
            unknown |= references.unknown || references.displaced;
        }

        (targets, unknown)
    }

    fn reference_locations_in<'a>(
        &'a self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        mut root_borrow: Option<Target>,
        terminal_regions: bool,
        lookup: impl Fn(&Target) -> (Option<&'a References>, Option<TypeId>),
    ) -> Option<Vec<Target>> {
        let mut targets = vec![target];
        let mut copied = 0;

        for (step, (index, ty)) in path.borrows().iter().enumerate() {
            let address = path.is_address_load(step);
            targets = self.project_reference_targets(targets, path, copied, *index, false, &lookup)?;

            targets = if let Some(root) = root_borrow.take().filter(|_| step == 0 && *index == 0 && !address) {
                vec![root]
            } else {
                let mut reached = std::collections::BTreeSet::new();

                for target in targets {
                    let reference = lookup(&target).0?;

                    if terminal_regions && step + 1 == path.borrows().len()
                        && *index + usize::from(address) == path.projections().len() {
                        reached.extend(reference.regions()?);
                        continue;
                    }

                    let typed = reference.typed_targets(*ty, |target| lookup(target).1).or_else(|| {
                        let data = values.type_data(*ty);
                        let bray_symbols::TypeData::Slice(element) = data.as_ref() else {
                            return None;
                        };

                        // A slice view keeps each existing element region. Bounds alone
                        // cannot supply a layout or turn a dynamic selector into one cell.
                        (!reference.unknown && !reference.displaced && !reference.targets.is_empty()
                            && reference.targets.iter().all(|reached| {
                                let declared = lookup(reached).1;

                                !reached.previous && (!reference.inexact || !reached.is_exact())
                                    && (declared == Some(*ty) || declared == Some(*element))
                            }))
                            .then_some(&reference.targets)
                    })?;

                    reached.extend(typed.iter().cloned());
                }

                reached.into_iter().collect()
            };

            copied = *index + usize::from(address);
        }

        self.project_reference_targets(targets, path, copied, path.projections().len(), terminal_regions, &lookup)
    }

    fn project_reference_targets<'a>(
        &self,
        mut targets: Vec<Target>,
        path: &bray_bound_tree::StorageReferencePath,
        mut start: usize,
        end: usize,
        terminal_regions: bool,
        lookup: &impl Fn(&Target) -> (Option<&'a References>, Option<TypeId>),
    ) -> Option<Vec<Target>> {
        let first = start;

        for (index, owner) in path.owners().iter().filter(|(index, _)| *index >= first && *index < end) {
            let mut reached = std::collections::BTreeSet::new();

            for target in targets {
                let target = target.project(&path.projections()[start..*index]);

                match owner {
                    bray_bound_tree::StorageReferenceOwner::Indirection => {
                        reached.insert(target.project(&[StorageProjection::OwnedTarget]));
                    }
                    bray_bound_tree::StorageReferenceOwner::Buffer { pointer, element, .. } => {
                        let reference = lookup(&target.project(&[StorageProjection::ProductField(*pointer)])).0?;

                        if terminal_regions && *index + 1 == path.projections().len() {
                            reached.extend(reference.regions()?);
                        } else {
                            reached.extend(reference.typed_targets(*element, |target| lookup(target).1)?.iter().cloned());
                        }
                    }
                }
            }

            targets = reached.into_iter().collect();
            start = *index + 1;
        }

        Some(targets.into_iter().map(|target| target.project(&path.projections()[start..end])).collect())
    }

}
