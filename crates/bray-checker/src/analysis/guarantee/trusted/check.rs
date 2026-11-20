use std::collections::{BTreeMap, BTreeSet};

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::analysis::id::AnalysisBlockId;
use crate::{CheckerRequestContext, CheckerUnitView, ExecutionCondition};
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, CheckedExpressionSemantics};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExpressionCategory, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};

/// Call requirements and initialized storage are independent proof results.
pub(in crate::analysis) struct MemoryCallEvidence {
    pub(in crate::analysis) requirements: Result<(), crate::UnprovenExecutionCondition>,
    pub(in crate::analysis) initialized: bool,
    pub(in crate::analysis) empty_copy: bool,
}

type Failures<Key, Cause> = BTreeMap<Key, (Option<AnalysisBlockId>, Cause)>;

fn record_failure<Key: Ord, Cause>(
    failures: &mut Failures<Key, Cause>,
    block: Option<AnalysisBlockId>,
    key: Key,
    cause: impl FnOnce() -> Cause,
) {
    use std::collections::btree_map::Entry;

    match failures.entry(key) {
        Entry::Vacant(entry) => { entry.insert((block, cause())); }
        Entry::Occupied(mut entry) if block < entry.get().0 => { entry.insert((block, cause())); }
        Entry::Occupied(_) => {}
    }
}

#[derive(Default)]
pub(in crate::analysis) struct TrustedChecks {
    pub(in crate::analysis) block: Option<AnalysisBlockId>,
    failed: Failures<bray_bound_tree::BoundExecutionSite, crate::UnprovenExecutionCondition>,
    failed_execution: Failures<bray_bound_tree::BoundExecutionSite, crate::UnprovenExecutionCondition>,
    transfers: Failures<BoundExpressionId, super::transfers::WitnessTransferFailure>,
    pending_transfers: Vec<(
        bray_bound_tree::BoundExecutionSite,
        Option<ExecutionCondition>,
        super::transfers::WitnessTransferFailure,
    )>,
    cleanup_failures: Failures<(AnyBoundNodeId, bray_symbols::CallableInstanceData), crate::UnprovenExecutionCondition>,
    pub(super) guarantees: Vec<(ExecutionCondition, bray_source::SourceSpan)>,
    guarantee_failures: Vec<Option<crate::UnprovenExecutionCondition>>,
    pub(super) owned_entry_conditions: BTreeSet<(ExecutionCondition, bool)>,

}

impl TrustedChecks {
    pub(in crate::analysis) fn new<C: CheckerRequestContext + ?Sized>(
        domain: &ExecutionFlowDomain<'_, '_, C>,
    ) -> Self {
        let boundary = domain.entry_state();

        let guarantees = boundary
            .as_ref()
            .into_iter()
            .flat_map(|boundary| {
                domain
                    .request
                    .trusted_contracts()
                    .into_iter()
                    .flat_map(|contracts| &contracts.guarantees)
                    .map(|(condition, span)| {
                        let condition = condition.capture_entry(
                            &|place| {
                                place
                                    .value_in(&boundary.current)
                                    .unwrap_or_else(ExecutionCondition::unknown)
                            },
                            &boundary.trusted_assumptions,
                            &boundary.assumptions,
                            &mut { ExecutionCondition::WORK_LIMIT },
                        );

                        (condition, *span)
                    })
            })
            .collect::<Vec<_>>();

        Self {
            owned_entry_conditions: boundary.as_ref().into_iter().flat_map(|state| state.assumptions.iter()).filter(|(condition, _)| matches!(condition.kind(), crate::ExecutionConditionKind::Input(place) if place.is_callable_input())).cloned().collect(),
            guarantee_failures: vec![None; guarantees.len()],
            guarantees,
            ..Self::default()
        }
    }

