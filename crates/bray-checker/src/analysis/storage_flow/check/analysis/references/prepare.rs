use super::super::effects::project;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Place;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn reference_target(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<crate::analysis::storage_flow::memory::Target> {
        let logical = storage.logical_path(access)?;
        let borrows = self.borrowed_projections.get(&access)?;

        let mut target =
            crate::analysis::storage_flow::memory::Target::storage(Place::root(logical.root));

        let mut copied = 0;

        for (step, (index, ty)) in borrows.iter().enumerate() {
            target = target.project(&logical.projections[copied..*index]);

            let entry = (step == 0 && *index == 0)
                .then(|| storage.access(access)?.root().borrow_capability())
                .flatten()
                .filter(|borrow| {
                    storage
                        .borrow_capability(*borrow)
                        .is_some_and(|borrow| borrow.entry_binding().is_some())
                });

            target = match entry {
                Some(capability) => memory.borrow_target(capability, *ty)?,
                None => memory.value_target(&target, *ty)?,
            };

            copied = *index;
        }

        Some(target.project(&logical.projections[copied..]))
    }

    pub(in crate::analysis::storage_flow) fn prepare_borrowed_projections<
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
                    self.borrowed_projections.insert(access, Vec::new());
                }

                continue;
            };

            if let Some((_, borrows)) = crate::storage::reference_path(
                request,
                ty,
                &path.projections,
                Some(reached.reached_type()),
            )? {
                self.borrowed_projections.insert(access, borrows);
            }
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn prepare_entry_references<
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

            if !matches!(
                storage.identity(path.root),
                Some(
                    bray_bound_tree::StorageIdentity::Parameter(_)
                        | bray_bound_tree::StorageIdentity::Receiver(_)
                )
            ) {
                continue;
            }

            let ty = reached.reached_type();

            let kind = crate::representation::reference_kind(request, ty);

            // Root borrow inputs already have their own entry capabilities.
            if kind == Some(bray_symbols::DependencyReferenceKind::Address)
                || kind == Some(bray_symbols::DependencyReferenceKind::Borrow)
                    && !path.projections.is_empty()
            {
                let target = crate::representation::reference_target_type(request, ty);

                self.entry_references.insert(path.clone(), (access, target));
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
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (identity, source) in storage.identity_entries() {
            let bray_bound_tree::StorageIdentity::Temporary(expression) = source else {
                continue;
            };

            let Some(owner) = storage.identity_type(identity) else {
                continue;
            };

            for (projection, operand) in values.constructed_fields(expression) {
                let Some(path) = project(request, Place::root(identity), &[projection]) else {
                    continue;
                };

                let ty = crate::storage::projected_value_type(request, owner, projection)?;

                let Some(ty) = *ty.value() else {
                    continue;
                };

                self.reference_constructions
                    .entry(expression)
                    .or_default()
                    .push((operand, path, ty));
            }
        }

        Ok(())
    }
}
