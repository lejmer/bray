use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes};
use bray_bound_tree::{StorageIdentity, StorageInvalidation, StoragePath, StorageProjection};
use bray_symbols::{
    DependencyMutationKind, DependencyStorageEffect, DependencySubject,
    TypeData,
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
        assignment: Option<&[Target]>,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> Option<EffectOutcomes> {
        self.prepare_assignment_reference(state, operation, assignment);

        if let Some(invocation) = self.call_attempt(operation)
            && matches!(invocation, bray_bound_tree::BoundExecutionSite::CallInput { .. })
        {
            self.observe_call_reference_inputs(state, invocation);
        }

        if let bray_bound_tree::BoundExecutionSite::Node(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = operation.occurrence()
            && self.memory.operation(expression).is_some()
        {
            if matches!(
                operation,
                AnalysisOperationKind::Call {
                    phase: AnalysisCallPhase::Input | AnalysisCallPhase::Completion,
                    ..
                }
            ) {
                return Some(EffectOutcomes::new([]));
            }

            return self.memory_effects(state, expression).map(EffectOutcomes::new);
        }

        let invocation = self.call_attempt(operation)?;

        if operation.awaited_expression().is_some() {
            return self.begin_await_transition(state, execution, operation);
        }

        if !self.dependencies.calls().contains_key(&invocation)
            && invocation.expression().is_some_and(|expression| {
                matches!(
                    self.execution.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Operation(_))
                ) || self.request.view().expression(expression).is_some_and(|bound| {
                    !matches!(bound, bray_bound_tree::BoundExpression::Call(_) | bray_bound_tree::BoundExpression::Await(_))
                })
            })
        {
            // Intrinsic operations and ownership admission checks have invocation
            // edges without calling source code. Their checked moves and writes
            // remain separate storage transitions.
            return Some(EffectOutcomes::new([]));
        }

        self.prepare_call_references(state, invocation);

        let contracts = self.dependencies.calls().get(&invocation)?;

        if self.publish {
            self.synchronous_cleanup
                .extend(contracts.synchronous_cleanup().iter().cloned());
        }

        let task_effects = invocation.expression().filter(|expression| invocation == (*expression).into()).and_then(|expression| {
            let value = self.execution.task_receiver(execution, expression)?;
            let proof = execution.ordinary_proof();
            let frame = value.deferred_value()?;
            let cleanup = frame.cleanup_effects(false, &proof)?;

            let (_, _, body) = self.execution.deferred_body(execution, value)?;

            task_effects(body.effects(&proof), cleanup)
        });

        if contracts.deferred_effect_outcomes().is_some() {
            self.capture_deferred_cleanup(state, execution, invocation);
        }

        let effects = if contracts.deferred_effects().is_none() {
            task_effects
                .unwrap_or_else(|| self.call_effect_outcomes(state, execution, invocation, contracts.effect_outcomes()))
        } else {
            self.call_effect_outcomes(state, execution, invocation, contracts.effect_outcomes())
        };

        Some(effects)
    }

    fn begin_await_transition(
        &mut self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: AnalysisOperationKind,
    ) -> Option<EffectOutcomes> {
        let invocation = operation.occurrence();

        let expression = invocation
            .expression()
            .expect("await transition retains its expression");

        let value = execution.admitted_receiver(invocation);
        let proof = execution.ordinary_proof();

        let Some(frame) = value.deferred_value().filter(|frame| !frame.unknown_possible(&proof)) else {
            state.memory.begin_transition(invocation);

            return None;
        };

        let task_frame = self.execution.observed_task_frame(execution, &value);
        let task = task_frame.is_some();

        let (body, depth) = if task {
            let (_, depth, body) = self.execution.deferred_body(execution, value.clone())?;

            (body, depth)
        } else {
            (std::sync::Arc::clone(frame), 0)
        };

        let mut body_effects = body.effects(&proof);

        if task
            && let Some(effects) = &mut body_effects
            && !effects.possible().contains(&Effect::Ambient)
        {
            *effects = task_effects(Some(effects.clone()), []).expect("known task effects remain known");
        }

        let effects = if task {
            // Normal and abnormal snapshots own descriptors with shared projection paths.
            task_effects(
                body_effects.clone(),
                task_frame.as_ref()?.cleanup_effects(false, &proof)?,
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
                    depth,
                ))
                .collect();
        }

        if let Some(transitions) = body.transitions(&proof) {
            state.memory.prepare_deferred_completion(invocation, result, transitions);
        } else {
            // Imported bodies publish their private outputs from their bound value expressions.
            state.memory.begin_transition(invocation);
        }

        if task {
            return Some(effects?.with_normal(body_effects?.normal().iter().cloned()));
        }

        effects
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
        assignment: Option<&[Effect]>,
        normal_effects: bool,
    ) {
        if !self.publish {
            return;
        }

        if let Some(effects) = assignment {
            self.record_resolved_effects(memory, effects, self.on_normal_path);
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
        assignment: Option<&[Target]>,
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

        if let Some(targets) = assignment {
            let destination = self.target_observations(&state.memory, targets.iter());

            if let Some(observed) = &mut observed {
                observed.merge(&destination);
            } else {
                observed = Some(destination);
            }
        }

        observed.map(|observed| observed.with_observation_transition(self.observation_transition(kind)))
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
            state.memory.apply_effects(call_effects, invalidation, self.storage);
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

        state.memory.apply_effects(
            Some(&effects.mutations),
            &effects.invalidation,
            self.storage,
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

    pub(super) fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[DependencyStorageEffect],
    ) -> Vec<Effect> {
        self.resolve_effects(effects, |slot| crate::analysis::storage_flow::memory::Source::Value(slot).mutation_targets(&state.memory), |subject, indirect| {
            if indirect
                && subject.projections().is_empty()
                && !subject.subject_root().is_static()
            {
                let ordinal = occurrence.expression().and_then(|expression| {
                    self.execution.semantics.selections().expression(expression)?
                        .invocation_input_ordinal(occurrence, subject.subject_root())
                });

                let entry = execution.admitted_call(occurrence);

                if let Some(captured) = entry.zip(ordinal).and_then(|(entry, ordinal)| entry.argument_storage(ordinal)) {
                    captured.mutation_targets()
                } else {
                    self.invocation_reference(state, occurrence, subject.subject_root())?
                        .mutation_targets(&state.memory)
                }
            } else {
                let locations = self.invocation_locations(state, occurrence, subject)?;

                if indirect {
                    locations
                        .into_iter()
                        .try_fold(Vec::new(), |mut targets, location| {
                            targets.extend(crate::analysis::storage_flow::memory::Source::Value(location).mutation_targets(&state.memory)?);

                            Some(targets)
                        })
                } else {
                    Some(locations.into_iter().map(crate::analysis::storage_flow::memory::MutationTarget::location).collect())
                }
            }
        })
    }

    fn call_effect_outcomes(
        &self,
        state: &StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &bray_symbols::DependencyEffectOutcomes<DependencyStorageEffect>,
    ) -> EffectOutcomes {
        effects.try_flat_map(|effect| {
            Ok::<_, std::convert::Infallible>(self.call_effects(state, execution, occurrence, std::slice::from_ref(effect)))
        }).expect("every formal effect resolves to a captured transition or an explicit conservative bound")
    }

    pub(super) fn resolve_effects(
        &self,
        effects: &[DependencyStorageEffect],
        mut read: impl FnMut(Target) -> Option<Vec<bray_bound_tree::StorageMutationTarget>>,
        mut resolve: impl FnMut(&DependencySubject, bool) -> Option<Vec<crate::analysis::storage_flow::memory::MutationTarget>>,
    ) -> Vec<Effect> {
        let mut resolved = Vec::new();

        for effect in effects {
            let (subject, indirect, projections, reads, kind, access_type, addressed) = match effect {
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
                DependencyStorageEffect::Direct { addressed, subject, kind, access_type } => {
                    (subject, false, &[][..], &[][..], *kind, *access_type, *addressed)
                }
                DependencyStorageEffect::Indirect { addressed, reads,
                    reference,
                    projections,
                    kind,
                    access_type,
                } => (reference, true, projections.as_ref(), reads.as_ref(), *kind, *access_type, *addressed),
            };

            let static_subject = subject.subject_root().is_static();

            let suffix = storage_projections(
                self.request.semantic_values(),
                self.dependencies,
                projections,
            );

            let addressed = addressed && suffix.is_some();

            let suffix = if let Some(suffix) = suffix {
                suffix
            } else if static_subject && !indirect && reads.is_empty() {
                Vec::new()
            } else {
                resolved.push(Effect::unbounded(kind, access_type, false));

                continue;
            };

            let targets = if static_subject {
                self.static_effect_targets(subject).and_then(|targets| {
                    if indirect {
                        targets.into_iter().try_fold(Vec::new(), |mut reached, target| {
                            reached.extend(read(target.address?)?);

                            Some(reached)
                        })
                    } else { Some(targets) }
                })
            } else {
                resolve(subject, indirect)
            }.and_then(|targets|
                bray_bound_tree::StorageMutationTarget::resolve_route(targets, &suffix, reads, &mut read));

            let Some(targets) = targets else {
                if kind == DependencyMutationKind::Atomic {
                    resolved.push(Effect::Atomic);
                    continue;
                }

                resolved.push(Effect::unbounded(kind, access_type, false));

                continue;
            };

            for target in targets {
                let target = if addressed { target } else { bray_bound_tree::StorageMutationTarget::bounded(target.footprint) };

                resolved.push(Effect::through(target, kind, access_type));
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
                Effect::Escape | Effect::Mutation { kind: DependencyMutationKind::Escape, .. } => {}
                Effect::Mutation { address, footprint, kind, access_type } => {
                    self.record_target_effect(memory, footprint, address.as_ref() == Some(footprint), *kind, *access_type, normal_effects);

                    if let Some(operand) = address && operand != footprint {
                        self.record_target_effect(memory, operand, true, *kind, *access_type, normal_effects);
                    }
                }
                Effect::UnboundedMutation { kind, access_type, ambient } => {
                    self.record_effect(DependencyStorageEffect::UnboundedMutation {
                        kind: *kind, access_type: *access_type, ambient: *ambient,
                    }, normal_effects);
                }
                Effect::Ambient => {
                    self.record_effect(DependencyStorageEffect::Ambient, normal_effects)
                }
                Effect::Unknown => {
                    self.record_effect(DependencyStorageEffect::Unknown, normal_effects)
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
        addressed: bool,
        kind: DependencyMutationKind,
        access_type: Option<bray_symbols::TypeId>,
        normal_effects: bool,
    ) {
        if !self.publish {
            return;
        }

        if matches!(target.origin, Origin::Returned { .. })
            && kind != DependencyMutationKind::Atomic
        {
            // Alias bounds retain the possible region, not the disposed value's layout.
            let kind = kind.region_kind();

            let (origins, unbounded) = memory.effect_origins(target);

            if unbounded && kind != DependencyMutationKind::Escape {
                // An independent alternative may reach external storage, without granting freshness.
                self.record_effect(DependencyStorageEffect::UnboundedMutation {
                    kind, access_type, ambient: true,
                }, normal_effects);
            }

            // Preserve every possible input region and the actual ownership transition.
            for origin in origins {
                self.record_target_effect(memory, origin, false, kind, access_type, normal_effects);
            }

            return;
        }

        let effect = match &target.origin {
            Origin::Entry {
                reference,
                projections,
                layer,
                ..
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
                    Some(source) => Some(DependencyStorageEffect::Indirect { addressed: addressed && suffix.len() == projections.len(),
                        reads: vec![0; layer.checked_sub(1).expect("an entry referent has a positive layer")].into(),
                        reference: DependencySubject::new(root, source),
                        projections: suffix.into(),
                        kind,
                        access_type: None,
                    }),
                    _ => Some(DependencyStorageEffect::Unknown),
                }
            }
            Origin::Returned { .. } => {
                Some(DependencyStorageEffect::Unknown)
            }
            Origin::Storage(path) => self.formal_effect(path, kind),
            Origin::Allocation { .. } => None,
            Origin::Static { root, projections, .. } => {
                // An unresolved selector widens within the same static identity.
                let suffix = projections.iter().copied().map_while(crate::storage::dependency_projection).collect::<Vec<_>>();

                Some(DependencyStorageEffect::Direct { addressed: addressed && suffix.len() == projections.len(),
                    subject: DependencySubject::new(*root, suffix),
                    kind,
                    access_type: None,
                })
            }
        };

        let effect = effect.or_else(|| matches!(kind, DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_)).then_some(
            DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient: true }
        ));

        if let Some(mut effect) = effect {
            if let DependencyStorageEffect::Direct { access_type: original, .. }
                | DependencyStorageEffect::Indirect { access_type: original, .. } = &mut effect {
                *original = access_type;
            }

            let effect = if effect == DependencyStorageEffect::Unknown {
                DependencyStorageEffect::unresolved(kind, access_type)
            } else {
                effect
            };

            let effect = if !addressed || !target.is_exact() || matches!(effect,
                DependencyStorageEffect::Direct { addressed: false, .. } | DependencyStorageEffect::Indirect { addressed: false, .. }) {
                effect.into_region()
            } else { effect };

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
            self.record_effect(DependencyStorageEffect::unresolved(kind, None), self.on_normal_path);
            return;
        };

        for path in paths {
            match self.formal_effect(path, kind) {
                Some(DependencyStorageEffect::Unknown) => {
                    self.record_effect(DependencyStorageEffect::unresolved(kind, None), self.on_normal_path);
                    return;
                }
                Some(effect) => {
                    self.record_effect(effect.into_region(), self.on_normal_path);
                }
                None => {}
            }
        }
    }

    fn record_effect(&mut self, effect: DependencyStorageEffect, normal_effects: bool) {
        if normal_effects {
            // Both outcomes share immutable subject paths and checked type identities.
            self.normal_effects.insert(effect.clone());
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
            && !matches!(kind, DependencyMutationKind::Dispose(_))
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
            StorageIdentity::Static(_) => self.formal_root(path.root)
                .expect("planned static storage retains its selected dependency root"),
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
            DependencyStorageEffect::Indirect { addressed: true, reads: [].into(),
                reference: DependencySubject::root(root),
                projections: projections.into(),
                kind,
                access_type: None,
            }
        } else {
            DependencyStorageEffect::Direct { addressed: true,
                subject: DependencySubject::new(root, projections),
                kind,
                access_type: None,
            }
        })
    }
}

pub(super) fn task_effects(
    effects: Option<EffectOutcomes>,
    cleanup: impl IntoIterator<Item = Effect>,
) -> Option<EffectOutcomes> {
    let effects = effects?;
    let cleanup = cleanup.into_iter().chain([Effect::Ambient]).collect::<Vec<_>>();

    // Task scheduling and cleanup use the same captured descriptors on each outcome.
    Some(EffectOutcomes::new(effects.possible().iter().chain(&cleanup).cloned())
        .with_normal(effects.normal().iter().chain(&cleanup).cloned()))
}
