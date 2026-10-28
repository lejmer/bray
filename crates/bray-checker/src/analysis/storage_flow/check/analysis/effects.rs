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
        &mut self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
        assignment: Option<&Target>,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> Option<Vec<Effect>> {
        self.prepare_assignment_reference(state, operation, assignment);

        if let bray_bound_tree::BoundExecutionSite::Node(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = operation.occurrence()
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
            let source = self.execution.awaited_invocation(execution, expression);
            let effects = source.and_then(|source| state.memory.deferred_effects(source));

            return effects;
        }

        if !self.dependencies.calls().contains_key(&invocation)
            && invocation.expression().is_some_and(|expression| {
                matches!(
                    self.execution.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Operation(_))
                )
            })
        {
            // Source-free operations have no invocation contract. Their operand
            // transfers and destination writes have separate storage operations.
            return Some(Vec::new());
        }

        self.prepare_call_references(state, invocation);

        let contracts = self.dependencies.calls().get(&invocation)?;

        if self.publish {
            self.synchronous_cleanup
                .extend(contracts.synchronous_cleanup().iter().cloned());
        }

        let mut task_effects = task_effects(
            &state.memory,
            invocation
                .expression()
                .and_then(|expression| self.execution.task_effect_source(execution, expression)),
        );

        if let Some(effects) = contracts.deferred_effects() {
            let effects = task_effects
                .take()
                .unwrap_or_else(|| self.call_effects(state, invocation, effects));

            state.memory.defer_effects(invocation, Some(effects));
        }

        if let bray_bound_tree::BoundExecutionSite::Node(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = invocation
            && let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                self.execution.semantics.selections().expression(expression)
            && crate::analysis::storage_invalidation::call_preserves_storage(call, None)
        {
            return Some(Vec::new());
        }

        let effects = if contracts.deferred_effects().is_none() {
            task_effects
                .unwrap_or_else(|| self.call_effects(state, invocation, contracts.effects()))
        } else {
            self.call_effects(state, invocation, contracts.effects())
        };

        Some(effects)
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
            ))
            | AnalysisOperationKind::TaskOperation { expression, .. } => {
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
        memory: &crate::analysis::storage_flow::memory::State,
        kind: AnalysisOperationKind,
        call_effects: Option<&[Effect]>,
        assignment: Option<&Target>,
        mutation: Option<DependencyMutationKind>,
        normal_effects: bool,
    ) {
        if !self.publish {
            return;
        }

        if let Some(target) = assignment {
            self.record_target_effect(
                memory,
                target,
                mutation.expect("assignment retains its transition"),
                self.on_normal_path,
            );
        }

        if let Some(effects) = call_effects {
            self.record_resolved_effects(memory, effects, normal_effects);
        } else if self.call_attempt(kind).is_some() {
            // Invalidation is not an ownership-effect summary. An unresolved call
            // may introduce ownership as well as retire storage, on any outcome.
            self.record_effect(DependencyStorageEffect::Unknown, normal_effects);
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
            && kind.occurrence().expression().is_some_and(|expression| {
                !matches!(
                    kind.occurrence(),
                    bray_bound_tree::BoundExecutionSite::Node(_)
                ) || self.memory.operation(expression).is_none()
            })
            && let Some(invalidation) =
                invalidation.or_else(|| self.refinements.invalidation_at(kind.occurrence()))
        {
            self.apply_resolved_effects(&mut state.memory, call_effects, invalidation);
        }
    }

    pub(super) fn capture_failure_effects(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: Option<Vec<Effect>>,
        escaped: std::collections::BTreeSet<Target>,
        invalidation: Option<&StorageInvalidation>,
    ) {
        if effects.as_ref().is_some_and(Vec::is_empty) && escaped.is_empty() {
            return;
        }

        state.memory.capture_failure_effects(
            occurrence,
            crate::analysis::storage_flow::memory::CapturedEffects {
                mutations: effects.unwrap_or_else(|| vec![Effect::Unknown]),
                escaped: escaped.into_iter().collect(),
                // The outcome owns the resolved descriptors after argument transfer.
                // Invalidation shares immutable projection paths with publication.
                invalidation: invalidation
                    .expect("resolved call effects have storage observations")
                    .clone(),
            },
        );
    }

    pub(in crate::analysis::storage_flow) fn apply_failure_effects(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {
        let Some(effects) = state.memory.take_failure_effects(occurrence) else {
            return;
        };

        // The captured regions were normalized before ownership transfer.
        state.memory.escape_regions(effects.escaped.iter().cloned());

        self.apply_resolved_effects(
            &mut state.memory,
            Some(&effects.mutations),
            &effects.invalidation,
        );

        self.execution.invalidate_call_effects(
            state
                .execution
                .as_mut()
                .expect("reachable call failure has execution facts"),
            occurrence,
            Some(&effects.invalidation),
        );
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
                Effect::Ambient => {
                    if effects.contains(&Effect::Escape) {
                        // Opaque calls can reach every invocation input, including delayed escapes.
                        memory.invalidate(
                            invalidation,
                            self.storage,
                            DependencyMutationKind::Retire,
                        );
                    } else {
                        let escaped = effects.iter().filter_map(|effect| match effect {
                            Effect::Mutation(target, DependencyMutationKind::Escape) => {
                                Some(target.clone())
                            }
                            _ => None,
                        });

                        let targets = memory.reachable_targets(memory.escaped_regions(escaped));

                        for target in targets {
                            memory.invalidate_target(&target, DependencyMutationKind::Retire);
                        }
                    }
                }
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

    pub(super) fn record_resolved_effects(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[Effect],
        normal_effects: bool,
    ) {
        if !self.publish {
            return;
        }

        for effect in effects {
            match effect {
                Effect::Escape | Effect::Mutation(_, DependencyMutationKind::Escape) => {}
                Effect::Mutation(target, kind) => {
                    self.record_target_effect(memory, target, *kind, normal_effects)
                }
                Effect::Ambient => {
                    self.record_effect(DependencyStorageEffect::Ambient, normal_effects)
                }
                Effect::Unknown => {
                    self.record_effect(DependencyStorageEffect::Unknown, normal_effects)
                }
                Effect::Cleanup(ty) => {
                    self.record_effect(DependencyStorageEffect::Cleanup(*ty), normal_effects)
                }
                Effect::Atomic => {
                    self.record_effect(DependencyStorageEffect::Atomic, normal_effects)
                }
            }
        }
    }

    pub(super) fn record_target_effect(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        target: &Target,
        kind: DependencyMutationKind,
        normal_effects: bool,
    ) {
        if !self.publish {
            return;
        }

        if matches!(target.origin, Origin::Returned { .. })
            && kind != DependencyMutationKind::Atomic
        {
            if kind != DependencyMutationKind::Escape {
                // An independent alternative may reach external storage, without granting freshness.
                self.record_effect(DependencyStorageEffect::Ambient, normal_effects);
            }

            // Preserve every possible input region and the actual ownership transition.
            for origin in memory.effect_origins(target) {
                self.record_target_effect(memory, origin, kind, normal_effects);
            }

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

                let source = crate::storage::dependency_projections(&reference.projections);

                // An unportable selector widens the reached region, never the identity
                // of the reference through which this operation obtained access.
                let suffix = projections
                    .iter()
                    .copied()
                    .map_while(crate::storage::dependency_projection)
                    .collect::<Vec<_>>();

                match source {
                    Some(source) => Some(DependencyStorageEffect::Indirect {
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

            self.record_effect(effect, normal_effects);
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
            self.record_effect(DependencyStorageEffect::Unknown, self.on_normal_path);
            return;
        };

        for path in paths {
            match self.formal_effect(path, kind) {
                Some(DependencyStorageEffect::Unknown) => {
                    self.record_effect(DependencyStorageEffect::Unknown, self.on_normal_path);
                    return;
                }
                Some(effect) => {
                    self.record_effect(effect, self.on_normal_path);
                }
                None => {}
            }
        }
    }

    fn record_effect(&mut self, effect: DependencyStorageEffect, normal_effects: bool) {
        if !matches!(
            effect,
            DependencyStorageEffect::Direct {
                kind: DependencyMutationKind::Consume,
                ..
            }
        ) {
            self.has_normal_effects |= normal_effects;
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
            && (kind != DependencyMutationKind::Escape
                || self.input.has_complete_references(self.storage, path))
            && !path.projections.contains(&StorageProjection::OwnedTarget)
        {
            // Writes to inline parameter storage stay in this invocation.
            // Escape summaries separately carry its reachable value contents.
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
            | StorageIdentity::ContractParameter(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::Error(_)
            | StorageIdentity::CustomIndexBorrow(_)
            | StorageIdentity::Alternative { .. } => return Some(DependencyStorageEffect::Unknown),
            // Resolved referent writes use physical targets. Initializing or retiring a local
            // reference slot does not mutate the storage reached by its contents.
            _ => return None,
        };

        let projections = crate::storage::dependency_projections(&path.projections);

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

pub(super) fn task_effects(
    memory: &crate::analysis::storage_flow::memory::State,
    source: Option<bray_bound_tree::BoundExecutionSite>,
) -> Option<Vec<Effect>> {
    let mut effects = memory.deferred_effects(source?)?;

    // Scheduling and resolution can affect external runtime state even when
    // the captured computation has no user storage mutations.
    if !effects.contains(&Effect::Ambient) {
        effects.push(Effect::Ambient);
    }

    Some(effects)
}
