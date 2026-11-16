use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{StorageReferenceOwner, StorageReferencePath};
use bray_symbols::{DependencyStorageEffect, DependencySubject, DependencySubjectRoot, TypeId};
use crate::CheckerRequestContext;

pub(super) type Demands = BTreeMap<
    bray_symbols::TypeId,
    BTreeSet<(
        Arc<[bray_bound_tree::StorageProjection]>,
        bray_symbols::TypeId,
    )>,
>;

type ParentPaths = BTreeMap<
    bray_symbols::TypeId,
    BTreeSet<(
        bray_symbols::TypeId,
        Arc<[bray_bound_tree::StorageProjection]>,
    )>,
>;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(super) fn close_entry_demands<C: CheckerRequestContext + ?Sized>(
        request: crate::CheckerUnitView<'_, C>,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut parents = ParentPaths::new();

        for (root, paths) in demands.iter() {
            for (path, _) in paths {
                let mut ty = *root;

                for (index, projection) in path.iter().enumerate() {
                    // Only inline containment transports demands, including protected
                    // payloads. References and ownership lead to finite explicit demands.
                    if matches!(projection, bray_bound_tree::StorageProjection::OwnedTarget | bray_bound_tree::StorageProjection::AddressTarget)
                        || crate::representation::reference_kind(request, ty).is_some() {
                        break;
                    }

                    let projected = crate::storage::projected_storage_type(request, ty, *projection)?;

                    let Some(projected) = *projected.value() else {
                        break;
                    };

                    ty = projected;

                    if crate::representation::reference_kind(request, ty).is_some() {
                        break;
                    }

                    parents.entry(ty).or_default().insert((*root, path[..=index].into()));
                }
            }
        }

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

    pub(in crate::analysis::storage_flow) fn prepare_summary_demands<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        values: &crate::dependency::ValueInputs,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut paths = std::collections::BTreeSet::new();

        for (site, contracts) in dependencies.calls() {
            for subject in contracts.reference_subjects() {
                let input = site
                    .expression()
                    .and_then(|expression| selections.expression(expression))
                    .and_then(|selection| selection.invocation_input_site(*site, subject.subject_root()));

                let staged = input
                    .and_then(|input| match input {
                        bray_bound_tree::BoundExecutionSite::CallInput {
                            expression,
                            ordinal,
                        } => storage.invocation_input(expression, ordinal),
                        _ => None,
                    })
                    .and_then(|access| storage.logical_path(access));

                let source = input.or_else(|| super::invocation::invocation_argument(
                    request.view(), selections, *site, subject.subject_root(),
                ).map(Into::into)).and_then(|input| {
                    values.projected_input_value(request.semantic_values(), selections, input, subject.projections())
                });

                let source_projections = if let Some((_, projections)) = source {
                    crate::storage::resolve_storage_projections(request, projections)?
                } else {
                    None
                };

                let path = contracts.reference_path(subject).expect("reference subject retains its prepared path");

                let mut bases = staged.into_iter().filter_map(|base| {
                    Some((storage.identity_type(base.root)?, base.projections.as_ref(), path.projections()))
                }).collect::<Vec<_>>();

                if let Some((source, _)) = source
                    && let Some(projections) = source_projections.as_deref()
                {
                    bases.extend(self.expression_places(source).iter().filter_map(|base| {
                        Some((storage.identity_type(base.root)?, base.projections.as_ref(), projections))
                    }));
                }

                if bases.is_empty() {
                    let owner = if subject.subject_root() == DependencySubjectRoot::Result {
                        contracts.reference_type(&DependencySubject::root(DependencySubjectRoot::Result))
                    } else {
                        crate::storage::static_subject_type(request, subject.subject_root())?
                    };

                    let Some(owner) = owner else {
                        continue;
                    };

                    bases.push((owner, &[][..], path.projections()));
                }

                let effects = contracts.effects().iter()
                    .chain(contracts.deferred_effects().into_iter().flatten())
                    .chain(contracts.deferred_capture_cleanup().into_iter().flatten())
                    .chain(contracts.deferred_result_cleanup().into_iter().flatten());
                let ty = contracts.reference_type(subject).expect("reference subject has its checked type");
                let targets = self.reference_target_paths(subject, ty, contracts.references(), effects)
                    .map(StorageReferencePath::projections).collect::<Vec<_>>();

                for (owner, prefix, selected) in bases {
                    for suffix in std::iter::once(&[][..]).chain(targets.iter().copied()) {
                        let projections = prefix.iter().chain(selected).chain(suffix).copied().collect::<Arc<[_]>>();

                        paths.insert((owner, projections));
                    }
                }
            }
        }

        for (owner, path) in paths {
            self.prepare_path_demands(request, owner, path, None, demands)?;
        }

        Ok(())
    }

    fn reference_target_paths<'a>(
        &'a self,
        subject: &'a DependencySubject,
        ty: TypeId,
        references: &'a [bray_bound_tree::BoundCallReference],
        effects: impl Iterator<Item = &'a DependencyStorageEffect> + 'a,
    ) -> impl Iterator<Item = &'a StorageReferencePath> {
        references.iter().filter(move |reference| reference.relation().entry() == Some(subject))
            .filter_map(bray_bound_tree::BoundCallReference::target_path)
            .chain(effects.filter_map(move |effect| {
                let DependencyStorageEffect::Indirect { reference, projections, .. } = effect else {
                    return None;
                };

                (reference == subject).then(|| self.effect_target_path(Some(ty), projections)).flatten()
            }))
    }

    pub(super) fn prepare_path_demands<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        owner: TypeId,
        projections: Arc<[bray_bound_tree::StorageProjection]>,
        reached: Option<TypeId>,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let Some((reached, path)) = crate::storage::reference_path(request, owner, projections, reached)? else {
            return Ok(());
        };

        let owner = request.semantic_values().unborrowed_type(owner);
        let mut buffers = BTreeSet::new();

        demands.entry(owner).or_default().insert((path.projections().into(), reached));

        for (index, policy) in path.owners() {
            let StorageReferenceOwner::Buffer { ty, pointer, .. } = policy else {
                continue;
            };

            let projection = bray_bound_tree::StorageProjection::ProductField(*pointer);
            let pointer_type = crate::storage::projected_storage_type(request, *ty, projection)?
                .value().expect("declared buffer pointer has its checked type");
            let projections = path.projections()[..*index].iter().copied().chain([projection]).collect();

            demands.entry(owner).or_default().insert((projections, pointer_type));
            buffers.insert(*ty);
        }

        self.prepare_buffer_fields(request, buffers.into_iter(), demands)
    }

    pub(super) fn prepare_cleanup_target_demands<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut paths = BTreeSet::new();

        for ((ty, _), lifecycle) in &cleanup.lifecycles {
            let callable = lifecycle.call.callable;
            let contract = &dependencies.lifecycle_dependencies()[&callable];
            let references = &self.lifecycle_references[&callable];

            for subject in contract.reference_subjects() {
                let Some((source_type, source)) = self.lifecycle_paths.get(&(callable, subject.clone())) else {
                    continue;
                };

                let owner = if subject.subject_root() == DependencySubjectRoot::Receiver {
                    *ty
                } else if let Some(ty) = crate::storage::static_subject_type(request, subject.subject_root())? {
                    ty
                } else {
                    continue;
                };

                let targets = std::iter::once(&[][..]).chain(self.reference_target_paths(subject, *source_type, references,
                    contract.effects().iter()).map(StorageReferencePath::projections));

                for suffix in targets {
                    paths.insert((owner, source.projections().iter().chain(suffix).copied().collect::<Arc<[_]>>()));
                }
            }
        }

        for ((receiver, subject), (ty, source)) in &self.buffer_cleanup_paths {
            let owner = if subject.subject_root() == DependencySubjectRoot::Receiver {
                *receiver
            } else if let Some(ty) = crate::storage::static_subject_type(request, subject.subject_root())? {
                ty
            } else {
                continue;
            };

            let targets = std::iter::once(&[][..]).chain(self.reference_target_paths(subject, *ty, &[],
                self.buffer_cleanup_effects[receiver].iter()).map(StorageReferencePath::projections));

            for suffix in targets {
                paths.insert((owner, source.projections().iter().chain(suffix).copied().collect::<Arc<[_]>>()));
            }
        }

        for (owner, path) in paths {
            self.prepare_path_demands(request, owner, path, None, demands)?;
        }

        Ok(())
    }

}
