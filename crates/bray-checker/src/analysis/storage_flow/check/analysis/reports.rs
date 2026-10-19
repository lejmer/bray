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

        let roots = subjects
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
    pub(super) fn retain_operation_reports(
        &mut self,
        state: &mut StorageFlowState,
        operation: AnalysisOperation,
    ) {
        let AnalysisOperationKind::Call { invocation, phase } = operation.kind() else {
            return;
        };

        if phase == AnalysisCallPhase::Attempt {
            self.prepare_call_reports(state, invocation);
            return;
        }

        self.retain_call_reports(state, invocation, CallReportOutput::Completed);

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

        self.retain_report_subjects(state, contract);

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
    ) {
        let Some(call) = self.report_call_contract(occurrence) else {
            return;
        };

        let Some(contract) = call.report(output) else {
            return;
        };

        self.retain_report_subjects(state, contract);

        state
            .result_dependencies
            .commit_reports(occurrence, output, self.report_boundary);
    }

    fn prepare_call_reports(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(call) = self.report_call_contract(occurrence) else {
            return;
        };

        for (output, _) in call.report_outputs() {
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
                if !matches!(
                    self.execution.semantics.selections().expression(expression),
                    Some(SemanticSelection::Call(_))
                ) {
                    // CFG call phases also delimit compiler-owned evaluation and with results.
                    // Their selected operations do not invoke a source callable here.
                    return None;
                }
            }
            BoundExecutionSite::ScopedEnter(_) | BoundExecutionSite::ScopedExit(_) => {}
            _ => panic!("call phase requires an expression invocation: {occurrence:?}"),
        }

        Some(
            self.dependencies
                .calls()
                .get(&occurrence)
                .unwrap_or_else(|| {
                    panic!("selected invocation retains its dependency contracts: {occurrence:?}")
                }),
        )
    }

    fn retain_report_subjects(
        &self,
        state: &mut StorageFlowState,
        contract: &BoundDependencyContract,
    ) {
        let mut subjects = dependency_subjects(contract.requirements(), self.storage);

        let roots = subjects
            .iter()
            .filter_map(|subject| match subject {
                BoundDependencySubject::Storage(root) => Some(*root),
                BoundDependencySubject::StorageAccess(access) => {
                    self.storage.root_identity(*access)
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();

        for root in roots {
            subjects.extend(
                self.input
                    .value_dependencies
                    .get(&root)
                    .into_iter()
                    .flatten(),
            );
        }

        state
            .reports
            .entry(self.report_boundary)
            .or_default()
            .extend(subjects);
    }
}
