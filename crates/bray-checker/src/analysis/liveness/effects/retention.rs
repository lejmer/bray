use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundDependencySubject, StorageBinding, StorageIdentityId, StoragePlan};

use super::core::OperationEffects;
use crate::analysis::storage_index::index_storage_roots;
use crate::dependency::ValueInputs;
use crate::storage::value_transfer_bindings;
use crate::{CheckerRequestContext, CheckerUnitView};

impl OperationEffects {
    pub(super) fn retain_report_dependencies<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        graph: &crate::analysis::model::ControlFlowGraph,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        storage: &StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};
        use bray_bound_tree::{BoundExecutionSite, CallReportOutput, SemanticSelection};

        let reporting = cleanup
            .reporting_accesses(dependencies)
            .map(|access| {
                storage
                    .root_identity(access)
                    .expect("selected cleanup retains its owner")
            })
            .collect::<BTreeSet<_>>();

        let mut reporting_scopes = BTreeMap::<_, BTreeSet<_>>::new();

        for requirement in cleanup.requirements.iter().filter(|requirement| {
            !requirement.transfers() && reporting.contains(&requirement.identity())
        }) {
            if let Some(scope) = requirement.owner() {
                reporting_scopes
                    .entry(scope)
                    .or_default()
                    .insert(requirement.identity());
            }
        }

        for (occurrence, call) in dependencies.calls() {
            let (BoundExecutionSite::ScopedEnter(expression)
            | BoundExecutionSite::ScopedExit(expression)) = occurrence
            else {
                continue;
            };

            let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(*expression)
            else {
                panic!("scoped result retention has its selected lifecycle");
            };

            if crate::representation::type_representation(
                request,
                scoped.invocation(*occurrence).1.result(),
            ) == Some(bray_compiler_known::RepresentationRole::Result)
            {
                // Failed entry and ordinary exit errors are payloads of the with result.
                self.owner_dependencies
                    .entry((*expression).into())
                    .or_default()
                    .extend(super::subjects::dependency_subjects(
                        call.result().requirements(),
                        storage,
                    ));
            }
        }

        for block in graph.blocks() {
            let Some(boundary) = block.report_boundary() else {
                continue;
            };

            for operation in block.operations().iter().map(|id| {
                graph
                    .operation(*id)
                    .expect("report retention operation belongs to its graph")
            }) {
                if let AnalysisOperationKind::ScopeExit { block: exited, .. } = operation.kind() {
                    for owner in
                        std::iter::successors(Some(boundary), |owner| graph.report_parent(*owner))
                    {
                        self.report_owner_inputs
                            .entry(owner.into())
                            .or_default()
                            .extend(reporting_scopes.get(&exited).into_iter().flatten().copied());
                    }
                }

                if let Some(bray_bound_tree::BoundExpression::Await(awaited)) = operation
                    .kind()
                    .occurrence()
                    .expression()
                    .and_then(|expression| request.view().expression(expression))
                {
                    for owner in
                        std::iter::successors(Some(boundary), |owner| graph.report_parent(*owner))
                    {
                        self.value_inputs.retain_input(owner, awaited.operand());
                    }
                }

                let (invocation, phase) = match operation.kind() {
                    AnalysisOperationKind::Call { invocation, phase } => (invocation, phase),
                    AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(
                        expression,
                    )) if dependencies.calls().contains_key(&expression.into()) => {
                        (expression.into(), AnalysisCallPhase::Completion)
                    }
                    _ => continue,
                };

                let Some(call) = dependencies.calls().get(&invocation) else {
                    continue;
                };

                if phase == AnalysisCallPhase::Input { continue; }
                if phase == AnalysisCallPhase::Attempt {
                    if let Some(contract) = call.report(CallReportOutput::Abnormal) {
                        self.owner_dependencies
                            .entry(boundary.into())
                            .or_default()
                            .extend(super::subjects::dependency_subjects(
                                contract.requirements(),
                                storage,
                            ));
                    }

                    continue;
                }

                let mut reports = call
                    .report(CallReportOutput::Completed)
                    .into_iter()
                    .collect::<Vec<_>>();

                if let (BoundExecutionSite::ScopedExit(expression), Some(exit)) =
                    (invocation, operation.scoped_exit())
                    && let Some(SemanticSelection::ScopedUse(scoped)) =
                        selections.expression(expression)
                    && crate::representation::type_representation(request, scoped.exit().1.result())
                        == Some(bray_compiler_known::RepresentationRole::Result)
                    && !bray_bound_tree::scoped_exit_completes_result(
                        request.view(),
                        expression,
                        Some(exit.exit()),
                    )
                {
                    reports.push(call.result());
                }

                let subjects = reports
                    .into_iter()
                    .flat_map(|contract| {
                        super::subjects::dependency_subjects(contract.requirements(), storage)
                    })
                    .collect::<BTreeSet<_>>();

                // Normal incidents can pass through successful nested catches before a later panic.
                for owner in
                    std::iter::successors(Some(boundary), |owner| graph.report_parent(*owner))
                {
                    self.owner_dependencies
                        .entry(owner.into())
                        .or_default()
                        .extend(subjects.iter().copied());
                }
            }
        }
    }

    pub(super) fn retain_owned_call_dependencies<C>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) where
        C: CheckerRequestContext + ?Sized,
    {
        let accesses_by_root = index_storage_roots(storage);

        let borrowed_reads = storage
            .access_plans()
            .iter()
            .filter_map(|plan| {
                reaches_borrowed_storage(request.semantic_values(), storage, plan.access())
                    .then_some(plan.occurrence())
            })
            .collect::<BTreeSet<_>>();

        let mut initializations = value_transfer_bindings(request, storage);

        for (identity, provenance) in storage.identity_entries() {
            let occurrence = match provenance {
                bray_bound_tree::StorageIdentity::InvocationResult(occurrence) => occurrence,
                bray_bound_tree::StorageIdentity::InvocationInput { expression, ordinal } => {
                    bray_bound_tree::BoundExecutionSite::CallInput { expression, ordinal }
                }
                bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. } => {
                    bray_bound_tree::BoundExecutionSite::ScopedEnter(expression)
                }
                bray_bound_tree::StorageIdentity::Temporary(expression)
                    if !self.value_inputs.is_independent(expression) =>
                {
                    expression.into()
                }
                _ => continue,
            };

            initializations
                .entry(occurrence)
                .or_default()
                .push(StorageBinding::Identity(identity));
        }

        let value_sources_by_root =
            index_value_sources_by_root(storage, &initializations, &self.value_inputs);

        let owners = self.owner_dependencies.keys().copied().collect::<Vec<_>>();

        // Keep seed contracts immutable while local ownership propagation reaches its fixed point.
        let mut retained_by_expression = self.owner_dependencies.clone();

        loop {
            let mut changed = false;

            for owner in &owners {
                let nested = retained_storage_borrows(
                    storage,
                    &value_sources_by_root,
                    retained_by_expression
                        .get(owner)
                        .into_iter()
                        .flat_map(|subjects| subjects.iter().copied()),
                    &self.value_inputs,
                    &retained_by_expression,
                );

                let output = retained_by_expression.entry(*owner).or_default();
                let previous = output.len();

                output.extend(nested);
                changed |= output.len() != previous;
            }

            for (owner, roots) in &self.report_owner_inputs {
                let retained = roots
                    .iter()
                    .flat_map(|root| value_sources_by_root.get(root).into_iter().flatten())
                    .flat_map(|source| {
                        retained_subtree_subjects(
                            &self.value_inputs,
                            *source,
                            &retained_by_expression,
                        )
                    })
                    .collect::<BTreeSet<_>>();

                let output = retained_by_expression.entry(*owner).or_default();
                let previous = output.len();

                output.extend(retained);
                changed |= output.len() != previous;
            }

            for (initializer, bindings) in &initializations {
                let retained = retained_subtree_subjects(
                    &self.value_inputs,
                    *initializer,
                    &retained_by_expression,
                );

                if retained.is_empty() {
                    continue;
                }

                let initializer_retention = retained_by_expression.entry(*initializer).or_default();
                let previous = initializer_retention.len();

                initializer_retention.extend(retained.iter().copied());
                changed |= initializer_retention.len() != previous;

                for binding in bindings {
                    let root = match binding {
                        StorageBinding::Identity(identity) => Some(*identity),
                        StorageBinding::Access(access) => storage.root_identity(*access),
                    };

                    let Some(root) = root else {
                        continue;
                    };

                    for occurrence in accesses_by_root.get(&root).into_iter().flatten() {
                        let expression = occurrence.expression();

                        // Dereferencing needs the pointer's loans during evaluation even when
                        // its copied result carries none. An unrelated owned field does not.
                        if borrowed_reads.contains(occurrence)
                            || matches!(
                                occurrence,
                                bray_bound_tree::BoundExecutionSite::ScopedExit(_)
                            )
                        {
                            self.extend_uses(*occurrence, retained.iter().copied());
                        }

                        if matches!(occurrence, bray_bound_tree::BoundExecutionSite::Node(_))
                            && expression.is_some_and(|expression| {
                                self.value_inputs.is_independent(expression)
                                    || self
                                        .value_inputs
                                        .projected_initializer(expression)
                                        .is_some()
                            })
                        {
                            continue;
                        }

                        let expression_retention =
                            retained_by_expression.entry(*occurrence).or_default();

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

        for (occurrence, retained) in &retained_by_expression {
            let defined = self
                .by_occurrence
                .get(occurrence)
                .map(|effect| &effect.definitions);

            let subjects = retained
                .iter()
                .copied()
                .filter(|subject| !defined.is_some_and(|definitions| definitions.contains(subject)))
                .collect::<Vec<_>>();

            self.extend_uses(*occurrence, subjects);
        }

        self.owner_dependencies = retained_by_expression;
    }
}

fn reaches_borrowed_storage(
    values: &bray_symbols::SemanticValueStore,
    storage: &StoragePlan,
    access: bray_bound_tree::StorageAccessId,
) -> bool {
    let Some(root) = storage.root_identity(access) else {
        return true;
    };

    let Some(path) = storage.resolved_projections(access) else {
        return true;
    };

    (0..path.len()).any(|length| {
        let ty = if length == 0 {
            storage.identity_type(root)
        } else {
            storage
                .access_at(root, &path[..length])
                .and_then(|access| storage.access(access))
                .map(|access| access.reached_type())
        };

        ty.is_none_or(|ty| {
            matches!(
                values.type_data(ty).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            )
        })
    })
}

fn index_value_sources_by_root(
    storage: &StoragePlan,
    initializations: &BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageBinding>>,
    inputs: &ValueInputs,
) -> BTreeMap<StorageIdentityId, Vec<bray_bound_tree::BoundExecutionSite>> {
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

        if matches!(storage.identity(root),
            Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }
                | bray_bound_tree::StorageIdentity::InvocationInput { expression, .. })
                if expression == plan.expression())
            || matches!(storage.identity(root),
                Some(bray_bound_tree::StorageIdentity::InvocationResult(site))
                    if site.expression() == Some(plan.expression()))
        {
            continue;
        }

        result
            .entry(root)
            .or_default()
            .extend(
                inputs
                    .projected_operands(plan.expression())
                    .map(|(value, path)| {
                        bray_bound_tree::BoundExecutionSite::from(inputs.project(value, &path).0)
                    }),
            );
    }

    result
}

