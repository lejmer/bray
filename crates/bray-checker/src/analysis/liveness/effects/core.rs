use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundDependencySubject, BoundExpression, BoundExpressionId, BoundUnit,
    CheckedMemoryOperations, CheckedSemanticSelections, DependencyContractInstantiationError,
    SemanticSelection, StoragePlan,
};
use bray_symbols::CallableSignatureQuery;

use super::retention::retained_subtree_subjects;
use super::subjects::{
    access_root_subjects, borrow_capability_subjects, collect_dependency_subjects,
    dependency_subjects,
};

use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::dependency::{ValueInputs, selected_call_contracts};
use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::liveness) struct OperationEffect {
    pub(in crate::analysis::liveness) uses: BTreeSet<BoundDependencySubject>,
    pub(in crate::analysis::liveness) definitions: BTreeSet<BoundDependencySubject>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::liveness) struct OperationEffects {
    pub(super) by_occurrence: BTreeMap<bray_bound_tree::BoundExecutionSite, OperationEffect>,
    operation_result_definitions: BTreeMap<BoundExpressionId, BTreeSet<BoundDependencySubject>>,
    pub(in crate::analysis::liveness) universe: BTreeSet<BoundDependencySubject>,
    pub(in crate::analysis::liveness) owner_dependencies:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<BoundDependencySubject>>,
    exit_dependencies: BTreeMap<AnyBoundNodeId, BTreeSet<BoundDependencySubject>>,
    pub(super) value_inputs: ValueInputs,
    pub(in crate::analysis::liveness) recovered_nodes: BTreeSet<AnyBoundNodeId>,
}

