use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencySubject, BoundExpression, BoundExpressionId, BoundUnit,
    CheckedMemoryOperations, CheckedSemanticSelections, SemanticSelection, StoragePlan,
};

use super::retention::retained_subtree_subjects;
use super::subjects::{
    access_root_subjects, borrow_capability_subjects, collect_dependency_subjects,
    dependency_subjects,
};

use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind, AnalysisResultPhase};
use crate::dependency::ValueInputs;
use crate::{CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::liveness) struct OperationEffect {
    pub(in crate::analysis::liveness) uses: BTreeSet<BoundDependencySubject>,
    pub(in crate::analysis::liveness) definitions: BTreeSet<BoundDependencySubject>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OperationEffects {
    pub(super) boundary_uses: BTreeMap<(bray_bound_tree::BoundExecutionSite, bray_bound_tree::StorageValueConsumptionPhase), BTreeSet<BoundDependencySubject>>,
    pub(super) by_occurrence: BTreeMap<bray_bound_tree::BoundExecutionSite, OperationEffect>,
    transfers: BTreeMap<crate::analysis::id::AnalysisOperationId, OperationEffect>,
    operation_result_definitions:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<BoundDependencySubject>>,
    private_results: BTreeMap<bray_bound_tree::BoundExecutionSite, BoundDependencySubject>,
    pub(in crate::analysis::liveness) universe: BTreeSet<BoundDependencySubject>,
    pub(in crate::analysis::liveness) owner_dependencies:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<BoundDependencySubject>>,
    exit_dependencies: BTreeMap<AnyBoundNodeId, BTreeSet<BoundDependencySubject>>,
    pub(super) value_requirements: BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        BTreeSet<bray_bound_tree::BoundDependencyRequirement>,
    >,
    pub(super) value_inputs: ValueInputs,
    pub(in crate::analysis::liveness) recovered_nodes: BTreeSet<AnyBoundNodeId>,
}

impl OperationEffects {
    pub(crate) fn value_inputs(&self) -> &ValueInputs {
        &self.value_inputs
    }

    pub(in crate::analysis) fn retained_by(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> impl Iterator<Item = BoundDependencySubject> + '_ {
        self.owner_dependencies.get(&occurrence).into_iter().flatten().copied()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "liveness consumes independently computed dependency, storage, memory and cleanup tables"
    )]
    pub(crate) fn from_checked_inputs<C>(
        request: CheckerUnitView<'_, C>,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        value_inputs: ValueInputs,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        storage: &StoragePlan,
        memory: &CheckedMemoryOperations,
        graph: &crate::analysis::model::ControlFlowGraph,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) -> Self
    where
        C: CheckerRequestContext + ?Sized,
    {
        let mut effects = Self::from_storage_plan(request.unit(), storage, memory);
        let selections = semantics.selections();

        effects.value_inputs = value_inputs;
        effects.add_selected_call_dependencies(request, selections, storage, dependencies);

        for operation in graph.operations() {
            use crate::analysis::model::ScopedResultDestination as Destination;

            let mut effect = OperationEffect::default();

            match operation.kind() {
                AnalysisOperationKind::ScopedResult(transfer) => {
                    effect.uses.extend(transfer.source.map(BoundDependencySubject::Storage));

                    match transfer.destination {
                        Destination::Capability(slot) | Destination::Region { slot, displaced: None } => {
                            effect.definitions.insert(BoundDependencySubject::Storage(slot));
                        }
                        Destination::Region { slot, displaced: Some(displaced) } => {
                            effect.uses.insert(BoundDependencySubject::Storage(slot));
                            effect.definitions.insert(BoundDependencySubject::Storage(displaced));
                        }
                        Destination::Incident { .. } | Destination::Discard => {}
                    }
                }
                AnalysisOperationKind::AssignmentInstall(expression) => {
                    let slot = storage.invocation_result(expression.into()).and_then(|access| storage.root_identity(access))
                        .expect("installation consumes the selected operator's private result");

                    effect.uses.insert(BoundDependencySubject::Storage(slot));
                    effect.definitions.extend(storage.value_slot(expression).map(BoundDependencySubject::Storage));

                    effect.uses.extend(operation.kind().storage_plans(storage).filter_map(|plan| storage.access(plan.access()))
                        .flat_map(|access| access_root_subjects(storage, access.root())));
                }
                _ => continue,
            }

            effects.transfers.insert(operation.id(), effect);
        }

        for (expression, source) in effects.value_inputs.deferred_sources() {
            let completed = dependencies
                .calls()
                .get(&source.into())
                .and_then(|call| call.deferred_result())
                .expect("known lazy producer has its checked completion contract");

            effects
                .value_requirements
                .entry(expression.into())
                .or_default()
                .extend(completed.requirements().iter().cloned());

            effects
                .owner_dependencies
                .entry(expression.into())
                .or_default()
                .extend(dependency_subjects(
                    request,
                    completed.requirements(),
                    storage,
                    &effects.value_inputs,
                    selections,
                ));
        }

        effects.retain_report_dependencies(
            request,
            graph,
            selections,
            storage,
            dependencies,
            cleanup,
        );

        for plan in storage.access_plans() {
            let access = storage
                .access(plan.access())
                .expect("liveness access plan retains its access");

            let Some(capability) = access.root().borrow_capability() else {
                continue;
            };

            let ty = request.semantic_values().type_data(access.reached_type());

            if matches!(ty.as_ref(), bray_symbols::TypeData::Borrow { .. }) {
                let owner = match storage
                    .root_identity(plan.access())
                    .and_then(|root| storage.identity(root))
                {
                    Some(bray_bound_tree::StorageIdentity::ScopedCapability {
                        expression, ..
                    }) if expression == plan.expression() => {
                        bray_bound_tree::BoundExecutionSite::ScopedEnter(expression)
                    }
                    _ => plan.occurrence(),
                };

                effects
                    .owner_dependencies
                    .entry(owner)
                    .or_default()
                    .extend(borrow_capability_subjects(storage, capability));
            }
        }

        for (borrow, capability) in storage.borrow_capability_entries() {
            let Some(occurrence) = capability.occurrence() else {
                continue;
            };

            let ty = match occurrence {
                bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(
                    expression,
                )) => semantics
                    .types()
                    .expression(expression)
                    .map(|entry| entry.ty()),
                bray_bound_tree::BoundExecutionSite::CallInput {
                    expression,
                    ordinal,
                } => Some(
                    selections
                        .expression(expression)
                        .expect("staged borrow retains its selected invocation")
                        .input(ordinal)
                        .expect("staged borrow retains its selected input")
                        .ty(request.semantic_values()),
                ),
                _ => None,
            };

            if ty.is_some_and(|ty| {
                matches!(
                    request.semantic_values().type_data(ty).as_ref(),
                    bray_symbols::TypeData::Borrow { .. }
                )
            }) {
                effects
                    .owner_dependencies
                    .entry(occurrence)
                    .or_default()
                    .extend(borrow_capability_subjects(storage, borrow));
            }
        }

        effects.retain_owned_call_dependencies(request, storage, selections);
        effects.retain_input_dependencies(request.unit(), dependencies);

        for (yielded, generator) in graph.generator_yields() {
            let Some(BoundExpression::ControlTransfer(transfer)) = request.view().expression(yielded) else { continue; };

            let Some(element) = transfer.operand() else { continue; };

            let Some(owner) = storage.value_slot(generator) else { continue; };

            let retained = retained_subtree_subjects(&effects.value_inputs, element, &effects.owner_dependencies);

            effects.extend_uses(yielded, [BoundDependencySubject::Storage(owner)]);
            effects.owner_dependencies.entry(generator.into()).or_default().extend(retained);
        }

        if let bray_bound_tree::BoundUnitRoot::Expression(root) = request.unit().root() {
            let retained =
                retained_subtree_subjects(&effects.value_inputs, root, &effects.owner_dependencies);

            effects.extend_uses(root, retained.iter().copied());

            effects
                .owner_dependencies
                .entry(root.into())
                .or_default()
                .extend(retained);
        }

        effects.retain_exit_dependencies(request.unit());

        effects
    }

    pub(in crate::analysis::liveness) fn from_storage_plan(
        unit: &BoundUnit,
        storage: &StoragePlan,
        memory: &CheckedMemoryOperations,
    ) -> Self {
        let mut effects = Self::default();

        for (identity, provenance) in storage.identity_entries() {
            let subject = BoundDependencySubject::Storage(identity);

            effects.universe.insert(subject);

            if let bray_bound_tree::StorageIdentity::InvocationInput { expression, .. } = provenance {
                // The admitted private value is read by completion, including protected
                // memory primitives and deferred-input capture after callee effects.
                effects.boundary_uses.entry((expression.into(),
                    bray_bound_tree::StorageValueConsumptionPhase::Completion))
                    .or_default().insert(subject);
            }

            let Some(occurrence) = provenance.definition_occurrence() else {
                continue;
            };

            effects
                .by_occurrence
                .entry(occurrence)
                .or_default()
                .definitions
                .insert(subject);

            if occurrence.expression().is_some() {
                effects
                    .operation_result_definitions
                    .entry(occurrence)
                    .or_default()
                    .insert(subject);
            }
        }

        effects.private_results.extend(storage.value_slots().filter_map(|(expression, slot)| {
            let node = unit.tree().expression(expression)?;

            (node.has_result_region() || node.accumulates_generator_result())
                .then_some((expression.into(), BoundDependencySubject::Storage(slot)))
        }));

        for (occurrence, phase, path) in storage.value_consumption_entries() {
            let subject = BoundDependencySubject::Storage(path.root);

            effects.universe.insert(subject);
            effects.by_occurrence.entry(occurrence).or_default();
            effects.boundary_uses.entry((occurrence, phase)).or_default().insert(subject);
        }

        for plan in storage.access_plans() {
            let subject = BoundDependencySubject::StorageAccess(plan.access());
            let node = plan.node();
            let effect = effects.by_occurrence.entry(plan.occurrence()).or_default();

            effect.uses.insert(subject);
            effect.definitions.insert(subject);
            effects.universe.insert(subject);

            if storage.access_is_produced_at(plan.access(), plan.occurrence()) {
                effects
                    .operation_result_definitions
                    .entry(plan.occurrence())
                    .or_default()
                    .insert(subject);
            }

            let access = storage
                .access(plan.access())
                .expect("liveness access plan retains its access");

            if access.is_recovered() {
                effects.recovered_nodes.insert(node);
            }

            for subject in access_root_subjects(storage, access.root()) {
                if plan.purpose() == bray_bound_tree::StorageAccessPurpose::Initialize
                    && access.projections().is_empty()
                    && matches!(access.root(), bray_bound_tree::StorageAccessRoot::Storage(identity)
                        if subject == BoundDependencySubject::Storage(identity))
                {
                    effect.definitions.insert(subject);
                } else {
                    effect.uses.insert(subject);
                }

                effects.universe.insert(subject);
            }
        }

        for (capability, planned) in storage.borrow_capability_entries() {
            let subject = BoundDependencySubject::BorrowCapability(capability);

            effects.universe.insert(subject);

            let Some(occurrence) = planned.occurrence() else {
                continue;
            };

            let effect = effects.by_occurrence.entry(occurrence).or_default();

            effect.definitions.insert(subject);

            effect
                .uses
                .insert(BoundDependencySubject::StorageAccess(planned.access()));

            let access = storage
                .access(planned.access())
                .expect("liveness borrow retains its planned access");

            effect
                .uses
                .extend(access_root_subjects(storage, access.root()));

            if let Some(parent) = planned.parent() {
                let parent = BoundDependencySubject::BorrowCapability(parent);

                effect.uses.insert(parent);
                effects.universe.insert(parent);
            }

            effect.uses.remove(&subject);

            if storage.access_is_produced_at(planned.access(), occurrence) {
                effects
                    .operation_result_definitions
                    .entry(occurrence)
                    .or_default()
                    .insert(subject);
            }
        }

        let input_uses = unit
            .tree()
            .expressions()
            .map(|(expression, node)| {
                let subjects = node
                    .child_expressions()
                    .flat_map(|child| {
                        effects
                            .by_occurrence
                            .get(&bray_bound_tree::BoundExecutionSite::Node(child.into()))
                            .into_iter()
                            .flat_map(|effect| {
                                effect
                                    .uses
                                    .iter()
                                    .chain(&effect.definitions)
                                    .filter(|subject| {
                                        matches!(
                                            subject,
                                            BoundDependencySubject::Storage(_)
                                                | BoundDependencySubject::BorrowCapability(_)
                                        )
                                    })
                                    .copied()
                            })
                    })
                    .collect::<BTreeSet<_>>();

                (expression, subjects)
            })
            .collect::<Vec<_>>();

        for (expression, subjects) in input_uses {
            effects.extend_uses(expression, subjects);
        }

        for operation in memory.operations() {
            let subjects = operation
                .arguments()
                .iter()
                .flat_map(|argument| effects.subtree_subjects(unit, *argument))
                .collect::<BTreeSet<_>>();

            effects.extend_uses(operation.expression(), subjects);
        }

        effects
    }

    pub(in crate::analysis::liveness) fn retain_input_dependencies(
        &mut self,
        unit: &BoundUnit,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) {
        let inputs = unit
            .tree()
            .expressions()
            .filter(|(expression, node)| {
                dependencies.calls().contains_key(&(*expression).into())
                    || matches!(
                        node,
                        BoundExpression::Call(_)
                            | BoundExpression::Await(_)
                            | BoundExpression::Assignment(_)
                    )
            })
            .map(|(expression, node)| {
                let subjects = node
                    .child_expressions()
                    .flat_map(|input| {
                        retained_subtree_subjects(
                            &self.value_inputs,
                            input,
                            &self.owner_dependencies,
                        )
                    })
                    .collect::<BTreeSet<_>>();

                (expression, subjects)
            })
            .collect::<Vec<_>>();

        for (expression, subjects) in inputs {
            self.extend_uses(expression, subjects);
        }
    }

    fn add_selected_call_dependencies<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) {
        for (occurrence, contracts) in dependencies.calls() {
            self.value_requirements
                .entry(*occurrence)
                .or_default()
                .extend(contracts.result().requirements().iter().cloned());

            let mut invocation =
                dependency_subjects(request, contracts.invocation().requirements(), storage, &self.value_inputs, selections);

            let mut returned =
                dependency_subjects(request, contracts.result().requirements(), storage, &self.value_inputs, selections);

            if let Some(loan) = forwarded_implicit_loan(storage, selections, *occurrence, contracts) {
                returned.extend(borrow_capability_subjects(storage, loan));
            }

            if let Some(SemanticSelection::Call(call)) = occurrence
                .expression()
                .and_then(|expression| selections.expression(expression))
                && let Some(receiver) = call.receiver()
                && let Some(effect) = self.by_occurrence.get(&receiver.expression().into())
            {
                // The implicit receiver loan lasts through invocation, independently
                // of the dependencies carried by the call's returned value.
                invocation.extend(
                    effect
                        .definitions
                        .iter()
                        .filter(|subject| {
                            matches!(subject, BoundDependencySubject::BorrowCapability(_))
                        })
                        .copied(),
                );
            }

            if occurrence.expression().is_some_and(|expression| {
                matches!(selections.expression(expression), Some(SemanticSelection::Call(call))
                    if call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::NativeThreadStart))
            }) {
                // The structured thread result retains its invocation contract.
                returned.clear();
                returned.extend(invocation.iter().copied());
            }

            self.owner_dependencies
                .entry(*occurrence)
                .or_default()
                .extend(returned);

            if let Some(deferred) = contracts.deferred() {
                collect_dependency_subjects(
                    request,
                    deferred.requirements(),
                    storage,
                    &self.value_inputs,
                    selections,
                    &mut invocation,
                );
            }

            self.universe.extend(invocation.iter().copied());
            self.extend_uses(*occurrence, invocation);
        }
    }

    fn retain_exit_dependencies(&mut self, unit: &BoundUnit) {
        let mut transfers = unit
            .tree()
            .expressions()
            .filter_map(|(expression, node)| {
                let value = match node {
                    BoundExpression::ControlTransfer(transfer) => transfer.operand(),
                    _ if bray_bound_tree::StorageScope::for_iteration(expression, node)
                        .is_some() =>
                    {
                        Some(expression)
                    }
                    _ => None,
                };

                value.map(|operand| {
                    (
                        expression,
                        retained_subtree_subjects(
                            &self.value_inputs,
                            operand,
                            &self.owner_dependencies,
                        ),
                    )
                })
            })
            .collect::<Vec<_>>();

        for (exit, value, projection) in self.value_inputs.propagated_errors() {
            let (value, _) = self.value_inputs.project(value, &[projection]);

            transfers.push((
                exit,
                retained_subtree_subjects(&self.value_inputs, value, &self.owner_dependencies),
            ));
        }

        for (expression, subjects) in transfers {
            self.extend_uses(expression, subjects.iter().copied());
            self.exit_dependencies.insert(expression.into(), subjects);
        }
    }

    pub(super) fn extend_uses(
        &mut self,
        occurrence: impl Into<bray_bound_tree::BoundExecutionSite>,
        subjects: impl IntoIterator<Item = BoundDependencySubject>,
    ) {
        self.by_occurrence
            .entry(occurrence.into())
            .or_default()
            .uses
            .extend(subjects);
    }

    fn subtree_subjects(
        &self,
        unit: &BoundUnit,
        root: BoundExpressionId,
    ) -> BTreeSet<BoundDependencySubject> {
        let mut subjects = BTreeSet::new();

        let walked = bray_bound_tree::walk_bound_tree(unit.tree(), root, |event| {
            if let bray_bound_tree::BoundWalkEvent::Enter(node) = event
                && let Some(effect) = self.by_occurrence.get(&node.into())
            {
                subjects.extend(effect.uses.iter().copied());
                subjects.extend(effect.definitions.iter().copied());
            }

            bray_bound_tree::BoundWalkControl::Continue
        });

        assert_eq!(
            walked,
            bray_bound_tree::BoundWalkOutcome::Completed,
            "liveness retains its complete bound subtree"
        );

        subjects
    }

    pub(in crate::analysis::liveness) fn effect(
        &self,
        node: AnyBoundNodeId,
    ) -> Option<&OperationEffect> {
        self.by_occurrence.get(&node.into())
    }

    pub(in crate::analysis::liveness) fn operation_effect(
        &self,
        operation: &AnalysisOperation,
    ) -> Option<OperationEffectView<'_>> {
        use crate::analysis::model::AnalysisSuspensionKind;

        let kind = operation.kind();
        let occurrence = kind.occurrence();
        let private_result = self.private_results.get(&occurrence);

        let phase = match kind {
            AnalysisOperationKind::ScopedResult(_) | AnalysisOperationKind::AssignmentInstall(_) => {
                return self.transfers.get(&operation.id()).map(|effect| OperationEffectView {
                    effect, boundary_uses: None, phase: None, result_definitions: None,
                    private_result: None, result_phase: None, recovered: false,
                });
            }
            AnalysisOperationKind::Result {
                phase: AnalysisResultPhase::Begin, ..
            } if private_result.is_none() => return None,
            AnalysisOperationKind::ScopeExit { .. }
            | AnalysisOperationKind::Suspension {
                kind: AnalysisSuspensionKind::ScopedCall,
                ..
            } => {
                // Scope cleanup is resolved by storage flow. A scoped suspension sits
                // between its separate call attempt and completion and has no new transfer.
                return None;
            }
            AnalysisOperationKind::Suspension {
                kind: AnalysisSuspensionKind::Await | AnalysisSuspensionKind::Yield,
                ..
            } => Some(AnalysisCallPhase::Attempt),
            AnalysisOperationKind::Call { phase, .. } => Some(phase),
            AnalysisOperationKind::Result {
                phase: AnalysisResultPhase::Production,
                ..
            } => Some(AnalysisCallPhase::Attempt),
            AnalysisOperationKind::Result {
                phase: AnalysisResultPhase::Begin | AnalysisResultPhase::Completion,
                ..
            } => Some(AnalysisCallPhase::Completion),
            AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::PatternObservation(_)
            | AnalysisOperationKind::Recovery(_) => None,
        };

        // The index is sparse: literals and operations without storage uses have no entry.
        let effect = self.by_occurrence.get(&occurrence)?;
        let result_definitions = self.operation_result_definitions.get(&occurrence);

        let boundary_uses = kind.value_consumption_phase()
            .and_then(|phase| self.boundary_uses.get(&(occurrence, phase)));

        let result_phase = match kind {
            AnalysisOperationKind::Result { phase, .. } => Some(phase),
            _ => None,
        };

        Some(OperationEffectView {
            effect,
            boundary_uses,
            phase,
            result_definitions,
            private_result,
            result_phase,
            recovered: self.recovered_nodes.contains(&kind.node()),
        })
    }

    pub(in crate::analysis::liveness) fn operation_requires_conservative_liveness(
        operation: &AnalysisOperation,
    ) -> bool {
        matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
    }

    pub(in crate::analysis::liveness) fn retained_at_exit(
        &self,
        scope: bray_bound_tree::StorageScope,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
    ) -> impl Iterator<Item = BoundDependencySubject> + '_ {
        let abandoned = match scope {
            bray_bound_tree::StorageScope::AbandonedTransfer { transfer, value } => Some((transfer, value)),
            _ => None,
        };

        (preserves_exit_value || abandoned.is_some()).then(|| self.exit_dependencies.get(&exit)).flatten()
            .into_iter().flatten().copied()
            .chain(abandoned.map(|(_, value)| BoundDependencySubject::Storage(value)))
            .chain(abandoned.into_iter().flat_map(|(transfer, _)| self.retained_by(transfer.into())))
    }

    pub(in crate::analysis::liveness) fn failed_invocation_subjects(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        consumption: Option<bray_bound_tree::StorageValueConsumptionPhase>,
    ) -> impl Iterator<Item = &BoundDependencySubject> {
        let effects = self.by_occurrence.get(&invocation);
        let results = self.operation_result_definitions.get(&invocation);
        let boundary = consumption.and_then(|phase| self.boundary_uses.get(&(invocation, phase)));

        effects.into_iter().flat_map(|effect| effect.uses.iter().chain(&effect.definitions))
            .chain(boundary.into_iter().flatten())
            .filter(move |subject| results.is_none_or(|results| !results.contains(subject)))
    }
}

