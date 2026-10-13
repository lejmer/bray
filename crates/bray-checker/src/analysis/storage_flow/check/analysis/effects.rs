use bray_bound_tree::{StorageIdentity, StorageInvalidation, StoragePath};
use bray_symbols::{
    DependencyMutationKind, DependencyProjection, DependencyStorageEffect, DependencySubject,
    DependencySubjectRoot, SymbolOrdinal, TypeData,
};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};
use crate::analysis::storage_flow::memory::{Origin, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::storage::storage_projections;
use bray_bound_tree::StorageAccessPurpose;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn begin_transition(
        &self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
        assignment: Option<&Target>,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> Option<Vec<(Target, DependencyMutationKind)>> {
        self.prepare_assignment_reference(state, operation, assignment);

        if let Some(expression) = operation.occurrence().expression()
            && self.memory.operation(expression).is_some()
        {
            return self.memory_effects(state, expression);
        }

        let invocation = self.call_attempt(operation)?;

        if let Some(expression) = invocation.expression()
            && matches!(
                self.request.view().expression(expression),
                Some(bray_bound_tree::BoundExpression::Await(_))
            )
        {
            let source = self.execution.awaited_invocation(execution, expression)?;

            return state.memory.deferred_effects(source);
        }

        self.prepare_call_references(state, invocation);

        let contracts = self.dependencies.calls().get(&invocation)?;

        if let Some(effects) = contracts.deferred_effects() {
            let effects = self.call_effects(state, invocation, effects);

            state.memory.defer_effects(invocation, effects);
        }

        self.call_effects(state, invocation, contracts.effects())
    }

    fn call_attempt(
        &self,
        operation: AnalysisOperationKind,
    ) -> Option<bray_bound_tree::BoundExecutionSite> {
        match operation {
            AnalysisOperationKind::Suspension {
                occurrence,
                kind: crate::analysis::model::AnalysisSuspensionKind::Await,
            } => Some(occurrence),
            AnalysisOperationKind::Call {
                invocation,
                phase: AnalysisCallPhase::Attempt,
            } => Some(invocation),
            AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(
                expression,
            )) => {
                // Calls without an abnormal edge complete in one graph operation.
                let occurrence = expression.into();

                self.dependencies
                    .calls()
                    .contains_key(&occurrence)
                    .then_some(occurrence)
            }
            _ => None,
        }
    }

    pub(super) fn record_operation_effects(
        &mut self,
        kind: AnalysisOperationKind,
        call_effects: Option<&[(Target, DependencyMutationKind)]>,
        assignment: Option<&Target>,
    ) {
        if !self.publish {
            return;
        }

        if let Some(target) = assignment {
            self.record_target_effect(target, DependencyMutationKind::Write);
        } else if let Some(effects) = call_effects {
            for (target, kind) in effects {
                self.record_target_effect(target, *kind);
            }
        } else if matches!(
            kind,
            AnalysisOperationKind::Suspension {
                kind: crate::analysis::model::AnalysisSuspensionKind::Await,
                ..
            }
        ) {
            self.record_effects(&StorageInvalidation::All(Default::default()), DependencyMutationKind::Retire);
        } else if let Some(effects) = self.refinements.invalidation_at(kind.occurrence()) {
            let effect_kind = if self.call_attempt(kind).is_some() {
                bray_symbols::DependencyMutationKind::Retire
            } else if self.storage.node_plans(kind.node()).any(|plan| {
                matches!(
                    self.storage.effective_purpose(plan),
                    StorageAccessPurpose::Move | StorageAccessPurpose::ValueTransfer
                )
            }) {
                bray_symbols::DependencyMutationKind::Consume
            } else {
                bray_symbols::DependencyMutationKind::Write
            };

            self.record_effects(effects, effect_kind);
        }
    }

    pub(super) fn apply_call_effects(
        &self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        call_effects: Option<&[(Target, DependencyMutationKind)]>,
        invalidation: Option<&StorageInvalidation>,
    ) {
        if self.call_attempt(kind).is_some()
            && kind
                .occurrence()
                .expression()
                .is_some_and(|expression| self.memory.operation(expression).is_none())
        {
            if let Some(effects) = call_effects {
                for (target, kind) in effects {
                    state.memory.invalidate_target(target, *kind);
                }
            } else if let Some(effects) =
                invalidation.or_else(|| self.refinements.invalidation_at(kind.occurrence()))
            {
                state.memory.invalidate(
                    effects,
                    self.storage,
                    bray_symbols::DependencyMutationKind::Retire,
                );
            }
        }
    }

    fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[DependencyStorageEffect],
    ) -> Option<Vec<(Target, DependencyMutationKind)>> {
        self.resolve_effects(effects, |subject, indirect| {
            if indirect && subject.projections().is_empty() {
                self.invocation_reference(state, occurrence, subject.subject_root())?
                    .regions(&state.memory)
            } else {
                let locations = self.invocation_locations(state, occurrence, subject)?;

                if indirect {
                    locations
                        .iter()
                        .try_fold(Vec::new(), |mut targets, location| {
                            targets.extend(state.memory.referenced_regions(location)?);

                            Some(targets)
                        })
                } else {
                    Some(locations)
                }
            }
        })
    }

    pub(super) fn resolve_effects(
        &self,
        effects: &[DependencyStorageEffect],
        mut resolve: impl FnMut(&DependencySubject, bool) -> Option<Vec<Target>>,
    ) -> Option<Vec<(Target, DependencyMutationKind)>> {
        let mut resolved = Vec::new();

        for effect in effects {
            let (subject, indirect, projections, kind) = match effect {
                DependencyStorageEffect::Unknown => return None,
                DependencyStorageEffect::Direct { subject, kind } => {
                    (subject, false, &[][..], *kind)
                }
                DependencyStorageEffect::Indirect {
                    reference,
                    projections,
                    kind,
                } => (reference, true, projections.as_ref(), *kind),
            };

            let suffix = storage_projections(self.request, projections)?;

            for target in resolve(subject, indirect)? {
                resolved.push((target.project(&suffix), kind));
            }
        }

        Some(resolved)
    }

    pub(super) fn record_target_effect(&mut self, target: &Target, kind: DependencyMutationKind) {
        if !self.publish || self.effects.contains(&DependencyStorageEffect::Unknown) {
            return;
        }

        let effect = match &target.origin {
            Origin::Entry {
                reference,
                projections,
                layer: 1,
            } => {
                let Some(root) = self.formal_root(reference.root) else {
                    return;
                };

                let source = super::references::dependency_projections(&reference.projections);
                let suffix = super::references::dependency_projections(projections);

                match (source, suffix) {
                    (Some(source), Some(suffix)) => Some(DependencyStorageEffect::Indirect {
                        reference: DependencySubject::new(root, source),
                        projections: suffix.into(),
                        kind,
                    }),
                    _ => Some(DependencyStorageEffect::Unknown),
                }
            }
            Origin::Entry { .. } => Some(DependencyStorageEffect::Unknown),
            Origin::Storage(path) => self.formal_effect(path, kind),
            Origin::Allocation(_) => None,
        };

        if let Some(effect) = effect {
            if effect == DependencyStorageEffect::Unknown {
                self.effects.clear();
            }

            self.effects.insert(effect);
        }
    }

    pub(super) fn record_effects(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
    ) {
        if !self.publish || self.effects.contains(&DependencyStorageEffect::Unknown) {
            return;
        }

        let StorageInvalidation::Paths(paths) = invalidation else {
            self.effects.clear();
            self.effects.insert(DependencyStorageEffect::Unknown);
            return;
        };

        for path in paths {
            match self.formal_effect(path, kind) {
                Some(DependencyStorageEffect::Unknown) => {
                    self.effects.clear();
                    self.effects.insert(DependencyStorageEffect::Unknown);
                    return;
                }
                Some(effect) => {
                    self.effects.insert(effect);
                }
                None => {}
            }
        }
    }

    fn formal_effect(
        &self,
        path: &StoragePath,
        kind: DependencyMutationKind,
    ) -> Option<DependencyStorageEffect> {
        let identity = self
            .storage
            .identity(path.root)
            .expect("flow mutation retains its planned root");

        let borrowed = self.storage.identity_type(path.root).is_some_and(|ty| {
            matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                TypeData::Borrow { .. }
            )
        });

        if matches!(identity, StorageIdentity::Parameter(_))
            && path.projections.is_empty()
            && !borrowed
        {
            // A by-value parameter slot belongs to this invocation, not to its caller.
            return None;
        }

        let root = match identity {
            StorageIdentity::Parameter(parameter) => {
                DependencySubjectRoot::Parameter(SymbolOrdinal::new(
                    self.request
                        .symbols()
                        .callable_parameter(parameter)
                        .expect("planned parameter retains its declaration")
                        .ordinal(),
                ))
            }
            StorageIdentity::Receiver(_) => DependencySubjectRoot::Receiver,
            StorageIdentity::Static(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::Error(_)
            | StorageIdentity::CustomIndexBorrow(_)
            | StorageIdentity::Alternative { .. } => return Some(DependencyStorageEffect::Unknown),
            // Resolved referent writes use physical targets. Initializing or retiring a local
            // reference slot does not mutate the storage reached by its contents.
            _ => return None,
        };

        let projections = super::references::dependency_projections(&path.projections);

        let Some(projections) = projections else {
            return Some(DependencyStorageEffect::Unknown);
        };

        Some(if borrowed {
            DependencyStorageEffect::Indirect {
                reference: DependencySubject::root(root),
                projections: projections.into(),
                kind,
            }
        } else {
            DependencyStorageEffect::Direct {
                subject: DependencySubject::new(root, projections),
                kind,
            }
        })
    }
}

pub(super) fn project<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    mut path: StoragePath,
    projections: &[DependencyProjection],
) -> Option<StoragePath> {
    let suffix = storage_projections(request, projections)?;

    path.projections = path.projections.iter().copied().chain(suffix).collect();

    Some(path)
}