fn retained_storage_borrows(
    storage: &StoragePlan,
    value_sources_by_root: &BTreeMap<StorageIdentityId, Vec<bray_bound_tree::BoundExecutionSite>>,
    subjects: impl IntoIterator<Item = BoundDependencySubject>,
    inputs: &ValueInputs,
    retained: &BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<BoundDependencySubject>>,
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
                    pending.extend(retained_subtree_subjects(inputs, *initializer, retained));
                }

                if let Some(bray_bound_tree::StorageIdentity::Temporary(expression)) =
                    storage.identity(root)
                {
                    pending.extend(retained_subtree_subjects(inputs, expression, retained));
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

pub(super) fn retained_subtree_subjects(
    inputs: &ValueInputs,
    root: impl Into<bray_bound_tree::BoundExecutionSite>,
    retained_by_expression: &BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        BTreeSet<BoundDependencySubject>,
    >,
) -> BTreeSet<BoundDependencySubject> {
    let mut retained = BTreeSet::new();
    let mut pending: Vec<bray_bound_tree::BoundExecutionSite> = vec![root.into()];
    let mut visited = BTreeSet::new();

    while let Some(occurrence) = pending.pop() {
        if !visited.insert(occurrence) {
            continue;
        }

        let expression = match occurrence {
            bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ) => Some(expression),
            _ => None,
        };

        if expression.is_some_and(|expression| inputs.is_independent(expression)) {
            continue;
        }

        if let Some(subjects) = retained_by_expression.get(&occurrence) {
            retained.extend(subjects.iter().copied());
        }

        let Some(expression) = expression else {
            continue;
        };

        pending.extend(
            inputs
                .operands(expression)
                .map(bray_bound_tree::BoundExecutionSite::from),
        );

        pending.extend(inputs.projected_operands(expression).map(|(value, path)| {
            bray_bound_tree::BoundExecutionSite::from(inputs.project(value, &path).0)
        }));
    }

    retained
}
