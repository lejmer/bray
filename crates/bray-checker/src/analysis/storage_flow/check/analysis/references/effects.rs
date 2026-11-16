use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::StorageReferencePath;
use bray_symbols::{DependencyMutationKind, DependencyProjection, DependencyStorageEffect, DependencySubject, DependencySubjectRoot, TypeId};

use super::super::core::StorageFlowCollector;
use crate::{CheckerRequestContext, CheckerUnitView};
use crate::analysis::storage_flow::memory::{Effect, Source, State, Target};
use crate::analysis::storage_flow::model::StorageFlowInput;

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn effect_target_path(
        &self,
        ty: Option<TypeId>,
        projections: &[DependencyProjection],
    ) -> Option<&StorageReferencePath> {
        self.effect_target_paths.get(&ty?)?.get(projections)?.as_ref()
    }

    pub(in crate::analysis::storage_flow) fn prepare_effect_target_paths<'a, C: CheckerRequestContext + ?Sized>(
        request: CheckerUnitView<'_, C>,
        paths: &mut BTreeMap<TypeId, BTreeMap<Arc<[DependencyProjection]>, Option<StorageReferencePath>>>,
        effects: impl IntoIterator<Item = &'a DependencyStorageEffect>,
        reference_type: impl Fn(&DependencySubject) -> Option<TypeId>,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for effect in effects {
            let DependencyStorageEffect::Indirect { reference, projections, .. } = effect else {
                continue;
            };

            if projections.is_empty() {
                continue;
            }

            let Some(ty) = reference_type(reference) else {
                continue;
            };

            let entry = paths.entry(ty).or_default();

            if !entry.contains_key(projections.as_ref()) {
                entry.insert(Arc::clone(projections), crate::storage::reference_target_path(request, ty, projections)?.map(|(_, path)| path));
            }
        }

        Ok(())
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[DependencyStorageEffect],
    ) -> Vec<Effect> {
        self.resolve_effects(effects, |subject, indirect, projections| {
            if !indirect {
                return self.invocation_locations(state, occurrence, subject);
            }

            let sources = if subject.projections().is_empty()
                && !matches!(subject.subject_root(), DependencySubjectRoot::ProductStatic(_) | DependencySubjectRoot::ExactThreadStatic(_)) {
                vec![self.invocation_reference(state, occurrence, subject.subject_root())?]
            } else {
                self.invocation_locations(state, occurrence, subject)?.into_iter()
                    .map(Source::Value).collect()
            };

            self.effect_reference_targets(&state.memory, sources,
                self.dependencies.calls().get(&occurrence).and_then(|call| call.reference_type(subject)), projections)
        })
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_effects(
        &self,
        effects: &[DependencyStorageEffect],
        mut resolve: impl FnMut(&DependencySubject, bool, &[DependencyProjection]) -> Option<Vec<Target>>,
    ) -> Vec<Effect> {
        let mut resolved = Vec::new();

        for effect in effects {
            let (subject, indirect, projections, kind, access_type) = match effect {
                DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient } => {
                    resolved.push(Effect::unbounded(*kind, *access_type, *ambient));
                    continue;
                }
                DependencyStorageEffect::Escape => {
                    resolved.push(Effect::Escape);
                    continue;
                }
                DependencyStorageEffect::Ambient => {
                    resolved.push(Effect::Ambient);
                    continue;
                }
                DependencyStorageEffect::Unknown => {
                    resolved.push(Effect::Unknown);
                    continue;
                }
                DependencyStorageEffect::Atomic => {
                    resolved.push(Effect::Atomic);
                    continue;
                }
                DependencyStorageEffect::Direct { subject, kind, access_type } => {
                    (subject, false, &[][..], *kind, *access_type)
                }
                DependencyStorageEffect::Indirect {
                    reference,
                    projections,
                    kind,
                    access_type,
                } => (reference, true, projections.as_ref(), *kind, *access_type),
            };

            let Some(targets) = resolve(subject, indirect, projections) else {
                if kind == DependencyMutationKind::Atomic {
                    resolved.push(Effect::Atomic);
                    continue;
                }

                resolved.push(Effect::unbounded(kind, access_type, false));

                continue;
            };

            for target in targets {
                resolved.push(Effect::Mutation(target, kind, access_type));
            }
        }

        resolved.sort_unstable();
        resolved.dedup();

        resolved
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn effect_reference_targets(
        &self,
        memory: &State,
        sources: impl IntoIterator<Item = Source>,
        ty: Option<TypeId>,
        projections: &[DependencyProjection],
    ) -> Option<Vec<Target>> {
        let path = self.input.effect_target_path(ty, projections);

        sources.into_iter().try_fold(Vec::new(), |mut targets, source| {
            targets.extend(if let Some(path) = path {
                source.reference_locations(self.request.semantic_values(), memory, path, true)?
            } else {
                source.regions(memory).filter(|regions| projections.is_empty() || regions.is_empty())?
            });

            Some(targets)
        })
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn staged_effect_reference_targets(
        &self,
        memory: &State,
        invocation: bray_bound_tree::BoundExecutionSite,
        root: &Target,
        locations: impl IntoIterator<Item = Target>,
        ty: Option<TypeId>,
        projections: &[DependencyProjection],
    ) -> Option<Vec<Target>> {
        let path = self.input.effect_target_path(ty, projections);

        locations.into_iter().try_fold(Vec::new(), |mut targets, location| {
            targets.extend(if let Some(path) = path {
                memory.staged_reference_target_regions(invocation, self.request.semantic_values(), root, location, path)?
            } else {
                memory.staged_reference_regions(invocation, root, &location).filter(|regions| projections.is_empty() || regions.is_empty())?
            });

            Some(targets)
        })
    }
}
