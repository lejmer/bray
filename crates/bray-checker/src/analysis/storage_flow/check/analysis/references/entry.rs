use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::CheckerRequestContext;

use crate::analysis::storage_flow::memory::Place;

pub(super) type Demands = BTreeMap<
    bray_symbols::TypeId,
    BTreeSet<(
        Arc<[bray_bound_tree::StorageProjection]>,
        bray_symbols::TypeId,
    )>,
>;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_entry_references<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        mut demands: Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut parents = BTreeMap::<_, BTreeSet<_>>::new();

        for (access, prepared) in &self.reference_paths {
            let Some(root) = storage
                .root_identity(*access)
                .and_then(|root| storage.identity_type(root))
            else {
                continue;
            };

            let root = request.semantic_values().unborrowed_type(root);

            let ty = storage
                .access(*access)
                .expect("prepared reference has a checked access")
                .reached_type();

            let projections =
                Arc::<[bray_bound_tree::StorageProjection]>::from(prepared.projections());

            demands
                .entry(root)
                .or_default()
                .insert((Arc::clone(&projections), ty));

            // Only inline containment transports demands. Following references or owned
            // indirection here would grow recursive paths instead of preparing finite slots.
            if !prepared.projections().is_empty()
                && !prepared
                    .projections()
                    .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                && prepared.borrows().iter().all(|(index, _)| *index == 0)
                && crate::representation::reference_kind(request, ty).is_none()
            {
                parents.entry(ty).or_default().insert((
                    root,
                    Arc::<[bray_bound_tree::StorageProjection]>::from(prepared.projections()),
                ));
            }
        }

        self.prepare_summary_demands(request, storage, selections, dependencies, &mut demands)?;
        Self::close_entry_demands(request, &mut demands, parents)?;

        self.install_entry_references(request, storage, &demands)
    }

    fn close_entry_demands<C: CheckerRequestContext + ?Sized>(
        request: crate::CheckerUnitView<'_, C>,
        demands: &mut Demands,
        mut parents: BTreeMap<
            bray_symbols::TypeId,
            BTreeSet<(
                bray_symbols::TypeId,
                Arc<[bray_bound_tree::StorageProjection]>,
            )>,
        >,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let components = bray_base::strongly_connected_components(parents.keys().copied(), |ty| {
            parents
                .get(&ty)
                .into_iter()
                .flatten()
                .map(|(parent, _)| *parent)
                .collect::<Vec<_>>()
        })
        .into_iter()
        .enumerate()
        .flat_map(|(component, types)| types.into_iter().map(move |ty| (ty, component)))
        .collect::<BTreeMap<_, _>>();

        // Recovered recursive inline types must not expand paths indefinitely. Their
        // representation diagnostic belongs to type analysis, not demand preparation.
        for (ty, containing) in &mut parents {
            containing.retain(|(parent, _)| components[ty] != components[parent]);
        }

        // Well-formed inline type containment is acyclic. Each demanded path is prepared
        // once, independent of how many whole-value moves lead to its eventual use.
        let mut pending = demands
            .iter()
            .flat_map(|(root, paths)| {
                paths
                    .iter()
                    .map(move |(path, reached)| (*root, Arc::clone(path), *reached))
            })
            .collect::<Vec<_>>();

        while let Some((ty, path, reached)) = pending.pop() {
            if request.is_cancelled() {
                return Err(crate::CheckerQueryError::Cancelled);
            }

            for (parent, prefix) in parents.get(&ty).into_iter().flatten() {
                let path = prefix
                    .iter()
                    .chain(path.iter())
                    .copied()
                    .collect::<std::sync::Arc<[_]>>();

                if demands
                    .entry(*parent)
                    .or_default()
                    .insert((Arc::clone(&path), reached))
                {
                    pending.push((*parent, path, reached));
                }
            }
        }

        Ok(())
    }

    fn install_entry_references<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        demands: &Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (identity, source) in storage.identity_entries() {
            let temporary = matches!(
                source,
                bray_bound_tree::StorageIdentity::Temporary(_)
                    | bray_bound_tree::StorageIdentity::InvocationInput { .. }
            );

            if !temporary
                && !matches!(
                    source,
                    bray_bound_tree::StorageIdentity::Parameter(_)
                        | bray_bound_tree::StorageIdentity::Receiver(_)
                        | bray_bound_tree::StorageIdentity::ContractParameter(_)
                        | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
                )
            {
                continue;
            }

            let Some(ty) = storage.storage_type(identity) else {
                continue;
            };

            let owner = request.semantic_values().unborrowed_type(ty);

            for (projections, reached) in demands.get(&owner).into_iter().flatten() {
                let Some((reached, prepared)) = crate::storage::reference_path(
                    request,
                    ty,
                    Arc::clone(projections),
                    Some(*reached),
                )?
                else {
                    continue;
                };

                if temporary {
                    let reference = prepared
                        .borrows()
                        .first()
                        .map(|(index, target)| {
                            (
                                Place {
                                    root: identity,
                                    projections: projections[..*index].into(),
                                },
                                *target,
                            )
                        })
                        .or_else(|| {
                            crate::representation::reference_target_type(request, reached).map(
                                |target| {
                                    (
                                        Place {
                                            root: identity,
                                            projections: Arc::clone(projections),
                                        },
                                        target,
                                    )
                                },
                            )
                        });

                    if let Some((path, target)) = reference {
                        self.returned_references
                            .entry(identity)
                            .or_default()
                            .insert(path, target);
                    }

                    continue;
                }

                self.prepare_entry_reference_path(
                    request,
                    storage,
                    Place {
                        root: identity,
                        projections: Arc::clone(projections),
                    },
                    reached,
                    &prepared,
                );
            }
        }

        Ok(())
    }

    fn prepare_summary_demands<C: CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut paths = std::collections::BTreeSet::new();

        for (site, contracts) in dependencies.calls() {
            for subject in contracts.reference_subjects() {
                let staged = site
                    .expression()
                    .and_then(|expression| selections.expression(expression))
                    .filter(|selection| selection.evaluates_defaults())
                    .and_then(|selection| {
                        selection.invocation_input_site(*site, subject.subject_root())
                    })
                    .and_then(|input| match input {
                        bray_bound_tree::BoundExecutionSite::CallInput {
                            expression,
                            ordinal,
                        } => storage.invocation_input(expression, ordinal),
                        _ => None,
                    })
                    .and_then(|access| storage.logical_path(access));

                let source = super::invocation::invocation_argument(
                    request.view(),
                    selections,
                    *site,
                    subject.subject_root(),
                )
                .and_then(|argument| self.expression_places(argument).first());

                let Some(base) = staged.or(source) else {
                    continue;
                };

                let path = contracts
                    .reference_path(subject)
                    .expect("reference subject retains its prepared path");

                let mut place = base.clone();

                place.projections = base
                    .projections
                    .iter()
                    .copied()
                    .chain(path.projections().iter().copied())
                    .collect();

                paths.insert(place);
            }
        }

        for path in paths {
            let Some(ty) = storage.identity_type(path.root) else {
                // Synthesized result slots can exist before their value type is installed.
                continue;
            };

            let owner = request.semantic_values().unborrowed_type(ty);

            let Some((ty, _)) =
                crate::storage::reference_path(request, ty, path.projections.clone(), None)?
            else {
                continue;
            };

            demands
                .entry(owner)
                .or_default()
                .insert((path.projections, ty));
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
                    | bray_bound_tree::StorageIdentity::ContractParameter(_)
                    | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
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
