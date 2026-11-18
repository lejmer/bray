use crate::CheckerRequestContext;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_buffer_fields<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
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

            self.raw_buffers.insert(ty, fields);
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn prepare_buffer_cleanup<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        types: impl Iterator<Item = bray_symbols::TypeId>,
        demands: &mut super::entry::Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut nested = std::collections::BTreeSet::new();

        for ty in types.collect::<std::collections::BTreeSet<_>>() {
            if !self.raw_buffers.contains_key(&ty) {
                continue;
            }

            let effects = crate::dependency::resolve_cleanup_effects(request, &[
                bray_symbols::DependencyStorageEffect::Direct { addressed: true,
                    subject: bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Receiver),
                    kind: bray_symbols::DependencyMutationKind::Dispose(ty), access_type: Some(ty),
                },
            ])?;

            for effect in &effects {
                let subject = match effect {
                    bray_symbols::DependencyStorageEffect::Direct { subject, .. } => subject,
                    bray_symbols::DependencyStorageEffect::Indirect { reference, .. } => reference,
                    _ => continue,
                };

                let owner = if subject.subject_root() == bray_symbols::DependencySubjectRoot::Receiver {
                    ty
                } else if let Some(ty) = crate::storage::static_subject_type(request, subject.subject_root())? {
                    ty
                } else {
                    continue;
                };

                let Some(projections) = crate::storage::resolve_storage_projections(request, subject.projections())? else {
                    continue;
                };

                let Some((reached, path)) = crate::storage::reference_path(request, owner, projections.into(), None)? else {
                    continue;
                };

                // The same guarded cleanup expansion supplies both runtime effects and
                // finite descriptor demands. Owning elements never become parent edges.
                demands.entry(owner).or_default().insert((path.projections().into(), reached));

                for end in 0..=path.projections().len() {
                    if let Some((prefix, _)) = crate::storage::reference_path(request, owner, path.projections()[..end].into(), None)? {
                        nested.insert(prefix);
                    }
                }

                self.buffer_cleanup_paths.insert((ty, subject.clone()), (reached, path));
            }

            self.buffer_cleanup_effects.insert(ty, effects);
        }

        self.prepare_buffer_fields(request, nested.into_iter(), demands)
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

            for subject in
                dependencies.lifecycle_dependencies()[&lifecycle.call.callable].reference_subjects()
            {
                let key = (lifecycle.call.callable, subject.clone());

                if self.lifecycle_paths.contains_key(&key) {
                    continue;
                }

                let owner = if subject.subject_root() == bray_symbols::DependencySubjectRoot::Receiver {
                    *ty
                } else if let Some(ty) = crate::storage::static_subject_type(request, subject.subject_root())? {
                    ty
                } else {
                    continue;
                };

                let Some(projections) =
                    crate::storage::resolve_storage_projections(request, subject.projections())?
                else {
                    continue;
                };

                // The actual cleanup receiver already denotes its physical storage.
                if let Some((ty, path)) =
                    crate::storage::reference_path(request, owner, projections.into(), None)?
                {
                    demands
                        .entry(owner)
                        .or_default()
                        .insert((std::sync::Arc::from(path.projections()), ty));

                    self.lifecycle_paths.insert(key, (ty, path));
                }
            }

            let references = crate::dependency::prepare_references(
                request,
                &dependencies.lifecycle_dependencies()[&lifecycle.call.callable].borrows(),
                |subject| self.lifecycle_paths.get(&(lifecycle.call.callable, subject.clone()))
                    .map(|(ty, _)| *ty),
            )?;

            self.lifecycle_references
                .insert(lifecycle.call.callable, references);

            let stored = crate::dependency::prepare_stored_value_paths(
                request.semantic_values(),
                dependencies.lifecycle_dependencies()[&lifecycle.call.callable].captured_values().iter()
                    .flat_map(|outcome| outcome.stored_values.iter()),
                |subject| self.lifecycle_paths.get(&(lifecycle.call.callable, subject.clone())).cloned(),
            );

            self.lifecycle_stored_paths.insert(lifecycle.call.callable, stored);
        }

        Ok(demands)
    }

}
