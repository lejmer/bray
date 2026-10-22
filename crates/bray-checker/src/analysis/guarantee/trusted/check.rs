use std::collections::BTreeSet;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::analysis::fixed_point::FixedPointDomain;
use crate::{CheckerRequestContext, CheckerUnitView, ExecutionCondition};
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, CheckedExpressionSemantics};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExpressionCategory, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};

#[derive(Default)]
pub(in crate::analysis) struct TrustedChecks {
    failed: BTreeSet<BoundExpressionId>,
    failed_execution: BTreeSet<BoundExpressionId>,
    transfers: BTreeSet<BoundExpressionId>,
    cleanup_failures: BTreeSet<AnyBoundNodeId>,
    pub(super) guarantees: Vec<(ExecutionCondition, bray_source::SourceSpan)>,
    guarantee_failures: Vec<bool>,
    pub(in crate::analysis) returned_input: Option<Option<bray_symbols::ConstantTermId>>,
    pub(in crate::analysis) observed_result: Option<Option<bray_symbols::ConstantTermId>>,
    pub(super) inferred: Option<BTreeSet<bray_symbols::DependencyWitnessCarrier>>,
    pub(super) carriers: Vec<Option<BTreeSet<Vec<bray_symbols::DependencyProjection>>>>,
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
                                    .unwrap_or_else(|| ExecutionCondition::Input(place.clone()))
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
            guarantee_failures: vec![false; guarantees.len()],
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
    ) {
        super::transfers::check_witness_transfers(
            domain.request,
            domain.storage,
            domain,
            state,
            operation.occurrence(),
            &mut self.transfers,
        );
    }

    pub(in crate::analysis) fn cleanup(
        &mut self,
        node: AnyBoundNodeId,
        established: bool,
        required: bool,
    ) {
        if !established && required {
            self.cleanup_failures.insert(node);
        }
    }

    pub(in crate::analysis) fn after_operation<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        operation: crate::analysis::model::AnalysisOperationKind,
        evidence: Option<bool>,
    ) {
        let Some(expression) = operation.occurrence().expression() else {
            return;
        };

        if let Some(bray_bound_tree::BoundExpression::Structured(bound)) =
            domain.request.view().expression(expression)
            && bound.kind() == bray_bound_tree::BoundStructuredExpressionKind::RepeatedArray
            && let [value, count] = bound.operands()
            && state.is_witness(&domain.value(state, *value))
        {
            let count = domain.value(state, *count);

            let singleton = matches!(count, ExecutionCondition::Literal(count)
                if matches!(count.kind(), bray_symbols::ConstantValueKind::Integer(count) if count.to_u64().is_some_and(|count| count <= 1)));

            if !singleton {
                self.transfers.insert(expression);
            }
        }

        let invocation = operation.occurrence();

        if let Some(contract) = domain
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
            && !contract.execution_requirements.is_empty()
            && let Some(entry) = state.entries.get(&invocation)
            && !entry.proves(&contract.execution_requirements)
        {
            self.failed_execution.insert(expression);
        }

        if domain
            .request
            .trusted_contracts()
            .is_some_and(|contracts| contracts.calls.contains_key(&invocation))
            && state.entries.contains_key(&invocation)
            && evidence.is_none()
        {
            self.failed.insert(expression);
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
            _ => state.result.clone(),
        };

        self.collect_carriers(domain.request, domain.dependencies, state, &result);

        if domain.request.unit().key().kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault {
            let observed = super::carriers::portable(domain.request, &result);

            match &mut self.observed_result {
                Some(previous) if *previous != observed => *previous = None,
                Some(_) => {}
                None => self.observed_result = Some(observed),
            }
        }

        let input = super::carriers::returned_input(domain, &result);

        match &mut self.returned_input {
            Some(previous) if *previous != input => *previous = None,
            Some(_) => {}
            None => self.returned_input = Some(input),
        }
    }

    pub(in crate::analysis) fn finish<C: CheckerRequestContext + ?Sized>(
        self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
    ) -> DiagnosticBag {
        let mut diagnostics = DiagnosticBag::new();

        for ((_, span), failed) in self.guarantees.into_iter().zip(self.guarantee_failures) {
            if !failed {
                continue;
            }

            diagnostics.add(
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
                .with_arg(DiagnosticArg::expression_category(
                    DiagnosticExpressionCategory::Call,
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::TrustedObligationEvidenceRequired,
                )),
            );
        }

        for node in self.cleanup_failures {
            diagnostics.add(contract_diagnostic(
                domain.request,
                domain.semantics,
                node,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            ));
        }

        for expression in self.transfers {
            diagnostics.add(contract_diagnostic(
                domain.request,
                domain.semantics,
                expression.into(),
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            ));
        }

        for expression in self.failed_execution {
            diagnostics.add(contract_diagnostic(
                domain.request,
                domain.semantics,
                expression.into(),
                DiagnosticKind::CheckingExecutionRequirementNotProven,
            ));
        }

        for expression in self.failed {
            diagnostics.add(contract_diagnostic(
                domain.request,
                domain.semantics,
                expression.into(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            ));
        }

        diagnostics
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
            evidence.proves(
                contract
                    .preconditions
                    .iter()
                    .map(|(condition, _)| condition),
            ),
        )
    }

    pub(in crate::analysis) fn trusted_memory_evidence(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<bool> {
        let contract = self.request.trusted_contracts()?.calls.get(&invocation)?;
        let evidence = state.entries.get(&invocation)?;

        self.trusted_requirements_proven(evidence, &contract.requirements)
            .then(|| self.empty_memory_copy(state, invocation))
    }
}

fn contract_diagnostic<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    node: AnyBoundNodeId,
    kind: DiagnosticKind,
) -> Diagnostic {
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
        && let Some(name) = request
            .context()
            .symbols()
            .member_name(callable.definition().symbol())
    {
        diagnostic = diagnostic.with_label(
            DiagnosticLabel::secondary(DiagnosticLabelKind::TrustedCallable, span)
                .with_arg(DiagnosticArg::declaration_name(name.as_str())),
        );
    }

    diagnostic
}
