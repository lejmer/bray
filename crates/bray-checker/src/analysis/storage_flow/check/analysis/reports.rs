use std::collections::BTreeSet;
use std::sync::Arc;

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use bray_bound_tree::ReportDependencies;
use crate::analysis::liveness::dependency_subjects;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::analysis::storage_flow::model::{StorageFlowInput, StorageFlowState};
use bray_bound_tree::{
    BoundDependencyContract, BoundDependencySubject, BoundExecutionSite, CallReportOutput,
    SemanticSelection,
};

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_report_dependencies<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        effects: &crate::analysis::liveness::OperationEffects,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
    ) {
        let subjects = dependencies
            .calls()
            .iter()
            .flat_map(|(occurrence, call)| {
                call.report_contracts()
                    .chain(
                        matches!(occurrence, BoundExecutionSite::ScopedExit(_))
                            .then_some(call.result()),
                    )
            })
            .flat_map(|contract| {
                dependency_subjects(request, contract.requirements(), storage, effects.value_inputs(), selections)
                    .into_iter()
                    .chain(contract.required_subjects())
            })
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
                contract.captured_values().iter().filter_map(|outcome| outcome.report.as_deref())
                    .chain(contract.failure_outcomes().into_iter().flatten().filter_map(|row| row.report.as_deref()))
                    .any(|requirements| !requirements.is_empty())
            })
        {
            roots.extend(
                cleanup
                    .requirements
                    .entries()
                    .iter()
                    .map(|requirement| requirement.identity()),
            );
        }

        roots.retain(|root| !matches!(storage.identity(*root), Some(bray_bound_tree::StorageIdentity::InvocationInput { .. })));

        if roots.is_empty() { return; }

        for plan in storage.access_plans() {
            if let Some(root) = storage.root_identity(plan.access())
                && roots.contains(&root)
            {
                let mut retained = effects.retained_by(plan.occurrence()).peekable();

                if retained.peek().is_some() {
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
    pub(super) fn collect_lifecycle_reports(
        &mut self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
        normal: Option<usize>,
        failure: Option<(bray_symbols::DependencyFailureKind, Option<usize>)>,
    ) {
        let request = self.request;

        let mut emit = |requirements: &[bray_symbols::DependencyRequirement], pending| {
            if let Some(report) = self.cleanup_report_snapshot(state, call.access, call.path, requirements) {
                state.retain_reports(self.report_boundary, report, pending);
            }
        };

        if let Some((kind, row)) = failure {
            let reports = call.contract.failure_outcomes().into_iter().flatten().enumerate()
                .filter(|(index, outcome)| outcome.covers(kind)
                    && row.map_or_else(|| call.entry.admits_failure(*index), |selected| selected == *index))
                .filter_map(|(_, outcome)| outcome.report.as_deref());

            let mut reports = reports.peekable();

            if reports.peek().is_some() {
                emit(&reports.flatten().cloned().collect::<Vec<_>>(), false);
            }

            return;
        }

        if let Some(requirements) = bray_symbols::CapturedValueOutcome::possible_report(call.normal_outcomes(normal)) {
            emit(&requirements, operation.is_normal_cleanup());
        }

        if !operation.is_normal_cleanup()
            && call
                .lifecycle
                .call
                .callable
                .definition()
                .callable_symbol()
                .kind()
                == bray_symbols::SymbolKind::Finalizer
                && crate::representation::type_representation(
                    request,
                    call.lifecycle.call.result.completion_type(),
                ) == Some(bray_compiler_known::RepresentationRole::Result)
        {
            let representation = request.available_compiler_known_symbols().result_representation()
                .expect("a fallible finalizer retains its checked Result representation");

            let returned = normal.and_then(|row| call.contract.captured_values()[row].result_variant());

            if returned != Some(representation.success_variant()) {
                emit(call.contract.requirements(), false);
            }
        }
    }

    pub(super) fn inactive_cleanup_report(
        &self,
        operation: AnalysisOperationKind,
        ty: bray_symbols::TypeId,
        captures: Option<&[bray_symbols::TypeId]>,
        inherited_tail: bool,
    ) -> Option<(Vec<bray_symbols::DependencyRequirement>, bool)> {
        if crate::representation::type_representation(self.request, ty)
            != Some(bray_compiler_known::RepresentationRole::Future)
            || matches!(
                operation,
                AnalysisOperationKind::ScopeExit {
                    phase:
                        crate::analysis::model::AnalysisScopeExitPhase::TaskCancellationBroadcast,
                    ..
                }
            )
        {
            return None;
        }

        let dependencies = bray_symbols::DependencyRequirement::direct(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Receiver),
            bray_symbols::DependencyRequirementKind::ValueDependencies,
        );

        let requirements = match captures {
            Some(captures) => captures
                .iter()
                .map(|capture| {
                    // Each capture shares the same immutable owner-dependency path.
                    bray_symbols::DependencyRequirement::cleanup_dependencies(
                        *capture,
                        [dependencies.clone()],
                    )
                })
                .collect(),
            None => vec![dependencies],
        };

        // A receiver tail retains the callable's report dependencies, but owns
        // collection after the source body hands off rather than within that body.
        Some((
            requirements,
            operation.is_normal_cleanup() && !inherited_tail,
        ))
    }

    pub(super) fn retain_operation_reports(
        &mut self,
        state: &mut StorageFlowState,
        operation: AnalysisOperation,
        execution: &crate::analysis::guarantee::ExecutionState,
        task_outcome: Option<bray_bound_tree::StorageTaskOutcome>,
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

        let acquired = invocation.expression().and_then(|expression| {
            match task_outcome {
                Some(bray_bound_tree::StorageTaskOutcome::Abnormal { kind: bray_symbols::DependencyFailureKind::Panic, .. }) => None,
                Some(outcome) => self.execution.observed_task_reports(execution, expression, outcome),
                None => self.execution.awaited_report_dependencies(execution, expression, CallReportOutput::Completed, None),
            }
        });

        if let Some(dependencies) = acquired {
            state.result_dependencies.prepare_reports(invocation, CallReportOutput::Completed, None, dependencies);
        }

        let outputs = execution.admitted_call(invocation).map(|entry| &entry.outputs);

        self.retain_selected_reports(state, invocation, CallReportOutput::Completed, pending,
            |row| outputs.is_none_or(|outputs| outputs.is_admitted(row)));
    }

    pub(super) fn retain_scoped_result_incident(
        &mut self,
        state: &mut StorageFlowState,
        transfer: crate::analysis::model::ScopedResultTransfer,
        pending: bool,
    ) {
        let contract = self.dependencies.calls()[&transfer.invocation].result();
        let requirements = state.result_dependencies.take_value(transfer.invocation);
        let mut dependencies = self.bind_report_dependencies(contract, requirements);

        if let Some(source) = transfer.source {
            let ty = self.storage.identity_type(source).expect("scoped exit retains its result type");

            for (slot, target) in state.memory.reference_targets(source) {
                if slot.projections.first() != Some(&transfer.projection) { continue; }

                let (origins, _) = state.memory.effect_origins(target);

                for target in origins {
                    match self.retained_borrow_access(slot, &target.origin, ty) {
                        Ok(Some(access)) => {
                            dependencies.storage.insert(access);
                            dependencies.subjects.insert(BoundDependencySubject::StorageAccess(access));
                            dependencies.subjects.extend(self.storage.root_identity(access).map(BoundDependencySubject::Storage));
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.fail_query(error);
                            return;
                        }
                    }
                }
            }
        }

        state.retain_reports(self.report_boundary, Arc::new(dependencies), pending);
    }

    pub(in crate::analysis::storage_flow) fn propagate_run_report(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
            self.request.view().expression(expression)
        else {
            unreachable!("run propagation retains its checked expression");
        };

        let Some(SemanticSelection::Propagation(bray_bound_tree::SelectedPropagation::CurrentRun {
            panicked: (variant, field), ..
        })) = self.execution.semantics.selections().expression(expression)
        else {
            unreachable!("run propagation retains the checked panic payload");
        };

        let operand = *operation.operands().first().expect("run propagation retains its evaluated operand");

        let projection = bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
            variant: *variant,
            field: *field,
        };

        let actual = match self.expression_reference(state, operand) {
            Some(crate::analysis::storage_flow::memory::Source::Value(value)) => {
                state.memory.report_dependencies(&value.project(&[projection]))
                    .map(|(dependencies, complete)| (Arc::clone(dependencies), complete))
            }
            _ => None,
        };

        let requirements = state.result_dependencies.take_value(operand.into());

        let dependencies = match actual {
            Some((dependencies, true)) => dependencies,
            possible => {
                let mut dependencies = Arc::new(ReportDependencies {
                    subjects: self.liveness.retained_by(operand).collect(),
                    requirements,
                    ..Default::default()
                });

                if let Some((possible, _)) = possible {
                    dependencies.merge(&possible);
                }

                dependencies
            }
        };

        state.retain_reports(self.report_boundary, dependencies, false);
    }

    pub(in crate::analysis::storage_flow) fn publish_caught_value(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        outcome: crate::analysis::model::AnalysisCaughtResult,
        report: Option<Arc<ReportDependencies>>,
    ) {
        use crate::analysis::storage_flow::memory::Target;
        use crate::analysis::model::AnalysisCaughtResult;
        use bray_bound_tree::StorageProjection::ActiveUnionPayloadField;

        if let Some(report) = &report {
            state.result_dependencies.publish_catch_value(expression, report.requirements.clone());
        }

        let Some(place) = self.expression_result_storage(expression) else { return; };

        let representation = self.request.available_compiler_known_symbols().result_representation()
            .expect("catch retains its checked Result representation");

        let success = ActiveUnionPayloadField { variant: representation.success_variant(), field: representation.success_field() };
        let error = ActiveUnionPayloadField { variant: representation.error_variant(), field: representation.error_field() };

        state.initialize_path(&place);
        state.memory.clear(&place);

        let root = Target::storage(place);
        let invocation = expression.into();

        let absent = match outcome {
            AnalysisCaughtResult::Completed(_) => error,
            AnalysisCaughtResult::Panicked => success,
        };

        state.memory.stage_no_payload_alternative(invocation, root.project(&[absent]));

        match outcome {
            AnalysisCaughtResult::Completed(Some(operand)) => {
                let requirements = state.result_dependencies.take_value(operand.into());

                state.result_dependencies.publish_catch_value(expression, requirements);

                if let Some(source) = self.expression_reference(state, operand) {
                    source.stage(&mut state.memory, invocation, root.project(&[success]));
                }
            }
            AnalysisCaughtResult::Panicked => {
                if let Some(report) = report {
                    let payload = root.project(&[error]);

                    state.memory.stage_report(invocation, root, payload, report, true);
                }
            }
            AnalysisCaughtResult::Completed(None) => {}
        }

        state.memory.complete_transition(invocation);
    }

    pub(super) fn retain_failed_invocation_report(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        kind: bray_symbols::DependencyFailureKind,
        accepts: impl Fn(usize) -> bool,
    ) {
        self.retain_selected_reports(state, occurrence, CallReportOutput::Abnormal(Some(kind)), false, accepts);
    }

    fn retain_selected_reports(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        output: CallReportOutput,
        pending: bool,
        accepts: impl Fn(usize) -> bool,
    ) {
        let candidates = state.result_dependencies.take_report_candidates(occurrence, output);

        let Some(dependencies) = candidates.admitted(accepts) else { return; };

        // Scope checking and exported summaries consume the same formation snapshot.
        state.retain_reports(self.report_boundary, dependencies, pending);

        if !output.is_abnormal() {
            self.reporting_completions.insert(occurrence);
        }

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

        let Some(bray_bound_tree::BoundExpression::Await(_)) =
            self.request.view().expression(expression)
        else {
            panic!("await suspension retains its consumed Future")
        };

        for kind in bray_symbols::DependencyFailureKind::ALL {
            let output = CallReportOutput::Abnormal(Some(kind));

            for row in self.execution.invocation_failure_rows(execution, occurrence, kind) {
                if let Some(dependencies) = self.execution.awaited_report_dependencies(execution, expression, output, row) {
                    state.result_dependencies.prepare_reports(occurrence, output, row, dependencies);
                }
            }
        }
    }

    pub(super) fn deferred_report_dependencies(
        &self,
        transfer: &crate::dependency::ResultTransfer<'_, '_, C>,
        expression: bray_bound_tree::BoundExpressionId,
        source: Option<(BoundExecutionSite, &bray_bound_tree::BoundCallReport)>,
    ) -> Result<bray_bound_tree::ReportDependencies, crate::CheckerQueryError<C::UpstreamError>> {
        let Some((source, report)) = source else {
            return Ok(bray_bound_tree::ReportDependencies {
                subjects: self.liveness.retained_by(expression).collect(),
                requirements: transfer.operand_dependencies(expression).cloned().collect(),
                ..Default::default()
            });
        };

        self.report_snapshot(transfer, source, report)
    }

    fn report_snapshot(
        &self,
        transfer: &crate::dependency::ResultTransfer<'_, '_, C>,
        occurrence: BoundExecutionSite,
        report: &bray_bound_tree::BoundCallReport,
    ) -> Result<bray_bound_tree::ReportDependencies, crate::CheckerQueryError<C::UpstreamError>> {
        let requirements = transfer.report_values(occurrence, report.requirements())?;

        Ok(self.bind_report_dependencies(report.storage(), requirements))
    }

    pub(super) fn stage_call_report_payloads(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        call: &bray_bound_tree::BoundCallDependencyContracts,
        admitted: &[(usize, &bray_symbols::CapturedValueOutcome)],
    ) {
        use crate::analysis::storage_flow::memory::Target;
        use bray_symbols::DependencySubject;

        if admitted.iter().all(|(_, outcome)| outcome.report_payloads.is_empty()) {
            return;
        }

        let Some(root) = self.invocation_result(occurrence).map(Target::storage) else { return; };

        let reports = call.reports();

        let transfer = crate::dependency::ResultTransfer::new(
            self.request,
            self.execution.semantics.types(),
            self.execution.semantics.selections(),
            self.patterns,
            self.values,
            self.storage,
            &mut state.result_dependencies,
        );

        let mut supports = std::collections::BTreeMap::<_, Arc<ReportDependencies>>::new();

        for (_, destination, report) in reports.payloads().filter(|(index, _, _)| {
            admitted.binary_search_by_key(index, |(index, _)| *index).is_ok()
        }) {
            match self.report_snapshot(&transfer, occurrence, report) {
                Ok(snapshot) => {
                    supports.entry(destination).and_modify(|support| {
                        support.merge(&snapshot);
                    }).or_insert_with(|| Arc::new(snapshot));
                }
                Err(error) => {
                    self.fail_query(error);
                    return;
                }
            }
        }

        for (destination, support) in supports {
            let Some(path) = self.invocation_logical_path(occurrence, destination) else { continue; };

            let complete = admitted.iter().all(|(_, outcome)| {
                outcome.accounts_for_report_payload(destination, DependencySubject::contains)
            });

            state.memory.stage_report(occurrence, root.clone(), Target::storage(path), support, complete);
        }
    }

    fn prepare_call_reports(
        &mut self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(call) = self.dependencies.calls().get(&occurrence) else {
            return;
        };

        let reports = call.reports();

        let candidates = reports.completed().map(|(row, report)| (CallReportOutput::Completed, Some(row), report))
            .chain(reports.abnormal().map(|(row, kind, report)| (CallReportOutput::Abnormal(kind), Some(row), report)));

        for (output, row, report) in candidates {
            let transfer = crate::dependency::ResultTransfer::new(
                self.request,
                self.execution.semantics.types(),
                self.execution.semantics.selections(),
                self.patterns,
                self.values,
                self.storage,
                &mut state.result_dependencies,
            );

            match self.report_snapshot(&transfer, occurrence, report) {
                Ok(snapshot) => state.result_dependencies.prepare_reports(occurrence, output, row, std::sync::Arc::new(snapshot)),
                Err(error) => self.fail_query(error),
            }
        }
    }

    pub(super) fn cleanup_report_snapshot(
        &mut self,
        state: &StorageFlowState,
        access: bray_bound_tree::StorageAccessId,
        receiver: Option<&bray_bound_tree::StoragePath>,
        requirements: &[bray_symbols::DependencyRequirement],
    ) -> Option<Arc<ReportDependencies>> {
        if requirements.is_empty() {
            return Some(Arc::new(ReportDependencies::default()));
        }

        let contract = match crate::dependency::cleanup_report_contract(
            self.request,
            self.storage,
            access,
            receiver,
            requirements,
        ) {
            Ok(contract) => contract,
            Err(error) => {
                self.fail_query(error);

                return None;
            }
        };

        let root = self
            .storage
            .root_identity(access)
            .expect("cleanup report has its original receiver storage");

        let inputs = state.result_dependencies.owner_requirements(
            self.storage,
            root,
            self.formal_root(root),
        );

        let portable = crate::dependency::map_requirements::<std::convert::Infallible>(requirements, &mut |subject, kind, _| {
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

        Some(Arc::new(self.bind_report_dependencies(&contract, portable.into_iter().collect())))
    }

    fn bind_report_dependencies(
        &self,
        contract: &BoundDependencyContract,
        requirements: BTreeSet<bray_symbols::DependencyRequirement>,
    ) -> ReportDependencies {
        let mut dependencies = ReportDependencies { requirements, ..Default::default() };
        let subjects = &mut dependencies.subjects;
        let mut pending = contract.requirements().iter().collect::<Vec<_>>();

        while let Some(requirement) = pending.pop() {
            dependencies.storage.extend(requirement.retained_storage_access());

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
                        subjects.extend(self.report_value_subjects(root));
                    } else {
                        subjects.extend(dependency_subjects(
                            self.request,
                            std::slice::from_ref(requirement),
                            self.storage,
                            self.values,
                            self.execution.semantics.selections(),
                        ));
                    }
                }
                bray_bound_tree::BoundDependencyRequirement::Value { source, path, ty } => {
                    let borrow_free = self.values.value_carries_no_borrows(*ty);

                    if let Some(value) = self.values.copied_value_source(
                        self.request, self.storage,
                        self.execution.semantics.selections(), *source, path, *ty,
                    ) {
                        subjects.extend(self.liveness.retained_by(value)
                            .filter(|subject| !borrow_free || !subject.is_storage_dependency()));

                        continue;
                    }

                    if let Some(root) = self.storage.root_identity(*source) {
                        subjects.extend(self.report_value_subjects(root)
                            .filter(|subject| !borrow_free || !subject.is_storage_dependency()));
                    }
                }
                bray_bound_tree::BoundDependencyRequirement::Direct { .. } => {
                    subjects.extend(dependency_subjects(
                        self.request,
                        std::slice::from_ref(requirement),
                        self.storage,
                        self.values,
                        self.execution.semantics.selections(),
                    ));
                }
                bray_bound_tree::BoundDependencyRequirement::Guarded(guarded) => {
                    pending.extend(guarded.requirements())
                }
            }
        }

        dependencies
    }

    fn report_value_subjects(&self, root: bray_bound_tree::StorageIdentityId) -> impl Iterator<Item = BoundDependencySubject> + '_ {
        let input = match self.storage.identity(root) {
            Some(identity @ bray_bound_tree::StorageIdentity::InvocationInput { .. }) => identity.definition_occurrence(),
            _ => None,
        };

        // The argument's evaluated value carries its own loans. Later uses of
        // its slot include the whole invocation and can include sibling inputs.
        input.into_iter().flat_map(|site| self.liveness.retained_by(site))
            .chain(self.input.value_dependencies.get(&root).into_iter().flatten().copied())
    }
}
