use std::collections::BTreeSet;

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::liveness::dependency_subjects;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::analysis::storage_flow::model::{StorageFlowInput, StorageFlowState};
use bray_bound_tree::{
    BoundDependencyContract, BoundDependencySubject, BoundExecutionSite, CallReportOutput,
    SemanticSelection,
};

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_report_dependencies(
        &mut self,
        storage: &bray_bound_tree::StoragePlan,
        liveness: &bray_bound_tree::Liveness,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) {
        let subjects = dependencies
            .calls()
            .iter()
            .flat_map(|(occurrence, call)| {
                call.reports()
                    .completed()
                    .into_iter()
                    .chain(call.reports().abnormal())
                    .chain(call.deferred_reports().into_iter().flat_map(|reports| {
                        reports.completed().into_iter().chain(reports.abnormal())
                    }))
                    .chain(
                        matches!(occurrence, BoundExecutionSite::ScopedExit(_))
                            .then_some(call.result()),
                    )
            })
            .flat_map(|contract| dependency_subjects(contract.requirements(), storage))
            .collect::<BTreeSet<_>>();

        let mut roots = subjects
            .iter()
            .filter_map(|subject| match subject {
                BoundDependencySubject::Storage(root) => Some(*root),
                BoundDependencySubject::StorageAccess(access) => storage.root_identity(*access),
                BoundDependencySubject::BorrowCapability(borrow) => storage
                    .borrow_capability(*borrow)
                    .and_then(|borrow| storage.root_identity(borrow.access())),
                _ => None,
            })
            .collect::<BTreeSet<_>>();

        if dependencies
            .lifecycle_dependencies()
            .values()
            .any(|contract| {
                contract
                    .completion_reports()
                    .into_iter()
                    .chain(contract.abnormal_reports())
                    .any(|requirements| !requirements.is_empty())
            })
        {
            roots.extend(
                cleanup
                    .requirements
                    .iter()
                    .map(|requirement| requirement.identity()),
            );
        }

        for plan in storage.access_plans() {
            if let Some(root) = storage.root_identity(plan.access())
                && roots.contains(&root)
            {
                let retained = liveness
                    .retained_by(plan.occurrence())
                    .collect::<BTreeSet<_>>();

                if !retained.is_empty() {
                    self.value_dependencies
                        .entry(root)
                        .or_default()
                        .extend(retained);
                }
            }
        }
    }
}

impl<'analysis, C: CheckerRequestContext + ?Sized> StorageFlowCollector<'analysis, C> {
    pub(super) fn inactive_cleanup_report(
        &self,
        operation: AnalysisOperationKind,
        mutation: &crate::analysis::guarantee::CleanupMutation<'_>,
        captures: Option<&[bray_symbols::TypeId]>,
        inherited_tail: bool,
    ) -> Option<(bray_bound_tree::StorageAccessId, Vec<bray_symbols::DependencyRequirement>, bool)> {
        let crate::analysis::guarantee::CleanupMutation::Opaque { ty, access, .. } = mutation else {
            return None;
        };

        if crate::representation::type_representation(self.request, *ty)
            != Some(bray_compiler_known::RepresentationRole::Future)
            || matches!(operation, AnalysisOperationKind::ScopeExit {
                phase: crate::analysis::model::AnalysisScopeExitPhase::TaskCancellationBroadcast, ..
            })
        {
            return None;
        }

        let dependencies = bray_symbols::DependencyRequirement::direct(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Receiver),
            bray_symbols::DependencyRequirementKind::ValueDependencies,
        );

        let requirements = match captures {
            Some(captures) => captures.iter().map(|capture| {
                // Each capture shares the same immutable owner-dependency path.
                bray_symbols::DependencyRequirement::cleanup_dependencies(*capture, [dependencies.clone()])
            }).collect(),
            None => vec![dependencies],
        };

