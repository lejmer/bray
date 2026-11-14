use std::collections::{BTreeMap, BTreeSet};

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::analysis::fixed_point::FixedPointDomain;
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

#[derive(Default)]
pub(in crate::analysis) struct TrustedChecks {
    failed: BTreeMap<bray_bound_tree::BoundExecutionSite, crate::UnprovenExecutionCondition>,
    failed_execution: BTreeMap<bray_bound_tree::BoundExecutionSite, crate::UnprovenExecutionCondition>,
    transfers: BTreeMap<BoundExpressionId, super::transfers::WitnessTransferFailure>,
    pending_transfers: Vec<(
        bray_bound_tree::BoundExecutionSite,
        Option<ExecutionCondition>,
        super::transfers::WitnessTransferFailure,
    )>,
    cleanup_failures: BTreeMap<(AnyBoundNodeId, bray_symbols::CallableInstanceData), crate::UnprovenExecutionCondition>,
    pub(super) guarantees: Vec<(ExecutionCondition, bray_source::SourceSpan)>,
    guarantee_failures: Vec<Option<crate::UnprovenExecutionCondition>>,
    pub(super) owned_returns: Vec<super::owned::OwnedReturn>,
    pub(super) owned_entry_conditions: BTreeSet<(ExecutionCondition, bool)>,
    pub(in crate::analysis) observed_result: Option<Option<bray_symbols::ConstantTermId>>,
    pub(super) inferred: Option<
        BTreeSet<bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>>,
    >,
    pub(super) carriers: Vec<Option<BTreeSet<Vec<bray_bound_tree::StorageProjection>>>>,
}

impl TrustedChecks {
    pub(in crate::analysis) fn new<C: CheckerRequestContext + ?Sized>(
        domain: &ExecutionFlowDomain<'_, '_, C>,
    ) -> Self {
        let boundary = domain.boundary();

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
                                    .unwrap_or(ExecutionCondition::Unknown)
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
            owned_entry_conditions: boundary.as_ref().into_iter().flat_map(|state| state.assumptions.iter()).filter(|(condition, _)| matches!(condition, ExecutionCondition::Input(place) if place.is_callable_input())).cloned().collect(),
            guarantee_failures: vec![None; guarantees.len()],
            carriers: vec![None; guarantees.len()],
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
            &mut self.transfers,
        );

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
            self.cleanup_failures.entry((node, lifecycle.call.callable)).or_insert_with(|| failure.clone());
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
            &mut self.transfers,
        );

        // Completion facts can prove that a potential extraction never occurred.
        let proof = state.proofs.ordinary_proof(&state.assumptions);

        for (invocation, condition, failure) in self.pending_transfers.drain(..) {
            let active = condition.is_none_or(|condition| {
                let condition = condition
                    .substitute(
                        &|place| ExecutionCondition::Input(place.clone()),
                        &domain.invocation_value(state, invocation),
                        &mut { ExecutionCondition::WORK_LIMIT },
                    );

                proof.prove_trusted(&condition) != Some(false)
            });

            if active {
                self.transfers.entry(
                    invocation
                        .expression()
                        .expect("pending ownership transfer has its source expression"),
                ).or_insert(failure);
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

            let singleton =
                crate::constant::known_array_length(domain.request.semantic_values(), ty)
                    .is_some_and(|count| count <= 1);

            if !singleton {
                self.transfers.entry(expression).or_insert_with(||
                    super::transfers::copied_witness(state, domain.value(state, *value)));
            }
        }

        let invocation = operation.occurrence();

        if let Some(contract) = domain.request.trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
            && let Some(entry) = state.entries.get(&invocation)
            && let Err(failure) = entry.prove_requirements(&contract.execution_requirements)
        {
            self.failed_execution.entry(invocation).or_insert(failure);
        }

        if let Some(Err(failure)) = evidence {
            self.failed.entry(invocation).or_insert_with(|| failure.clone());
        }
    }

    pub(in crate::analysis) fn exit<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
    ) {
        super::super::postcondition::check_postconditions(
            domain,
            state,
            &self.guarantees,
            &mut self.guarantee_failures,
        );

        let result = match domain.request.unit().root() {
            bray_bound_tree::BoundUnitRoot::Expression(expression) => {
                domain.value(state, expression)
            }
            _ => domain.returned_value(state),
        };

        self.collect_carriers(domain.request, state, &result);

        if domain.request.unit().key().kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault {
            let observed = super::carriers::portable(domain.request, result.copied_value());

            match &mut self.observed_result {
                Some(previous) if *previous != observed => *previous = None,
                Some(_) => {}
                None => self.observed_result = Some(observed),
            }
        }

        self.collect_owned_return(domain, state, &result);
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

        for ((node, callable), failure) in self.cleanup_failures {
            diagnostics.add(super::diagnostic::failed_obligation(
                domain, node, DiagnosticKind::CheckingTrustedObligationNotProven,
                bray_diagnostics::DiagnosticContractObligationKind::CleanupRequirement,
                &failure, Some(callable),
            )?);
        }

        for (expression, failure) in self.transfers {
            diagnostics.add(contract_diagnostic(
                domain.request,
                domain.semantics,
                expression.into(),
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            )?.with_arg(DiagnosticArg::witness_transfer(super::super::diagnostic::witness_transfer(
                domain, failure.kind, &failure.carrier, &failure.guarantee,
            )?)));
        }

        for (invocation, failure) in self.failed_execution {
            diagnostics.add(super::diagnostic::failed_obligation(
                domain, invocation.node(), DiagnosticKind::CheckingExecutionRequirementNotProven,
                bray_diagnostics::DiagnosticContractObligationKind::ExecutionRequirement,
                &failure, domain.semantics.selections().invoked_callable(invocation),
            )?);
        }

        for (invocation, failure) in self.failed {
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
    pub(in crate::analysis) fn preconditions_proven(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<bool> {
        let contract = self.request.trusted_contracts()?.calls.get(&invocation)?;

        if contract.preconditions.is_empty() {
            return None;
        }

        let evidence = state
            .entries
            .get(&invocation)
            .expect("invocation attempt retains its entry evidence");

        Some(
            evidence.prove_requirements(
                contract
                    .preconditions
                    .iter()
                    .map(|(condition, _)| condition),
            ).is_ok(),
        )
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
                matches!(condition, ExecutionCondition::Trusted(value)
                    if matches!(value.as_ref(), ExecutionCondition::Predicate(predicate, _, _)
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
