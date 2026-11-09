use bray_bound_tree::AnyBoundNodeId;
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticContractObligation, DiagnosticContractObligationKind, DiagnosticKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind};

use crate::analysis::guarantee::flow::ExecutionFlowDomain;
use crate::{CheckerQueryResult, CheckerRequestContext, UnprovenExecutionCondition};

pub(in crate::analysis::guarantee::trusted) fn obligation<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, kind: DiagnosticContractObligationKind, failed: &UnprovenExecutionCondition,
) -> CheckerQueryResult<DiagnosticArg, C::UpstreamError> {
    Ok(DiagnosticArg::contract_obligation(DiagnosticContractObligation {
        kind,
        required: super::condition::condition(domain, failed.required(), &mut 128)?,
        actual: super::condition::condition(domain, failed.actual(), &mut 128)?,
    }))
}

pub(in crate::analysis::guarantee::trusted) fn failed_obligation<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, node: AnyBoundNodeId, diagnostic_kind: DiagnosticKind,
    kind: DiagnosticContractObligationKind, failed: &UnprovenExecutionCondition, callable: Option<bray_symbols::CallableInstanceData>,
) -> CheckerQueryResult<Diagnostic, C::UpstreamError> {
    let mut diagnostic = super::super::check::contract_diagnostic(domain.request, domain.semantics, node, diagnostic_kind)
        .with_arg(obligation(domain, kind, failed)?);

    if let Some(anchor) = callable.and_then(|callable| domain.request.symbols().declaration_syntax_anchor(callable.definition().symbol())) {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::RequirementOrigin, domain.request.source_syntax(anchor).span(),
        ));
    }

    Ok(diagnostic)
}