        // A receiver tail retains the callable's report dependencies, but owns
        // collection after the source body hands off rather than within that body.
        Some((*access, requirements, operation.is_normal_cleanup() && !inherited_tail))
    }

    pub(super) fn retain_operation_reports(
        &mut self,
        state: &mut StorageFlowState,
        operation: AnalysisOperation,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) {
        if let AnalysisOperationKind::Suspension {
            occurrence,
            kind: crate::analysis::model::AnalysisSuspensionKind::Await,
        } = operation.kind()
        {
            self.prepare_await_reports(state, execution, occurrence);
            return;
        }

        let (invocation, phase) = match operation.kind() {
            AnalysisOperationKind::Call { invocation, phase } => (invocation, phase),
            AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(
                expression,
            )) if matches!(
                self.execution.semantics.selections().expression(expression),
                Some(SemanticSelection::Call(_))
            ) || self.dependencies.calls().contains_key(&expression.into()) =>
            {
                self.prepare_call_reports(state, expression.into());

                (expression.into(), AnalysisCallPhase::Completion)
            }
            AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(
                expression,
            )) if matches!(
                self.request.view().expression(expression),
                Some(bray_bound_tree::BoundExpression::Await(_))
            ) =>
            {
                (expression.into(), AnalysisCallPhase::Completion)
            }
            _ => return,
        };

        if phase == AnalysisCallPhase::Input {
            return;
        }

        if phase == AnalysisCallPhase::Attempt {
            self.prepare_call_reports(state, invocation);
            return;
        }

        let pending = !matches!(invocation, BoundExecutionSite::ScopedExit(_))
            || operation.is_normal_cleanup();

        self.retain_call_reports(state, invocation, CallReportOutput::Completed, pending);

        let BoundExecutionSite::ScopedExit(expression) = invocation else {
            return;
        };

        let exit = operation
            .scoped_exit()
            .expect("scoped completion retains its exact exit");

        let Some(SemanticSelection::ScopedUse(scoped)) =
            self.execution.semantics.selections().expression(expression)
        else {
            panic!("scoped completion retains its selected lifecycle");
        };

        if crate::representation::type_representation(self.request, scoped.exit().1.result())
            != Some(bray_compiler_known::RepresentationRole::Result)
            || bray_bound_tree::scoped_exit_completes_result(
                self.request.view(),
                expression,
                Some(exit.exit()),
            )
        {
            return;
        }

        let contract = self.dependencies.calls()[&invocation].result();

        self.retain_report_subjects(state, contract, pending);

        let transfer = crate::dependency::ResultTransfer::new(
            self.request,
            self.execution.semantics.types(),
            self.execution.semantics.selections(),
            self.patterns,
            self.values,
            &mut state.result_dependencies,
        );

        match transfer.exit_values(scoped) {
            Ok(requirements) => state
                .result_dependencies
                .retain_reports(self.report_boundary, requirements),
            Err(error) => self.query_failure = Some(error),
        }
    }

    pub(in crate::analysis::storage_flow) fn retain_call_reports(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        output: CallReportOutput,
        pending: bool,
    ) {
        if let Some(bray_bound_tree::BoundExpression::Await(awaited)) = occurrence
            .expression()
            .and_then(|expression| self.request.view().expression(expression))
        {
            if let Some(requirements) = state
                .result_dependencies
                .report_candidate(occurrence, output)
            {
                let subjects = if requirements.is_empty() {
                    BTreeSet::new()
                } else {
                    self.liveness.retained_by(awaited.operand()).collect()
                };

                state.retain_reports(self.report_boundary, subjects, pending);

                if !output.is_abnormal() {
                    self.reporting_completions.insert(occurrence);
                }

                state
                    .result_dependencies
                    .commit_reports(occurrence, output, self.report_boundary);
            } else {
                state
                    .result_dependencies
                    .discard_report_candidates(occurrence);
            }

            return;
        }

        let Some(call) = self.report_call_contract(occurrence) else {
            return;
        };

        let Some(contract) = call.report(output) else {
            state
                .result_dependencies
                .discard_report_candidates(occurrence);

            return;
        };

        if !output.is_abnormal() {
            self.reporting_completions.insert(occurrence);
        }

        self.retain_report_subjects(state, contract, pending);

        state
            .result_dependencies
            .commit_reports(occurrence, output, self.report_boundary);
    }

    fn prepare_await_reports(
        &mut self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: BoundExecutionSite,
    ) {
        let expression = occurrence
            .expression()
            .expect("await has its source expression");

        let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        else {
            panic!("await suspension retains its consumed Future")
        };

        let reports = self
            .execution
            .awaited_invocation(execution, expression)
            .and_then(|source| self.dependencies.calls().get(&source))
            .map(|call| {
                call.deferred_reports()
                    .expect("lazy call has its checked deferred outputs")
            });

        for (output, known) in [
            (
                CallReportOutput::Completed,
                reports.map(|reports| reports.completed()),
            ),
            (
                CallReportOutput::Abnormal,
                reports.map(|reports| reports.abnormal()),
            ),
        ] {
            if known == Some(None) {
                continue;
            }

            // Frame identity and entry snapshots travel with the consumed value. Several
            // frames may originate at one factory site, so that site cannot own this snapshot.
            let requirements = if known.is_some_and(|contract| {
                contract.is_some_and(|contract| contract.requirements().is_empty())
            }) {
                BTreeSet::new()
            } else {
                state
                    .result_dependencies
                    .value_requirements(awaited.operand())
                    .cloned()
                    .collect()
            };

            state
                .result_dependencies
                .prepare_reports(occurrence, output, requirements);
        }
    }

    fn prepare_call_reports(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(call) = self.report_call_contract(occurrence) else {
            return;
        };

        for (output, _) in call.report_outputs().filter(|(output, _)| {
            matches!(
                output,
                CallReportOutput::Completed | CallReportOutput::Abnormal
            )
        }) {
            let transfer = crate::dependency::ResultTransfer::new(
                self.request,
                self.execution.semantics.types(),
                self.execution.semantics.selections(),
                self.patterns,
                self.values,
                &mut state.result_dependencies,
            );

            match transfer.report_values(occurrence, output) {
                Ok(Some(requirements)) => {
                    state
                        .result_dependencies
                        .prepare_reports(occurrence, output, requirements)
                }
                Ok(None) => panic!("bound report output retains its portable dependency summary"),
                Err(error) => self.query_failure = Some(error),
            }
        }
    }

    fn report_call_contract(
        &self,
        occurrence: BoundExecutionSite,
    ) -> Option<&'analysis bray_bound_tree::BoundCallDependencyContracts> {
        match occurrence {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                let contract = self.dependencies.calls().get(&occurrence);

                if matches!(self.execution.semantics.selections().expression(expression), Some(SemanticSelection::Call(_))) {
                    return Some(contract.expect("selected source invocation retains its dependency contracts"));
                }

                // Call phases also delimit compiler-owned evaluation without a source invocation.
                contract
            }
            BoundExecutionSite::Iterate(_) | BoundExecutionSite::Next(_) | BoundExecutionSite::CallInput { .. } | BoundExecutionSite::ScopedEnter(_) | BoundExecutionSite::ScopedExit(_) => {
                Some(self.dependencies.calls().get(&occurrence).unwrap_or_else(|| {
                    panic!("selected scoped invocation retains its dependency contracts: {occurrence:?}")
                }))
            }
            // Replacement failure follows the committed source invocation. Its reports
            // come from the selected lifecycle transition, not the source call snapshot.
            BoundExecutionSite::Cleanup(_) => None,
            _ => panic!("call phase requires an expression invocation: {occurrence:?}"),
        }
    }

    pub(super) fn retain_cleanup_reports(
        &mut self,
        state: &mut StorageFlowState,
        access: bray_bound_tree::StorageAccessId,
        requirements: &[bray_symbols::DependencyRequirement],
        pending: bool,
    ) {
        let contract = match crate::dependency::cleanup_report_contract(
            self.request,
            self.storage,
            access,
            requirements,
        ) {
            Ok(contract) => contract,
            Err(error) => {
                self.query_failure = Some(error);
                return;
            }
        };

        self.retain_report_subjects(state, &contract, pending);

        let root = self
            .storage
            .root_identity(access)
            .expect("cleanup report has its original receiver storage");

        let inputs = state.result_dependencies.owner_requirements(
            self.storage,
            root,
            self.formal_root(root),
        );

        let portable = crate::dependency::map_requirements(&requirements, &mut |subject, kind| {
            if subject.subject_root() != bray_symbols::DependencySubjectRoot::Receiver {
                return Ok(vec![bray_symbols::DependencyRequirement::direct(
                    subject.clone(),
                    kind,
                )]);
            }

            if kind == bray_symbols::DependencyRequirementKind::ValueDependencies {
                return Ok(inputs.iter().cloned().collect());
            }

            Ok(vec![bray_symbols::DependencyRequirement::direct(
                bray_symbols::DependencySubject::root(
                    bray_symbols::DependencySubjectRoot::EvaluationStorage,
                ),
                kind,
            )])
        })
        .expect("cleanup report requirements preserve checked variable bindings");

        state
            .result_dependencies
            .retain_reports(self.report_boundary, portable);
    }

    fn retain_report_subjects(
        &self,
        state: &mut StorageFlowState,
        contract: &BoundDependencyContract,
        pending_report: bool,
    ) {
        let mut subjects = BTreeSet::new();
        let mut pending = contract.requirements().iter().collect::<Vec<_>>();

        while let Some(requirement) = pending.pop() {
            match requirement {
                bray_bound_tree::BoundDependencyRequirement::Direct {
                    subject,
                    kind: bray_bound_tree::BoundDependencyRequirementKind::ValueDependencies,
                } => {
                    let root = match subject {
                        BoundDependencySubject::Storage(root) => Some(*root),
                        BoundDependencySubject::StorageAccess(access) => {
                            self.storage.root_identity(*access)
                        }
                        _ => None,
                    };

                    if let Some(root) = root {
                        subjects.extend(
                            self.input
                                .value_dependencies
                                .get(&root)
                                .into_iter()
                                .flatten()
                                .copied(),
                        );
                    } else {
                        subjects.extend(dependency_subjects(
                            std::slice::from_ref(requirement),
                            self.storage,
                        ));
                    }
                }
                bray_bound_tree::BoundDependencyRequirement::Direct { .. } => {
                    subjects.extend(dependency_subjects(
                        std::slice::from_ref(requirement),
                        self.storage,
                    ));
                }
                bray_bound_tree::BoundDependencyRequirement::Guarded(guarded) => {
                    pending.extend(guarded.requirements())
                }
            }
        }

        state.retain_reports(self.report_boundary, subjects, pending_report);
    }
}
