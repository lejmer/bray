use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{StorageIdentity, StorageInvalidation, StoragePath, StorageProjection};
use bray_symbols::{
    DependencyMutationKind, DependencyStorageEffect, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal, TypeData,
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
    ) -> Option<Vec<Effect>> {
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

        if let Some(bray_bound_tree::SemanticSelection::Operation(operation)) = invocation
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        {
            // The graph also splits fallible built-ins and construction into attempt
            // and completion. Only selected calls or defaults can add opaque effects.
            // Operand transfers and destination writes have their own storage operations.
            let mut calls = Vec::new();
            let mut defaults = Vec::new();

            let resolved = crate::behavior::collect_operation_behavior(
                operation,
                None,
                &mut calls,
                &mut defaults,
            );

            return (resolved && calls.is_empty() && defaults.is_empty()).then(Vec::new);
        }

        self.prepare_call_references(state, invocation);

        let contracts = self.dependencies.calls().get(&invocation)?;

        if let Some(effects) = contracts.deferred_effects() {
            let effects = self.call_effects(state, invocation, effects);

            state.memory.defer_effects(invocation, Some(effects));
        }

        if let Some(bray_bound_tree::SemanticSelection::Call(call)) = invocation
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
            && crate::analysis::storage_invalidation::call_preserves_storage(call, None)
        {
            return Some(Vec::new());
        }

        Some(self.call_effects(state, invocation, contracts.effects()))
    }

    pub(super) fn call_attempt(
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

    pub(super) fn assignment_mutation(
        &self,
        operation: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> Option<DependencyMutationKind> {
        let plan = self
            .storage
            .node_plans(operation.node())
            .find(|plan| plan.purpose() == StorageAccessPurpose::Assignment)?;

        let ty = self
            .storage
            .access(plan.access())
            .expect("assignment retains its checked access")
            .reached_type();

        if let bray_bound_tree::AnyBoundNodeId::Expression(expression) = operation.node()
            && let Some(bray_bound_tree::BoundExpression::Assignment(assignment)) =
                self.request.view().expression(expression)
            && assignment.operator() == bray_bound_tree::BoundAssignmentOperator::Assign
            && let [_, source] = assignment.operands()
            && self.nullable_is_absent(execution, *source, ty)
        {
            return Some(DependencyMutationKind::Clear);
        }

        Some(DependencyMutationKind::Replace(ty))
    }

    pub(super) fn record_operation_effects(
        &mut self,
        kind: AnalysisOperationKind,
        call_effects: Option<&[Effect]>,
        assignment: Option<&Target>,
        mutation: Option<DependencyMutationKind>,
    ) {
        if !self.publish {
            return;
        }

        if let Some(target) = assignment {
            self.record_target_effect(target, mutation.expect("assignment retains its transition"));
        }

        if let Some(effects) = call_effects {
            self.record_resolved_effects(effects);
        } else if self.call_attempt(kind).is_some() {
            // Invalidation is not an ownership-effect summary. An unresolved call
            // may introduce ownership as well as retire storage, on any outcome.
            self.record_effect(DependencyStorageEffect::Unknown);
        } else if assignment.is_none()
            && let Some(effects) = self.refinements.invalidation_at(kind.occurrence())
        {
            let effect_kind = if self.storage.node_plans(kind.node()).any(|plan| {
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

    pub(super) fn operation_observations(
        &self,
        state: &StorageFlowState,
        kind: AnalysisOperationKind,
        assignment: Option<&Target>,
        effects: Option<&[Effect]>,
        roots: Option<&[Target]>,
    ) -> Option<StorageInvalidation> {
        let mut observed = effects
            .map(|effects| {
                self.call_observations(state, kind.occurrence(), effects, roots.unwrap_or(&[]))
            })
            .or_else(|| {
                roots.map(|roots| self.opaque_observations(&state.memory, roots.iter().cloned()))
            })
            .or_else(|| {
                assignment
                    .is_none()
                    .then(|| {
                        self.refinements
                            .invalidation_at(kind.occurrence())
                            .map(|planned| {
                                self.planned_observations(state, planned, kind.occurrence())
                            })
                    })
                    .flatten()
            });

        if let Some(target) = assignment {
            let destination = self.target_observations(&state.memory, std::iter::once(target));

            if let Some(observed) = &mut observed {
                observed.merge(&destination);
            } else {
                observed = Some(destination);
            }
        }

        observed
    }

    pub(super) fn apply_call_effects(
        &self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        call_effects: Option<&[Effect]>,
        invalidation: Option<&StorageInvalidation>,
    ) {
        if self.call_attempt(kind).is_some()
            && kind
                .occurrence()
                .expression()
                .is_some_and(|expression| self.memory.operation(expression).is_none())
        {
            if let Some(invalidation) =
                invalidation.or_else(|| self.refinements.invalidation_at(kind.occurrence()))
            {
                self.apply_resolved_effects(&mut state.memory, call_effects, invalidation);
            }
        }
    }

    pub(super) fn apply_resolved_effects(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        effects: Option<&[Effect]>,
        invalidation: &StorageInvalidation,
    ) {
        let Some(effects) = effects.filter(|effects| {
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Unknown | Effect::Cleanup(_)))
        }) else {
            memory.invalidate(invalidation, self.storage, DependencyMutationKind::Retire);
            return;
        };

        for effect in effects {
            match effect {
                Effect::Escape | Effect::Mutation(_, DependencyMutationKind::Escape) => {}
                Effect::Mutation(target, kind) => memory.invalidate_target(target, *kind),
                Effect::Atomic => memory.invalidate(
                    &StorageInvalidation::unknown(),
                    self.storage,
                    DependencyMutationKind::Atomic,
                ),
                Effect::Unknown | Effect::Cleanup(_) => {
                    unreachable!("opaque effects handled above")
                }
            }
        }
    }

    fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[DependencyStorageEffect],
    ) -> Vec<Effect> {
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
    ) -> Vec<Effect> {
        let mut resolved = Vec::new();

        for effect in effects {
            let (subject, indirect, projections, kind) = match effect {
                DependencyStorageEffect::Escape => {
                    resolved.push(Effect::Escape);
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
                DependencyStorageEffect::Cleanup(ty) => {
                    resolved.push(Effect::Cleanup(*ty));
                    continue;
                }
                DependencyStorageEffect::Direct { subject, kind } => {
                    (subject, false, &[][..], *kind)
                }
                DependencyStorageEffect::Indirect {
                    reference,
                    projections,
                    kind,
                } => (reference, true, projections.as_ref(), *kind),
            };

            let Some(suffix) = storage_projections(self.dependencies, projections) else {
                resolved.push(if kind == DependencyMutationKind::Escape {
                    Effect::Escape
                } else {
                    Effect::Unknown
                });

                continue;
            };

            let Some(targets) = resolve(subject, indirect) else {
                if kind == DependencyMutationKind::Atomic {
                    resolved.push(Effect::Atomic);
                    continue;
                }

                resolved.push(if kind == DependencyMutationKind::Escape {
                    Effect::Escape
                } else {
                    Effect::Unknown
                });

                continue;
            };

            for target in targets {
                resolved.push(Effect::Mutation(target.project(&suffix), kind));
            }
        }

        resolved.sort_unstable();
        resolved.dedup();

        resolved
    }

    pub(super) fn record_resolved_effects(&mut self, effects: &[Effect]) {
        if !self.publish {
            return;
        }

        for effect in effects {
            match effect {
                Effect::Escape | Effect::Mutation(_, DependencyMutationKind::Escape) => {}
                Effect::Mutation(target, kind) => self.record_target_effect(target, *kind),
                Effect::Unknown => self.record_effect(DependencyStorageEffect::Unknown),
                Effect::Cleanup(ty) => self.record_effect(DependencyStorageEffect::Cleanup(*ty)),
                Effect::Atomic => self.record_effect(DependencyStorageEffect::Atomic),
            }
        }
    }

    pub(super) fn record_target_effect(&mut self, target: &Target, kind: DependencyMutationKind) {
        if !self.publish {
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
            Origin::Entry { .. } | Origin::Returned { .. } => {
                Some(DependencyStorageEffect::Unknown)
            }
            Origin::Storage(path) => self.formal_effect(path, kind),
            Origin::Allocation(_) => None,
        };

        if let Some(effect) = effect {
            let effect = if effect == DependencyStorageEffect::Unknown {
                DependencyStorageEffect::unresolved(kind)
            } else {
                effect
            };

            self.record_effect(effect);
        }
    }

    pub(super) fn record_effects(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
    ) {
        if !self.publish {
            return;
        }

        let StorageInvalidation::Paths(paths) = invalidation else {
            self.record_effect(DependencyStorageEffect::Unknown);
            return;
        };

        for path in paths {
            match self.formal_effect(path, kind) {
                Some(DependencyStorageEffect::Unknown) => {
                    self.record_effect(DependencyStorageEffect::Unknown);
                    return;
                }
                Some(effect) => {
                    self.record_effect(effect);
                }
                None => {}
            }
        }
    }

    fn record_effect(&mut self, effect: DependencyStorageEffect) {
        if !matches!(
            effect,
            DependencyStorageEffect::Direct {
                kind: DependencyMutationKind::Consume,
                ..
            }
        ) {
            self.has_normal_effects |= self.on_normal_path;
        }

        self.effects.insert(effect);
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
            && identity.is_invocation_owned(self.storage.kind())
            && !borrowed
            && !path.projections.contains(&StorageProjection::OwnedTarget)
        {
            // Inline parameter storage belongs to this invocation, including projected
            // fields and exposed addresses. References and owned targets remain shared.
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
