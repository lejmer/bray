use super::super::effects::project;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Place;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_lifecycle_references<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for ((ty, _), lifecycle) in &cleanup.lifecycles {
            if self.lifecycle_references.contains_key(&lifecycle.callable) {
                continue;
            }

            let mut types = std::collections::BTreeMap::new();

            for subject in
                dependencies.lifecycle_dependencies()[&lifecycle.callable].reference_subjects()
            {
                let key = (lifecycle.callable, subject.clone());

                if subject.subject_root() != bray_symbols::DependencySubjectRoot::Receiver
                    || self.lifecycle_paths.contains_key(&key)
                {
                    continue;
                }

                let Some(projections) =
                    crate::storage::storage_projections(request, subject.projections())
                else {
                    continue;
                };

                // The actual cleanup receiver already denotes its physical storage.
                let ty = *ty;

                if let Some((ty, path)) =
                    crate::storage::reference_path(request, ty, projections.into(), None)?
                {
                    types.insert(subject.clone(), ty);
                    self.lifecycle_paths.insert(key, path);
                }
            }

            let mut references = Vec::new();

            for relation in dependencies.lifecycle_dependencies()[&lifecycle.callable].borrows() {
                if let Some(ty) = types.get(relation.entry())
                    && let Some(reference) =
                        crate::dependency::prepare_reference(request, relation, *ty, None)?
                {
                    references.push(reference);
                }
            }

            self.lifecycle_references
                .insert(lifecycle.callable, references);
        }

        Ok(())
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

            if let Some((ty, prepared)) = crate::storage::reference_path(
                request,
                ty,
                path.projections.clone(),
                Some(reached.reached_type()),
            )? {
                self.prepare_entry_reference_path(request, storage, path.clone(), ty, &prepared);
                self.reference_paths.insert(access, prepared);
            }
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn prepare_summary_references<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut paths = std::collections::BTreeSet::new();

        for (site, contracts) in dependencies.calls() {
            let Some(bray_bound_tree::SemanticSelection::Call(call)) = site
                .expression()
                .and_then(|expression| selections.expression(expression))
            else {
                continue;
            };

            for subject in contracts.reference_subjects() {
                let Some(argument) =
                    crate::dependency::result_argument(call, subject.subject_root())
                else {
                    continue;
                };

                let Some(base) = self.expression_places(argument).first() else {
                    continue;
                };

                if !matches!(
                    storage.identity(base.root),
                    Some(
                        bray_bound_tree::StorageIdentity::Parameter(_)
                            | bray_bound_tree::StorageIdentity::Receiver(_)
                    )
                ) {
                    continue;
                }

                if let Some(path) = project(request, base.clone(), subject.projections()) {
                    paths.insert(path);
                }
            }
        }

        for path in paths {
            let ty = storage
                .identity_type(path.root)
                .expect("entry storage has its declared type");

            let Some((ty, prepared)) =
                crate::storage::reference_path(request, ty, path.projections.clone(), None)?
            else {
                continue;
            };

            self.prepare_entry_reference_path(request, storage, path, ty, &prepared);
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
        if !matches!(
            storage.identity(path.root),
            Some(
                bray_bound_tree::StorageIdentity::Parameter(_)
                    | bray_bound_tree::StorageIdentity::Receiver(_)
            )
        ) {
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
                        bray_bound_tree::StorageReferencePath::new(
                            prefix.projections,
                            prepared.borrows()[..step].into(),
                        ),
                        Some(*target),
                    )
                });
        }

        let layer = 1 + prepared
            .borrows()
            .iter()
            .rev()
            .take_while(|(index, _)| *index == path.projections.len())
            .count();

        if is_entry_reference(request, ty, &path)
            || layer > 1 && crate::representation::reference_kind(request, ty).is_some()
        {
            let target = crate::representation::reference_target_type(request, ty);

            self.entry_references
                .insert((path, layer), (prepared.clone(), target));
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

fn is_entry_reference<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    ty: bray_symbols::TypeId,
    path: &Place,
) -> bool {
    // Root borrow inputs already have their own entry capabilities.
    match crate::representation::reference_kind(request, ty) {
        Some(bray_symbols::DependencyReferenceKind::Address) => true,
        Some(bray_symbols::DependencyReferenceKind::Borrow) => !path.projections.is_empty(),
        None => false,
    }
}