fn forwarded_implicit_loan(
    storage: &StoragePlan,
    selections: &CheckedSemanticSelections,
    occurrence: bray_bound_tree::BoundExecutionSite,
    contracts: &bray_bound_tree::BoundCallDependencyContracts,
) -> Option<bray_bound_tree::BorrowCapabilityId> {
    use bray_symbols::{DependencyReferenceKind, DependencySubjectRoot, SymbolOrdinal};

    let input = storage.implicit_input_plan(occurrence)?;
    let loan = storage.borrow_for_plan(input)?;
    let selection = selections.expression(occurrence.expression()?)?;
    let root = selection.invocation_input_root(occurrence, SymbolOrdinal::new(0))?;

    // A forwarded descriptor still requires the capability formed at input.
    // An address formation or a reached region carries different support.
    contracts.captured_values().iter()
        .flat_map(|row| contracts.storage().reference_outputs([row]))
        .any(|reference| {
            let relation = reference.relation();

            relation.exit().subject_root() == DependencySubjectRoot::Result
                && relation.kind() == DependencyReferenceKind::Borrow
                && relation.is_exact()
                && relation.entry().is_some_and(|entry| entry.subject_root() == root && entry.projections().is_empty())
                && relation.dereferences() == 0
                && relation.target_projections().is_empty()
                && reference.addressed_type().is_none()
        }).then_some(loan)
}