impl OperationEffects {
    pub(in crate::analysis::liveness) fn from_checked_inputs<C>(
        request: CheckerUnitView<'_, C>,
        selections: &CheckedSemanticSelections,
        types: &bray_bound_tree::CheckedExpressionTypes,
        patterns: &bray_bound_tree::CheckedPatterns,
        storage: &StoragePlan,
        memory: &CheckedMemoryOperations,
    ) -> Result<Self, CheckerQueryError<C::UpstreamError>>
    where
        C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
    {
        let mut effects = Self::from_storage_plan(request.unit(), storage, memory);

        effects.value_inputs =
            ValueInputs::prepare(request, types, selections, patterns, |callable| {
                request.context().callable_result_dependencies(callable)
            })?;

        effects.retain_call_input_dependencies(request.unit());
        effects.add_selected_call_dependencies(request, selections, storage)?;

        for plan in storage.access_plans() {
            let Some(access) = storage.access(plan.access()) else {
                continue;
            };

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
                    }) => bray_bound_tree::BoundExecutionSite::ScopedEnter(expression),
                    _ => plan.expression().into(),
                };

                effects
                    .owner_dependencies
                    .entry(owner)
                    .or_default()
                    .extend(borrow_capability_subjects(storage, capability));
            }
        }

        for (borrow, capability) in storage.borrow_capability_entries() {
            if let Some(expression) = capability.expression() {
                let owner = if matches!(
                    selections.expression(expression),
                    Some(SemanticSelection::ScopedUse(_))
                ) {
                    bray_bound_tree::BoundExecutionSite::ScopedEnter(expression)
                } else {
                    expression.into()
                };

                effects
                    .owner_dependencies
                    .entry(owner)
                    .or_default()
                    .extend(borrow_capability_subjects(storage, borrow));
            }
        }

        effects.retain_owned_call_dependencies(request, storage);

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

        Ok(effects)
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

            let Some(node) = provenance.definition_node() else {
                continue;
            };

            effects
                .by_occurrence
                .entry(node.into())
                .or_default()
                .definitions
                .insert(subject);

            if let AnyBoundNodeId::Expression(expression) = node {
                effects
                    .operation_result_definitions
                    .entry(expression)
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

            let Some(access) = storage.access(plan.access()) else {
                effects.recovered_nodes.insert(node);

                continue;
            };

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

            let Some(expression) = planned.expression() else {
                continue;
            };

            let occurrence = match unit.tree().expression(expression) {
                Some(BoundExpression::Structured(scoped))
                    if scoped.kind() == bray_bound_tree::BoundStructuredExpressionKind::With =>
                {
                    bray_bound_tree::BoundExecutionSite::ScopedEnter(expression)
                }
                _ => expression.into(),
            };

            let effect = effects.by_occurrence.entry(occurrence).or_default();

            effect.definitions.insert(subject);

            effect
                .uses
                .insert(BoundDependencySubject::StorageAccess(planned.access()));

            if let Some(access) = storage.access(planned.access()) {
                effect
                    .uses
                    .extend(access_root_subjects(storage, access.root()));
            }

            if let Some(parent) = planned.parent() {
                let parent = BoundDependencySubject::BorrowCapability(parent);

                effect.uses.insert(parent);
                effects.universe.insert(parent);
            }

            effect.uses.remove(&subject);
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

        let await_uses = unit
            .tree()
            .expressions()
            .filter_map(|(expression, node)| match node {
                BoundExpression::Await(awaited) => Some((
                    expression,
                    effects.subtree_subjects(unit, awaited.operand()),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();

        for (expression, subjects) in await_uses {
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

    pub(in crate::analysis::liveness) fn retain_call_input_dependencies(
        &mut self,
        unit: &BoundUnit,
    ) {
        let call_inputs = unit
            .tree()
            .expressions()
            .filter_map(|(expression, node)| {
                let BoundExpression::Call(call) = node else {
                    return None;
                };

                let subjects = std::iter::once(call.callee())
                    .chain(
                        call.arguments()
                            .iter()
                            .map(bray_bound_tree::BoundArgument::expression),
                    )
                    .flat_map(|input| self.subtree_subjects(unit, input))
                    .collect::<BTreeSet<_>>();

                Some((expression, subjects))
            })
            .collect::<Vec<_>>();

        for (expression, subjects) in call_inputs {
            self.extend_uses(expression, subjects);
        }
    }

    fn add_selected_call_dependencies<C>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selections: &CheckedSemanticSelections,
        storage: &StoragePlan,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>>
    where
        C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
    {
        for entry in selections.entries() {
            if let SemanticSelection::ScopedUse(scoped) = entry.selection() {
                for occurrence in [
                    bray_bound_tree::BoundExecutionSite::ScopedEnter(entry.expression()),
                    bray_bound_tree::BoundExecutionSite::ScopedExit(entry.expression()),
                ] {
                    let contracts = crate::dependency::selected_scoped_contracts(
                        request, storage, scoped, occurrence,
                    )
                    .map_err(|error| match error {
                        DependencyContractInstantiationError::Resolution(error) => error,
                        DependencyContractInstantiationError::UnresolvedWitness => panic!(
                            "selected scoped operation {:?} must retain its dependency witness",
                            occurrence
                        ),
                        DependencyContractInstantiationError::ForeignUnit => panic!(
                            "selected scoped operation {:?} must belong to liveness unit {:?}",
                            occurrence,
                            request.unit().unit()
                        ),
                    })?;

                    let mut subjects =
                        dependency_subjects(contracts.invocation().requirements(), storage);

                    if let Some(deferred) = contracts.deferred() {
                        collect_dependency_subjects(
                            deferred.requirements(),
                            storage,
                            &mut subjects,
                        );
                    }

                    self.universe.extend(subjects.iter().copied());
                    self.extend_uses(occurrence, subjects);

                    self.owner_dependencies
                        .entry(occurrence)
                        .or_default()
                        .extend(dependency_subjects(
                            contracts.result().requirements(),
                            storage,
                        ));
                }

                continue;
            }

            let SemanticSelection::Call(call) = entry.selection() else {
                continue;
            };

            let contracts = match selected_call_contracts(
                request,
                storage,
                entry.expression(),
                call,
                &self.value_inputs,
            ) {
                Ok(contracts) => contracts,
                Err(DependencyContractInstantiationError::Resolution(error)) => {
                    return Err(error);
                }
                Err(DependencyContractInstantiationError::UnresolvedWitness) => {
                    panic!(
                        "A call reached concrete dependency checking without its selected witness. in add_selected_call_dependencies, expression: {:?}",
                        entry.expression()
                    );
                }
                Err(DependencyContractInstantiationError::ForeignUnit) => {
                    panic!(
                        "Liveness inputs or durable decisions violate the requested unit contract. in add_selected_call_dependencies"
                    );
                }
            };

            let invocation = dependency_subjects(contracts.invocation().requirements(), storage);
            let returned = dependency_subjects(contracts.result().requirements(), storage);

            self.owner_dependencies
                .entry(entry.expression().into())
                .or_default()
                .extend(returned);

            if call.implementation_hook()
                == Some(bray_compiler_known::ImplementationHook::NativeThreadStart)
            {
                // The structured thread result retains the same contract used during invocation.
                self.owner_dependencies
                    .insert(entry.expression().into(), invocation.clone());
            }

            let mut subjects = invocation;

            if let Some(deferred) = contracts.deferred() {
                collect_dependency_subjects(deferred.requirements(), storage, &mut subjects);
            }

            self.universe.extend(subjects.iter().copied());
            self.extend_uses(entry.expression(), subjects);
        }

        Ok(())
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
        let mut pending = vec![root];

        while let Some(expression) = pending.pop() {
            let node = AnyBoundNodeId::Expression(expression);

            if let Some(effect) = self.by_occurrence.get(&node.into()) {
                subjects.extend(effect.uses.iter().copied());
                subjects.extend(effect.definitions.iter().copied());
            }

            if let Some(node) = unit.tree().expression(expression) {
                pending.extend(node.child_expressions());
            }
        }

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
        if matches!(operation.kind(), AnalysisOperationKind::ScopeExit { .. }) {
            return None;
        }

        let effect = self.by_occurrence.get(&operation.kind().occurrence())?;

        let (phase, result_definitions) = match operation.kind() {
            AnalysisOperationKind::Call {
                invocation:
                    bray_bound_tree::BoundExecutionSite::Node(
                        bray_bound_tree::AnyBoundNodeId::Expression(expression),
                    ),
                phase,
            } => (
                Some(phase),
                self.operation_result_definitions.get(&expression),
            ),
            AnalysisOperationKind::Call { phase, .. } => (Some(phase), None),
            _ => (None, None),
        };

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
                Some(AnalysisCallPhase::Attempt) => !is_result,
                Some(AnalysisCallPhase::Completion) => is_result,
                None => true,
            }
        })
    }

    pub(in crate::analysis::liveness) fn defines(self, subject: &BoundDependencySubject) -> bool {
        self.definitions().any(|definition| definition == subject)
    }
}
