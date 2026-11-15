use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{StorageIdentity, StorageInvalidation, StoragePath, StorageProjection};
use bray_symbols::{
    DependencyMutationKind, DependencyStorageEffect, DependencySubject, DependencySubjectRoot,
    TypeData,
};

use super::core::StorageFlowCollector;
use std::collections::BTreeSet;
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

        if self.call_attempt(operation).is_some() {
            self.observe_call_reference_inputs(state, operation.occurrence());
        }

        if let bray_bound_tree::BoundExecutionSite::Node(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = operation.occurrence()
            && self.memory.operation(expression).is_some()
        {
            if matches!(operation, AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input | AnalysisCallPhase::Completion, ..
            }) {
                return Some(Vec::new());
            }

            return self.memory_effects(state, expression);
        }

        let invocation = self.call_attempt(operation)?;

        let awaited = invocation.expression().and_then(|expression| {
            match self.request.view().expression(expression) {
                Some(bray_bound_tree::BoundExpression::Await(awaited)) => Some(*awaited),
                _ => None,
            }
        });

        if let Some(awaited) = awaited {
            return self.begin_await_transition(state, execution, operation, awaited);
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

        let task_sources = invocation
            .expression()
            .and_then(|expression| self.execution.task_invocations(execution, expression));

        if invocation.expression().is_some_and(|expression| matches!(
            self.execution.semantics.selections().expression(expression),
            Some(bray_bound_tree::SemanticSelection::Call(call))
                if call.implementation_hook().and_then(crate::analysis::AnalysisTaskOperationKind::from_implementation_hook).is_some()
        ))
            && let Some(crate::analysis::storage_flow::memory::Source::Value(source)) = self.invocation_reference(state, invocation, DependencySubjectRoot::Receiver)
            && let Some(result) = self.invocation_result(invocation)
        {
            state.memory.stage_deferred_values(invocation, [(source, Target::storage(result))]);
        }

        let mut task_effects = task_sources.as_ref().and_then(|sources| {
            let source =
                self.invocation_reference(state, invocation, DependencySubjectRoot::Receiver)?;

            let body = self
                .execution
                .task_body(execution, invocation.expression()?)?;

            task_effects(
                state.memory.deferred_effects(&source, &[body].into()),
                self.execution.task_cleanup_types(sources, false)?,
            )
        });

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

    fn begin_await_transition(
        &mut self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: AnalysisOperationKind,
        awaited: bray_bound_tree::BoundAwaitExpression,
    ) -> Option<Vec<Effect>> {
        let invocation = operation.occurrence();

        let expression = invocation
            .expression()
            .expect("await transition retains its expression");

        let source = self.expression_reference(state, awaited.operand())?;

        let task_producers = self
            .execution
            .awaited_task_invocations(execution, expression);

        let task = task_producers.is_some();

        let producers = task_producers.unwrap_or_else(|| {
            self.execution
                .awaited_invocations(execution, expression)
                .map(|(source, _)| source)
                .collect::<std::collections::BTreeSet<_>>()
        });

        if self.execution.awaited_unknown(execution, expression) {
            state.memory.begin_transition(invocation);

            return None;
        }

        let body = task
            .then(|| {
                self.execution.deferred_body(
                    execution,
                    self.execution.value(execution, awaited.operand()),
                )
            })
            .flatten();

        let body_sources = body
            .map(|(source, _)| [source].into())
            .unwrap_or_else(|| producers.clone());

        let mut body_effects = if task && body.is_none() {
            None
        } else {
            state.memory.deferred_effects(&source, &body_sources)
        };

        if task
            && let Some(effects) = &mut body_effects
            && !effects.contains(&Effect::Ambient)
        {
            effects.push(Effect::Ambient);
        }

        let effects = if task {
            // Normal and abnormal snapshots own descriptors with shared projection paths.
            task_effects(
                body_effects.clone(),
                self.execution.task_cleanup_types(&producers, false)?,
            )
        } else {
            body_effects.take()
        };

        let mut result = self
            .expression_result_storage(expression)
            .expect("await retains its checked result storage");

        if task {
            let (variant, field) = crate::representation::union_payload(
                self.request,
                "RunResultVariant0Completed",
                "RunResultVariant0CompletedValue",
            );

            result.projections = result
                .projections
                .iter()
                .copied()
                .chain(std::iter::repeat_n(
                    StorageProjection::ActiveUnionPayloadField { variant, field },
                    body.map_or(1, |(_, depth)| depth),
                ))
                .collect();
        }

        state.memory.prepare_deferred_completion(
            invocation,
            &source,
            result,
            body_sources.iter().map(|producer| {
                (
                    *producer,
                    self.invocation_result(*producer)
                        .expect("formed producer retains its result storage"),
                )
            }),
        );

        if task {
            // Capture cancellation before consuming the operand. Its cleanup
            // runs on the abnormal result path, after that outcome is selected.
            let roots = self.invocation_roots(state, invocation);
            let escaped = self.address_escapes(state, operation, effects.as_deref(), &roots, false);

            let invalidation = self.operation_observations(
                state,
                operation,
                None,
                effects.as_deref(),
                Some(&roots),
                None,
            );

            self.capture_failure_effects(
                state,
                invocation,
                effects,
                escaped,
                invalidation.as_ref(),
            );

            return body_effects;
        }

        return effects;
    }

    pub(super) fn call_attempt(
        &self,
        operation: AnalysisOperationKind,
    ) -> Option<bray_bound_tree::BoundExecutionSite> {
        match operation {
            AnalysisOperationKind::Suspension {
                occurrence,
                kind:
                    crate::analysis::model::AnalysisSuspensionKind::Await
                    | crate::analysis::model::AnalysisSuspensionKind::Yield,
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
        marker_exposure: Option<&BTreeSet<Target>>,
    ) -> Option<StorageInvalidation> {
        let mut observed = effects
            .map(|effects| {
                self.call_observations(
                    state,
                    kind.occurrence(),
                    effects,
                    roots.unwrap_or(&[]),
                    marker_exposure,
                )
            })
            .or_else(|| {
                roots.map(|roots| {
                    self.opaque_observations(&state.memory, roots.iter().cloned(), marker_exposure)
                })
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
        &mut self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        normal_effects: bool,
    ) {
        let Some(effects) = state.memory.take_failure_effects(occurrence) else {
            return;
        };

        // The captured regions were normalized before ownership transfer.
        state.memory.escape_regions(effects.escaped.iter().cloned());

        self.record_resolved_effects(&state.memory, &effects.mutations, normal_effects);

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
                Effect::Mutation(target, kind) => {
                    memory.invalidate_target(target, *kind, invalidation)
                }
                effect if effect.is_ambient() => {
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
                            memory.invalidate_target(
                                &target,
                                DependencyMutationKind::Retire,
                                invalidation,
                            );
                        }
                    }
                }
                effect if effect.is_atomic() => memory.invalidate(
                    &StorageInvalidation::unknown(),
                    self.storage,
                    DependencyMutationKind::Atomic,
                ),
                Effect::Unknown | Effect::Cleanup(_) => {
                    unreachable!("opaque effects handled above")
                }
                Effect::Ambient | Effect::Atomic => {
                    unreachable!("classified effects handled above")
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
            if indirect
                && subject.projections().is_empty()
                && !matches!(
                    subject.subject_root(),
                    DependencySubjectRoot::ProductStatic(_)
                        | DependencySubjectRoot::ExactThreadStatic(_)
                )
            {
                self.invocation_reference(state, occurrence, subject.subject_root())?
                    .regions(&state.memory)
            } else {
                let locations = self.invocation_locations(state, occurrence, subject)?;

                if indirect {
                    locations
                        .iter()
                        .try_fold(Vec::new(), |mut targets, location| {
                            let regions = state.memory.referenced_regions(location);

                            targets.extend(regions?);

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

            let static_subject = matches!(
                subject.subject_root(),
                DependencySubjectRoot::ProductStatic(_)
                    | DependencySubjectRoot::ExactThreadStatic(_)
            );

            let suffix = storage_projections(
                self.request.semantic_values(),
                self.dependencies,
                projections,
            );

            let suffix = if let Some(suffix) = suffix {
                suffix
            } else if static_subject {
                Vec::new()
            } else {
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
            Origin::Static { root, projections } => {
                // An unresolved selector widens within the same static identity.
                let projections = projections.iter().copied()
                    .map_while(crate::storage::dependency_projection);

                Some(DependencyStorageEffect::Direct {
                    subject: DependencySubject::new(*root, projections),
                    kind,
                })
            }
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

        if matches!(
            identity,
            StorageIdentity::Parameter(_)
                | StorageIdentity::Receiver(_)
                | StorageIdentity::AnonymousParameter(_)
                | StorageIdentity::ContractParameter(_)
        ) && identity.is_invocation_owned(self.storage.kind())
            && !borrowed
            && (kind != DependencyMutationKind::Escape
                || self.input.reference_free(self.storage, path).is_some())
            && !path.projections.contains(&StorageProjection::OwnedTarget)
        {
            // Writes to inline owned input storage stay in this invocation.
            // Escape summaries separately carry its reachable value contents.
            return None;
        }

        let root = match identity {
            StorageIdentity::Parameter(_)
            | StorageIdentity::Receiver(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::ContractParameter(_) => self
                .formal_root(path.root)
                .expect("checked callable input retains its formal dependency subject"),
            StorageIdentity::Static(declaration) => *self
                .request
                .static_dependencies()
                .get(&declaration)
                .expect("planned static storage retains its prepared dependency root"),
            StorageIdentity::Error(_)
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
    effects: Option<Vec<Effect>>,
    cleanup: impl IntoIterator<Item = bray_symbols::TypeId>,
) -> Option<Vec<Effect>> {
    let mut effects = effects?;

    for ty in cleanup {
        let effect = Effect::Cleanup(ty);

        if !effects.contains(&effect) {
            effects.push(effect);
        }
    }

    // Scheduling and resolution can affect external runtime state even when
    // the captured computation has no user storage mutations.
    if !effects.contains(&Effect::Ambient) {
        effects.push(Effect::Ambient);
    }

    Some(effects)
}
