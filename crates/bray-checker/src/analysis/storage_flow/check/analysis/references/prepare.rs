use super::super::effects::project;
use super::dependency_projections;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Place;
use bray_symbols::TypeData;

impl crate::analysis::storage_flow::model::StorageFlowInput {
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

            let Some(mut ty) = storage.identity_type(path.root) else {
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

            let mut borrows = Vec::new();
            let mut complete = true;

            for index in 0..=path.projections.len() {
                while let TypeData::Borrow { target, .. } =
                    request.semantic_values().type_data(ty).as_ref()
                {
                    if index == path.projections.len() && ty == reached.reached_type() {
                        break;
                    }

                    ty = *target;
                    borrows.push((index, ty));
                }

                let Some(projection) = path.projections.get(index) else {
                    break;
                };

                let Some(projections) = dependency_projections(&[*projection]) else {
                    complete = false;
                    break;
                };

                let projected = crate::storage::projected_value_type(request, ty, projections[0])?;

                let Some(projected) = *projected.value() else {
                    complete = false;
                    break;
                };

                ty = projected;
            }

            if complete {
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
                self.entry_references.insert(path.clone());
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