    pub(in crate::analysis) fn before_operation<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        operation: crate::analysis::model::AnalysisOperationKind,
        current_access: &impl Fn(bray_bound_tree::StorageAccessPlan) -> Option<bray_bound_tree::StoragePath>,
    ) {
        super::transfers::check_witness_transfers(
            domain.request,
            domain.storage,
            domain,
            state,
            operation.storage_plans(domain.storage).filter(|plan|
                !domain.storage.access_is_produced_at(plan.access(), operation.occurrence())),
            current_access,
            |expression, failure| record_failure(&mut self.transfers, self.block, expression, || failure),
        );
    }

    pub(in crate::analysis) fn capture_returned_witness_transfers<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        operation: crate::analysis::model::AnalysisOperationKind,
    ) {
        self.pending_transfers.clear();

        super::transfers::collect_returned_witness_transfers(
            domain,
            state,
            operation,
            &mut self.pending_transfers,
        );
    }

    pub(in crate::analysis) fn cleanup(
        &mut self,
        node: AnyBoundNodeId,
        lifecycle: &super::super::Lifecycle,
        proof: &Result<(), crate::UnprovenExecutionCondition>,
    ) {
        if !lifecycle.contract.requirements.is_empty() && let Err(failure) = proof {
            record_failure(&mut self.cleanup_failures, self.block, (node, lifecycle.call.callable), || failure.clone());
        }
    }

    pub(in crate::analysis) fn after_operation<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        operation: crate::analysis::model::AnalysisOperationKind,
        evidence: Option<&Result<(), crate::UnprovenExecutionCondition>>,
        current_access: &impl Fn(bray_bound_tree::StorageAccessPlan) -> Option<bray_bound_tree::StoragePath>,
    ) {
        super::transfers::check_witness_transfers(
            domain.request,
            domain.storage,
            domain,
            state,
            operation.storage_plans(domain.storage).filter(|plan|
                domain.storage.access_is_produced_at(plan.access(), operation.occurrence())),
            current_access,
            |expression, failure| record_failure(&mut self.transfers, self.block, expression, || failure),
        );

        // Completion facts can prove that a potential extraction never occurred.
        let proof = state.ordinary_proof();

        for (invocation, condition, failure) in self.pending_transfers.drain(..) {
            let active = condition.is_none_or(|condition| {
                let condition = condition
                    .substitute(
                        &|place| ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())),
                        &domain.invocation_value(state, invocation),
                        &mut { ExecutionCondition::WORK_LIMIT },
                    );

                proof.prove_trusted(&condition) != Some(false)
            });

            if active {
                record_failure(&mut self.transfers, self.block,
                    invocation
                        .expression()
                        .expect("pending ownership transfer has its source expression"),
                    || failure,
                );
            }
        }

        let Some(expression) = operation.occurrence().expression() else {
            return;
        };

        if let Some(bray_bound_tree::BoundExpression::Structured(bound)) =
            domain.request.view().expression(expression)
            && bound.kind() == bray_bound_tree::BoundStructuredExpressionKind::RepeatedArray
            && let [value] = bound.runtime_operands()
            && state.owns_witness(&domain.value(state, *value))
        {
            let ty = domain
                .semantics
                .types()
                .expression(expression)
                .expect("checked repetition retains its type")
                .ty();

            let no_copies =
                crate::constant::known_array_length(domain.request.semantic_values(), ty)
                    .is_some_and(|count| count == 0);

            if !no_copies {
                record_failure(&mut self.transfers, self.block, expression, ||
                    super::transfers::copied_witness(state, domain.value(state, *value)));
            }
        }

        let invocation = operation.occurrence();

        if let Some(contract) = domain.request.trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
            && let Some(entry) = state.entries.get(&invocation)
            && let Err(failure) = entry.prove_requirements(&contract.execution_requirements)
        {
            record_failure(&mut self.failed_execution, self.block, invocation, || failure);
        }

        if let Some(Err(failure)) = evidence
            && !state.admitted_call(invocation).is_some_and(|entry| entry.trusted_boundary)
        {
            if matches!(invocation, bray_bound_tree::BoundExecutionSite::ScopedExit(_))
                && domain.trace_authority_site(bray_bound_tree::StorageMutation::Operation(invocation))
            {
                let debug = |value: &ExecutionCondition| format!("{:?}", value.kind()).chars().take(1800).collect::<String>();

                let arguments = state.admitted_call(invocation).into_iter().flat_map(|entry| entry.positional_arguments())
                    .map(|(ordinal, value)| (ordinal, debug(value))).collect::<Vec<_>>();

                eprintln!("scoped exit authority {invocation:?} block={:?}: required={} actual={} arguments={arguments:?}",
                    self.block, debug(failure.required()), debug(failure.actual()));
            }

            record_failure(&mut self.failed, self.block, invocation, || failure.clone());
        }
    }

    pub(in crate::analysis) fn exit<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        result: &ExecutionCondition,
        writes: &BTreeSet<bray_bound_tree::StoragePath>,
    ) -> Option<super::owned::NormalReturn> {
        super::super::postcondition::check_postconditions(
            domain,
            state,
            &self.guarantees,
            &mut self.guarantee_failures,
        );

        self.normal_return(domain, state, result, writes)
    }

    pub(in crate::analysis) fn failure_outcome<C: CheckerRequestContext + ?Sized>(
        &self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        kind: bray_symbols::DependencyFailureKind,
        writes: &BTreeSet<bray_bound_tree::StoragePath>,
    ) -> bray_symbols::CapturedFailureOutcome {
        let guards = super::captured::exit_guards(domain, state, &crate::ExecutionCondition::unknown(), &self.owned_entry_conditions);

        bray_symbols::CapturedFailureOutcome {
            kind: Some(kind),
            guard: super::captured::conjunction(domain.request.semantic_values(), guards),
            effects: None,
            cell_changes: None,
            writes: super::captured::captured_bindings(domain, state, None, writes).into_iter().collect(),
            report: None,
        }
    }

    pub(in crate::analysis) fn finish<C: CheckerRequestContext + ?Sized>(
        self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
    ) -> crate::CheckerQueryResult<DiagnosticBag, C::UpstreamError> {
        let mut diagnostics = DiagnosticBag::new();

        for ((_, span), failed) in self.guarantees.into_iter().zip(self.guarantee_failures) {
            let Some(failed) = failed else { continue; };

            diagnostics.add(super::diagnostic::with_obligation(
                domain, bray_diagnostics::DiagnosticContractObligationKind::Postcondition, &failed,
                Diagnostic::new(
                    bray_diagnostics::DiagnosticId::new(span.start().bytes()),
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                    SeverityKind::Error,
                )
                .with_primary_span(span)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::TrustedObligationFailure,
                    span,
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::TrustedObligationEvidenceRequired,
                )),
            )?);
        }

        for ((node, callable), (_, failure)) in self.cleanup_failures {
            diagnostics.add(super::diagnostic::failed_obligation(
                domain, node, DiagnosticKind::CheckingTrustedObligationNotProven,
                bray_diagnostics::DiagnosticContractObligationKind::CleanupRequirement,
                &failure, Some(callable),
            )?);
        }

        for (expression, (_, failure)) in self.transfers {
            diagnostics.add(contract_diagnostic(
                domain.request,
                domain.semantics,
                expression.into(),
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            )?.with_arg(DiagnosticArg::witness_transfer(super::super::diagnostic::witness_transfer(
                domain, failure.kind, &failure.carrier, &failure.guarantee,
            )?)));
        }

        for (invocation, (_, failure)) in self.failed_execution {
            diagnostics.add(super::diagnostic::failed_obligation(
                domain, invocation.node(), DiagnosticKind::CheckingExecutionRequirementNotProven,
                bray_diagnostics::DiagnosticContractObligationKind::ExecutionRequirement,
                &failure, domain.semantics.selections().invoked_callable(invocation),
            )?);
        }

        for (invocation, (_, failure)) in self.failed {
            let kind = if matches!(invocation, bray_bound_tree::BoundExecutionSite::ScopedExit(_)) {
                bray_diagnostics::DiagnosticContractObligationKind::CleanupRequirement
            } else {
                bray_diagnostics::DiagnosticContractObligationKind::CallRequirement
            };

            diagnostics.add(super::diagnostic::failed_obligation(
                domain, invocation.node(), DiagnosticKind::CheckingTrustedObligationNotProven,
                kind,
                &failure, domain.semantics.selections().invoked_callable(invocation),
            )?);
        }

        Ok(diagnostics)
    }

}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn admitted_pure(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: &crate::ExecutionCallEvidence,
    ) -> bool {
        let declared = match invocation {
            bray_bound_tree::BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                matches!(self.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Call(call)) if call.phase_behaviors().invocation()
                        .execution_properties().contains(&crate::ExecutionProperty::Pure))
            }
            _ => false,
        };

        let contract = self.request.trusted_contracts().and_then(|contracts| contracts.calls.get(&invocation));

        contract.map_or(declared, |contract| {
            (declared || contract.preserves_inputs)
                && entry.prove_requirements(contract.preconditions.iter().map(|(condition, _)| condition)).is_ok()
                && (entry.trusted_boundary || self.prove_trusted_requirements(entry, &contract.requirements).is_ok())
        })
    }

    pub(in crate::analysis) fn capture_entry_check(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<bool> {
        let contract = self.request.trusted_contracts()?.calls.get(&invocation)?;

        if !contract.checks_preconditions {
            return None;
        }

        let evidence = state
            .entries
            .get(&invocation)
            .expect("runtime preconditions retain the captured input evidence");

        let equalities = self.proofs.equalities(&evidence.assumptions, None);

        let conditions = contract.preconditions.iter()
            .filter(|(_, domain)| *domain == bray_symbols::PredicateDomain::Value)
            .map(|(condition, _)| {
                evidence.bind_condition(condition, &ExecutionCondition::unknown()).with_equalities(&equalities)
            }).collect();

        let guard = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, conditions);
        let proven = guard.prove_ordinary(&evidence.assumptions, Some(&self.proofs));

        state.entry_check = Some((invocation, proven.map_or(guard, ExecutionCondition::boolean)));

        Some(proven == Some(true))
    }

    pub(in crate::analysis::guarantee) fn refine_invocation_entry(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        admitted: bool,
    ) -> bool {
        let Some((site, condition)) = state.entry_check.take() else {
            // Some implicit CFG calls reserve an admission fork before evidence is available.
            return admitted || self.request.trusted_contracts().and_then(|contracts| contracts.calls.get(&invocation))
                .is_none_or(|contract| contract.checks_preconditions);
        };

        assert_eq!(site, invocation, "admission consumes its evaluated check");

        if !state.ordinary_proof().can_assume(&condition, admitted) {
            return false;
        }

        let evidence = state.admitted_call_mut(invocation).expect("guarded admission retains its evidence");

        condition.clone().assume(admitted, &mut evidence.assumptions);
        condition.assume(admitted, &mut state.assumptions);

        true
    }

    pub(in crate::analysis) fn trusted_memory_evidence(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<MemoryCallEvidence> {
        let contract = self.request.trusted_contracts()?.calls.get(&invocation)?;
        let evidence = state.entries.get(&invocation)?;
        let requirements = self.prove_trusted_requirements(evidence, &contract.requirements);
        let empty_copy = self.empty_memory_copy(state, invocation);

        let initialized = requirements.is_ok() || empty_copy || {
            // Failure of another requirement must not erase established initialization.
            // Resolve these identities only on the failing path.
            let provider = self.request.context().available_compiler_known_symbols().provider();

            let predicates = ["InitializedAs", "InitializedRangeAs"].map(|name| {
                provider.declaration_symbol::<bray_symbols::PredicateSymbolId>(
                    &bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                        .expect("closed initialization predicate key must be valid"),
                ).map(bray_symbols::PredicateDefinitionSymbolId::from)
            });

            // Conditions share their immutable operand trees with the call contract.
            let initialization = contract.requirements.iter().filter(|condition| {
                matches!(condition.kind(), crate::ExecutionConditionKind::Trusted(value)
                    if matches!(value.as_ref().kind(), crate::ExecutionConditionKind::Predicate(predicate, _, _)
                        if predicates.contains(&Some(*predicate))))
            }).cloned().collect::<Vec<_>>();

            !initialization.is_empty() && self.prove_trusted_requirements(evidence, &initialization).is_ok()
        };

        Some(MemoryCallEvidence { requirements, initialized, empty_copy })
    }
}

