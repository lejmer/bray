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
            if self.raw_buffers.contains_key(&ty) { continue; }

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

            let mut subjects = std::collections::BTreeMap::new();

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

                for (prefix, pointer) in path.backing_descriptors() {
                    let descriptor = prefix.iter().copied()
                        .chain([bray_bound_tree::StorageProjection::ProductField(pointer)])
                        .collect::<std::sync::Arc<[_]>>();

                    if let Some((ty, _)) = crate::storage::reference_path(request, owner, descriptor.clone(), None)? {
                        demands.entry(owner).or_default().insert((descriptor, ty));
                    }
                }

                for end in 0..=path.projections().len() {
                    if let Some((prefix, _)) = crate::storage::reference_path(request, owner, path.projections()[..end].into(), None)? {
                        nested.insert(prefix);
                    }
                }

                subjects.insert(subject.clone(), (reached, path));
            }

            let paths = crate::dependency::prepare_effect_paths(request, &effects, |subject| {
                subjects.get(subject).map(|(ty, _)| *ty)
            })?;

            let storage = bray_bound_tree::BoundCallStorage::new(subjects, [], Default::default()).with_effect_paths(paths);

            self.buffer_cleanup_effects.insert(ty, effects);
            self.buffer_cleanup_storage.insert(ty, storage);
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
            let callable = lifecycle.call.callable;

            if self.lifecycle_storage.contains_key(&callable) {
                continue;
            }

            let template = &dependencies.lifecycle_dependencies()[&callable];

            let storage = crate::dependency::prepare_call_storage(request, template, [], [], [], |subject| {
                // The selected cleanup receiver already denotes its physical storage.
                let owner = if subject.subject_root() == bray_symbols::DependencySubjectRoot::Receiver {
                    *ty
                } else if let Some(ty) = crate::storage::static_subject_type(request, subject.subject_root())? {
                    ty
                } else {
                    return Ok(None);
                };

                let prepared = crate::dependency::reference_subject_path(request, subject, owner)?;

                if let Some((ty, path)) = &prepared {
                    demands.entry(owner).or_default().insert((std::sync::Arc::from(path.projections()), *ty));
                }

                Ok(prepared)
            })?;

            self.lifecycle_storage.insert(callable, storage);
        }

        Ok(demands)
    }

}