#[derive(Clone, Copy)]
pub(in crate::analysis::liveness) struct OperationEffectView<'a> {
    effect: &'a OperationEffect,
    boundary_uses: Option<&'a BTreeSet<BoundDependencySubject>>,
    phase: Option<AnalysisCallPhase>,
    result_definitions: Option<&'a BTreeSet<BoundDependencySubject>>,
    private_result: Option<&'a BoundDependencySubject>,
    result_phase: Option<AnalysisResultPhase>,
    recovered: bool,
}

impl<'a> OperationEffectView<'a> {
    pub(in crate::analysis::liveness) fn uses(
        self,
    ) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        // Defining an identity establishes its birth, even when the same operation consumes it.
        self.effect.uses.iter().filter(move |subject| {
            self.phase != Some(AnalysisCallPhase::Completion)
                && (self.recovered || !self.effect.definitions.contains(subject))
        }).chain(self.boundary_uses.into_iter().flatten())
            .chain(self.private_result.filter(|_| self.result_phase == Some(AnalysisResultPhase::Completion)))
    }

    pub(in crate::analysis::liveness) fn definitions(
        self,
    ) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        self.effect.definitions.iter().filter(move |subject| {
            // Beginning a region ends backward demand for its old slot incarnation.
            // Completion observes the existing result, which was initialized on a producer edge.
            match self.result_phase {
                Some(AnalysisResultPhase::Begin) => return self.private_result == Some(*subject),
                Some(AnalysisResultPhase::Completion) if self.private_result == Some(*subject) => return false,
                _ => {}
            }

            let is_result = self
                .result_definitions
                .is_some_and(|definitions| definitions.contains(subject));

            match self.phase {
                Some(AnalysisCallPhase::Input) => !is_result,
                Some(AnalysisCallPhase::Attempt) => false,
                Some(AnalysisCallPhase::Completion) => is_result,
                None => true,
            }
        })
    }

    fn lifetime_subjects(self) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        self.effect
            .uses
            .iter()
            .chain(&self.effect.definitions)
            .chain(self.boundary_uses.into_iter().flatten())
            .filter(|subject| {
                matches!(
                    subject,
                    BoundDependencySubject::Storage(_)
                        | BoundDependencySubject::BorrowCapability(_)
                )
            })
    }

    pub(in crate::analysis::liveness) fn retirement_subjects(
        self,
    ) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        self.lifetime_subjects().filter(move |_| {
            self.result_phase != Some(AnalysisResultPhase::Begin) && !matches!(
                self.phase,
                Some(AnalysisCallPhase::Input | AnalysisCallPhase::Attempt)
            )
        })
    }

    pub(in crate::analysis::liveness) fn defines(self, subject: &BoundDependencySubject) -> bool {
        self.definitions().any(|definition| definition == subject)
    }
}