pub(super) fn contract_diagnostic<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    node: AnyBoundNodeId,
    kind: DiagnosticKind,
) -> crate::CheckerQueryResult<Diagnostic, C::UpstreamError> {
    let origin = crate::diagnostic::bound_node_origin(request, node)
        .expect("trusted requirement failure retains its source origin");

    let anchor = origin.source_anchor().syntax();
    let span = bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range());

    let category = if let AnyBoundNodeId::Expression(expression) = node {
        crate::diagnostic::expression_category(
            request
                .unit()
                .tree()
                .expression(expression)
                .expect("trusted call expression belongs to the checked unit"),
        )
    } else {
        DiagnosticExpressionCategory::ControlFlow
    };

    let note = if kind == DiagnosticKind::CheckingExecutionRequirementNotProven {
        DiagnosticNoteKind::ExecutionRequirementEvidenceRequired
    } else if kind == DiagnosticKind::CheckingTrustedWitnessTransferNotProven {
        DiagnosticNoteKind::TrustedWitnessTransferRequired
    } else {
        DiagnosticNoteKind::TrustedObligationEvidenceRequired
    };

    let mut diagnostic = Diagnostic::new(
        bray_diagnostics::DiagnosticId::new(span.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(DiagnosticArg::expression_category(category))
    .with_label(DiagnosticLabel::primary(
        if kind == DiagnosticKind::CheckingExecutionRequirementNotProven {
            DiagnosticLabelKind::ExecutionRequirementFailure
        } else {
            DiagnosticLabelKind::TrustedObligationFailure
        },
        span,
    ))
    .with_note(DiagnosticNote::new(note));

    if kind != DiagnosticKind::CheckingExecutionRequirementNotProven
        && let AnyBoundNodeId::Expression(expression) = node
        && let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            expressions.selections().expression(expression)
        && let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
        && let Some(name) = request.member_name(callable.definition().symbol())?
    {
        diagnostic = diagnostic.with_label(
            DiagnosticLabel::secondary(DiagnosticLabelKind::TrustedCallable, span)
                .with_arg(DiagnosticArg::declaration_name(name.as_str())),
        );
    }

    Ok(diagnostic)
}
