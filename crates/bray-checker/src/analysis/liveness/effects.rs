use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencyRequirement, BoundDependencySubject,
    BoundExpression, BoundExpressionId, BoundUnit, CheckedMemoryOperations,
    CheckedSemanticSelections, DependencyContractInstantiationError, SemanticSelection,
    StorageAccessRoot, StorageBinding, StorageIdentityId, StoragePlan,
};
use bray_symbols::CallableSignatureQuery;

use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::analysis::storage_index::index_storage_roots;
use crate::dependency::{ValueInputs, selected_call_contracts};
use crate::storage::value_transfer_bindings;
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct OperationEffect {
    pub(super) uses: BTreeSet<BoundDependencySubject>,
    pub(super) definitions: BTreeSet<BoundDependencySubject>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct OperationEffects {
    by_node: BTreeMap<AnyBoundNodeId, OperationEffect>,
    operation_result_definitions: BTreeMap<BoundExpressionId, BTreeSet<BoundDependencySubject>>,
    pub(super) universe: BTreeSet<BoundDependencySubject>,
    pub(super) owner_dependencies: BTreeMap<BoundExpressionId, BTreeSet<BoundDependencySubject>>,
    exit_dependencies: BTreeMap<AnyBoundNodeId, BTreeSet<BoundDependencySubject>>,
    value_inputs: ValueInputs,
    pub(super) recovered_nodes: BTreeSet<AnyBoundNodeId>,
}

impl OperationEffects {
    pub(super) fn from_checked_inputs<C>(
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

            let ty = request
                .semantic_values()
                .type_data(access.reached_type())
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            if matches!(ty.as_ref(), bray_symbols::TypeData::Borrow { .. }) {
                effects
                    .owner_dependencies
                    .entry(plan.expression())
                    .or_default()
                    .extend(borrow_capability_subjects(storage, capability));
            }
        }

        for (borrow, capability) in storage.borrow_capability_entries() {
            if let Some(expression) = capability.expression() {
                effects
                    .owner_dependencies
                    .entry(expression)
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
                .entry(root)
                .or_default()
                .extend(retained);
        }

        effects.retain_owner_control_transfer_dependencies(request.unit());

        Ok(effects)
    }

    pub(super) fn from_storage_plan(
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
                .by_node
                .entry(node)
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
            let effect = effects.by_node.entry(node).or_default();

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

            let node = AnyBoundNodeId::Expression(expression);
            let effect = effects.by_node.entry(node).or_default();

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

    pub(super) fn retain_call_input_dependencies(&mut self, unit: &BoundUnit) {
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
                Err(DependencyContractInstantiationError::Resolution(
                    CheckerQueryError::Infrastructure(
                        CheckerInfrastructureError::InvalidSemanticSelectionInput,
                    ),
                )) => {
                    self.recovered_nodes
                        .insert(AnyBoundNodeId::Expression(entry.expression()));

                    continue;
                }
                Err(DependencyContractInstantiationError::Resolution(error)) => {
                    return Err(error);
                }
                Err(DependencyContractInstantiationError::UnresolvedWitness) => {
                    return Err(CheckerInfrastructureError::StorageFlow(
                        crate::CheckerStorageFlowFailure::UnresolvedDependencyWitness {
                            expression: entry.expression(),
                        },
                    )
                    .into());
                }
                Err(DependencyContractInstantiationError::ForeignUnit) => {
                    return Err(CheckerInfrastructureError::InvalidLiveness.into());
                }
            };

            let invocation = dependency_subjects(contracts.invocation().requirements(), storage);
            let returned = dependency_subjects(contracts.result().requirements(), storage);

            self.owner_dependencies
                .entry(entry.expression())
                .or_default()
                .extend(returned);

            if call.implementation_hook()
                == Some(bray_compiler_known::ImplementationHook::NativeThreadStart)
            {
                // The structured thread result retains the same contract used during invocation.
                self.owner_dependencies
                    .insert(entry.expression(), invocation.clone());
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

    fn retain_owned_call_dependencies<C>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) where
        C: CheckerRequestContext + ?Sized,
    {
        let (accesses_by_root, _) = index_storage_roots(storage);

        let initializations = value_transfer_bindings(request, storage);

        let value_sources_by_root =
            index_value_sources_by_root(storage, &initializations, &self.value_inputs);

        for expression in self.owner_dependencies.keys().copied().collect::<Vec<_>>() {
            let nested_borrows = retained_storage_borrows(
                storage,
                &value_sources_by_root,
                self.owner_dependencies
                    .get(&expression)
                    .into_iter()
                    .flat_map(|subjects| subjects.iter().copied()),
                self,
            );

            self.owner_dependencies
                .entry(expression)
                .or_default()
                .extend(nested_borrows);
        }

        // Keep seed contracts immutable while local ownership propagation reaches its fixed point.
        let mut retained_by_expression = self.owner_dependencies.clone();

        loop {
            let mut changed = false;

            for (initializer, bindings) in &initializations {
                let retained = retained_subtree_subjects(
                    &self.value_inputs,
                    *initializer,
                    &retained_by_expression,
                );

                if retained.is_empty() {
                    continue;
                }

                for binding in bindings {
                    let root = match binding {
                        StorageBinding::Identity(identity) => Some(*identity),
                        StorageBinding::Access(access) => storage.root_identity(*access),
                    };

                    let Some(root) = root else {
                        continue;
                    };

                    for expression in accesses_by_root.get(&root).into_iter().flatten() {
                        if self.value_inputs.is_independent(*expression)
                            || self
                                .value_inputs
                                .projected_initializer(*expression)
                                .is_some()
                        {
                            continue;
                        }

                        let expression_retention =
                            retained_by_expression.entry(*expression).or_default();

                        let previous = expression_retention.len();

                        expression_retention.extend(retained.iter().copied());
                        changed |= expression_retention.len() != previous;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        for (expression, retained) in &retained_by_expression {
            let node = AnyBoundNodeId::Expression(*expression);
            let defined = self.by_node.get(&node).map(|effect| &effect.definitions);

            let subjects = retained
                .iter()
                .copied()
                .filter(|subject| !defined.is_some_and(|definitions| definitions.contains(subject)))
                .collect::<Vec<_>>();

            self.extend_uses(*expression, subjects);
        }

        self.owner_dependencies = retained_by_expression;
    }

    fn extend_uses(
        &mut self,
        expression: BoundExpressionId,
        subjects: impl IntoIterator<Item = BoundDependencySubject>,
    ) {
        self.by_node
            .entry(AnyBoundNodeId::Expression(expression))
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

            if let Some(effect) = self.by_node.get(&node) {
                subjects.extend(effect.uses.iter().copied());
                subjects.extend(effect.definitions.iter().copied());
            }

            if let Some(node) = unit.tree().expression(expression) {
                pending.extend(node.child_expressions());
            }
        }

        subjects
    }

    pub(super) fn effect(&self, node: AnyBoundNodeId) -> Option<&OperationEffect> {
        self.by_node.get(&node)
    }

    pub(super) fn operation_effect(
        &self,
        operation: &AnalysisOperation,
    ) -> Option<OperationEffectView<'_>> {
        if matches!(operation.kind(), AnalysisOperationKind::ScopeExit { .. }) {
            return None;
        }

        let effect = self.effect(operation.kind().node())?;

        let (phase, result_definitions) = match operation.kind() {
            AnalysisOperationKind::Call { expression, phase } => (
                Some(phase),
                self.operation_result_definitions.get(&expression),
            ),
            _ => (None, None),
        };

        Some(OperationEffectView {
            effect,
            phase,
            result_definitions,
        })
    }

    pub(super) fn operation_requires_conservative_liveness(operation: &AnalysisOperation) -> bool {
        matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
    }

    pub(super) fn retained_at_exit(
        &self,
        exit: AnyBoundNodeId,
    ) -> impl Iterator<Item = &BoundDependencySubject> {
        self.exit_dependencies.get(&exit).into_iter().flatten()
    }
}

#[derive(Clone, Copy)]
pub(super) struct OperationEffectView<'a> {
    effect: &'a OperationEffect,
    phase: Option<AnalysisCallPhase>,
    result_definitions: Option<&'a BTreeSet<BoundDependencySubject>>,
}

impl<'a> OperationEffectView<'a> {
    pub(super) fn uses(self) -> impl Iterator<Item = &'a BoundDependencySubject> + Clone + 'a {
        self.effect
            .uses
            .iter()
            .filter(move |_| self.phase != Some(AnalysisCallPhase::Completion))
    }

    pub(super) fn definitions(
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

    pub(super) fn defines(self, subject: &BoundDependencySubject) -> bool {
        self.definitions().any(|definition| definition == subject)
    }
}

fn index_value_sources_by_root(
    storage: &StoragePlan,
    initializations: &BTreeMap<BoundExpressionId, Vec<StorageBinding>>,
    inputs: &ValueInputs,
) -> BTreeMap<StorageIdentityId, Vec<BoundExpressionId>> {
    let mut result = BTreeMap::<_, Vec<_>>::new();

    for (initializer, bindings) in initializations {
        for binding in bindings {
            let root = match binding {
                StorageBinding::Identity(identity) => Some(*identity),
                StorageBinding::Access(access) => storage.root_identity(*access),
            };

            if let Some(root) = root {
                result.entry(root).or_default().push(*initializer);
            }
        }
    }

    for plan in storage.access_plans() {
        let Some(root) = storage.root_identity(plan.access()) else {
            continue;
        };

        result.entry(root).or_default().extend(
            inputs
                .projected_operands(plan.expression())
                .map(|(value, path)| inputs.project(value, path).0),
        );
    }

    result
}

fn retained_storage_borrows(
    storage: &StoragePlan,
    value_sources_by_root: &BTreeMap<StorageIdentityId, Vec<BoundExpressionId>>,
    subjects: impl IntoIterator<Item = BoundDependencySubject>,
    effects: &OperationEffects,
) -> BTreeSet<BoundDependencySubject> {
    let mut borrows = BTreeSet::new();
    let mut pending = subjects.into_iter().collect::<Vec<_>>();
    let mut visited_roots = BTreeSet::new();

    while let Some(subject) = pending.pop() {
        match subject {
            BoundDependencySubject::BorrowCapability(_) => {
                borrows.insert(subject);
            }
            BoundDependencySubject::Storage(root) => {
                if !visited_roots.insert(root) {
                    continue;
                }

                for initializer in value_sources_by_root.get(&root).into_iter().flatten() {
                    pending.extend(retained_subtree_subjects(
                        &effects.value_inputs,
                        *initializer,
                        &effects.owner_dependencies,
                    ));
                }

                if let Some(bray_bound_tree::StorageIdentity::Temporary(expression)) =
                    storage.identity(root)
                {
                    pending.extend(retained_subtree_subjects(
                        &effects.value_inputs,
                        expression,
                        &effects.owner_dependencies,
                    ));
                }
            }
            BoundDependencySubject::StorageAccess(access) => {
                if let Some(root) = storage.root_identity(access) {
                    pending.push(BoundDependencySubject::Storage(root));
                }
            }
            BoundDependencySubject::ScopedCapability(_)
            | BoundDependencySubject::ImplementationWitness(_)
            | BoundDependencySubject::ProductStatic(_)
            | BoundDependencySubject::ExactThreadStatic(_)
            | BoundDependencySubject::LifecycleObligation(_) => {}
        }
    }

    borrows
}

fn retained_subtree_subjects(
    inputs: &ValueInputs,
    root: BoundExpressionId,
    retained_by_expression: &BTreeMap<BoundExpressionId, BTreeSet<BoundDependencySubject>>,
) -> BTreeSet<BoundDependencySubject> {
    let mut retained = BTreeSet::new();
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();

    while let Some(expression) = pending.pop() {
        if !visited.insert(expression) || inputs.is_independent(expression) {
            continue;
        }

        if let Some(subjects) = retained_by_expression.get(&expression) {
            retained.extend(subjects.iter().copied());
        }

        pending.extend(inputs.operands(expression));

        pending.extend(
            inputs
                .projected_operands(expression)
                .map(|(value, path)| inputs.project(value, path).0),
        );
    }

    retained
}

fn dependency_subjects(
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
) -> BTreeSet<BoundDependencySubject> {
    let mut subjects = BTreeSet::new();

    collect_dependency_subjects(requirements, storage, &mut subjects);

    subjects
}

fn collect_dependency_subjects(
    requirements: &[BoundDependencyRequirement],
    storage: &StoragePlan,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    for requirement in requirements {
        match requirement {
            BoundDependencyRequirement::Direct { subject, .. } => {
                subjects.insert(*subject);

                if let BoundDependencySubject::StorageAccess(access) = subject
                    && let Some(access) = storage.access(*access)
                {
                    subjects.extend(access_root_subjects(storage, access.root()));
                }
            }
            BoundDependencyRequirement::Guarded(requirement) => {
                let guard = requirement.guard().subject();

                subjects.insert(guard);
                collect_dependency_subjects(requirement.requirements(), storage, subjects);
            }
        }
    }
}

fn access_root_subjects(
    storage: &StoragePlan,
    root: StorageAccessRoot,
) -> Vec<BoundDependencySubject> {
    match root {
        StorageAccessRoot::Storage(storage)
        | StorageAccessRoot::Recovery(storage)
        | StorageAccessRoot::OwnedIndirection { storage, .. } => {
            vec![BoundDependencySubject::Storage(storage)]
        }
        StorageAccessRoot::Borrow(capability) => borrow_capability_subjects(storage, capability),
        StorageAccessRoot::BorrowedStorage {
            capability,
            storage: retained,
        } => {
            let mut subjects = vec![BoundDependencySubject::Storage(retained)];

            subjects.extend(borrow_capability_subjects(storage, capability));

            subjects
        }
    }
}

fn borrow_capability_subjects(
    storage: &StoragePlan,
    capability: BorrowCapabilityId,
) -> Vec<BoundDependencySubject> {
    let mut subjects = Vec::new();
    let mut current = Some(capability);

    while let Some(capability) = current {
        subjects.push(BoundDependencySubject::BorrowCapability(capability));

        current = storage
            .borrow_capability(capability)
            .and_then(|planned| planned.parent());
    }

    subjects
}
