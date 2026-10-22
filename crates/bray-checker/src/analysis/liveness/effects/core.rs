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

use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::dependency::ValueInputs;
use crate::{CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::liveness) struct OperationEffect {
    pub(in crate::analysis::liveness) uses: BTreeSet<BoundDependencySubject>,
    pub(in crate::analysis::liveness) definitions: BTreeSet<BoundDependencySubject>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OperationEffects {
    pub(super) by_occurrence: BTreeMap<bray_bound_tree::BoundExecutionSite, OperationEffect>,
    operation_result_definitions:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<BoundDependencySubject>>,
    pub(in crate::analysis::liveness) universe: BTreeSet<BoundDependencySubject>,
    pub(in crate::analysis::liveness) owner_dependencies:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<BoundDependencySubject>>,
    exit_dependencies: BTreeMap<AnyBoundNodeId, BTreeSet<BoundDependencySubject>>,
    pub(super) report_owner_inputs:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<bray_bound_tree::StorageIdentityId>>,
    pub(super) value_inputs: ValueInputs,
    pub(in crate::analysis::liveness) recovered_nodes: BTreeSet<AnyBoundNodeId>,
}

impl OperationEffects {
    pub(crate) fn value_inputs(&self) -> &ValueInputs {
        &self.value_inputs
    }

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
        effects.add_selected_call_dependencies(selections, storage, dependencies);

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

        effects.retain_owned_call_dependencies(request, storage);
        effects.retain_input_dependencies(request.unit(), dependencies);

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

        effects.retain_owner_control_transfer_dependencies(request.unit());

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
                effect.uses.insert(subject);
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

    fn add_selected_call_dependencies(
        &mut self,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) {
        for (occurrence, contracts) in dependencies.calls() {
            let mut invocation =
                dependency_subjects(contracts.invocation().requirements(), storage);

            let mut returned = dependency_subjects(contracts.result().requirements(), storage);

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
                collect_dependency_subjects(deferred.requirements(), storage, &mut invocation);
            }

            self.universe.extend(invocation.iter().copied());
            self.extend_uses(*occurrence, invocation);
        }
    }

    fn retain_owner_control_transfer_dependencies(&mut self, unit: &BoundUnit) {
        let mut transfers = unit
            .tree()
            .expressions()
            .filter_map(|(expression, node)| {
                let BoundExpression::ControlTransfer(transfer) = node else {
                    return None;
                };

                transfer.operand().map(|operand| {
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

        let phase = match operation.kind() {
            AnalysisOperationKind::Result {
                phase: crate::analysis::model::AnalysisResultPhase::Begin,
                ..
            }
            | AnalysisOperationKind::ScopeExit { .. }
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
                phase: crate::analysis::model::AnalysisResultPhase::Completion,
                ..
            } => Some(AnalysisCallPhase::Completion),
            AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::PatternObservation(_)
            | AnalysisOperationKind::TaskOperation { .. }
            | AnalysisOperationKind::Recovery(_) => None,
        };

        // The index is sparse: literals and operations without storage uses have no entry.
        let effect = self.by_occurrence.get(&operation.kind().occurrence())?;

        let result_definitions = self
            .operation_result_definitions
            .get(&operation.kind().occurrence());

        Some(OperationEffectView {
            effect,
            phase,
            result_definitions,
            recovered: self.recovered_nodes.contains(&operation.kind().node()),
        })
    }

    pub(in crate::analysis::liveness) fn operation_requires_conservative_liveness(
        operation: &AnalysisOperation,
    ) -> bool {
        matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
    }

    pub(in crate::analysis::liveness) fn retained_at_exit(
        &self,
        exit: AnyBoundNodeId,
    ) -> impl Iterator<Item = &BoundDependencySubject> {
        self.exit_dependencies.get(&exit).into_iter().flatten()
    }
}

#[derive(Clone, Copy)]
pub(in crate::analysis::liveness) struct OperationEffectView<'a> {
    effect: &'a OperationEffect,
    phase: Option<AnalysisCallPhase>,
    result_definitions: Option<&'a BTreeSet<BoundDependencySubject>>,
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
        })
    }

    pub(in crate::analysis::liveness) fn definitions(
        self,
    ) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        self.effect.definitions.iter().filter(move |subject| {
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
            !matches!(
                self.phase,
                Some(AnalysisCallPhase::Input | AnalysisCallPhase::Attempt)
            )
        })
    }

    pub(in crate::analysis::liveness) fn failed_invocation_subjects(
        self,
    ) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        self.lifetime_subjects().filter(move |subject| {
            !self
                .result_definitions
                .is_some_and(|definitions| definitions.contains(subject))
        })
    }

    pub(in crate::analysis::liveness) fn defines(self, subject: &BoundDependencySubject) -> bool {
        self.definitions().any(|definition| definition == subject)
    }
}
