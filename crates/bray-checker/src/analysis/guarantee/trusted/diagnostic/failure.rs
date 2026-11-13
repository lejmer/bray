use bray_bound_tree::AnyBoundNodeId;
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticContractObligationKind, DiagnosticKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind};

use crate::analysis::guarantee::flow::ExecutionFlowDomain;
use crate::{CheckerQueryResult, CheckerRequestContext, UnprovenExecutionCondition};

pub(in crate::analysis::guarantee::trusted) fn with_obligation<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, kind: DiagnosticContractObligationKind, failed: &UnprovenExecutionCondition,
    diagnostic: Diagnostic,
) -> CheckerQueryResult<Diagnostic, C::UpstreamError> {
    let mut diagnostic = diagnostic.with_arg(DiagnosticArg::contract_obligation(super::super::super::diagnostic::obligation(domain, kind, failed)?));

    for mutation in failed.actual().observed_mutations() {
        let origin = crate::diagnostic::bound_node_origin(domain.request, mutation.occurrence().node())
            .expect("a current value's mutation belongs to the checked unit");

        let anchor = origin.source_anchor().syntax();

        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::ValueChanged,
            bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range()),
        ));
    }

    Ok(diagnostic)
}

pub(in crate::analysis::guarantee::trusted) fn failed_obligation<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, node: AnyBoundNodeId, diagnostic_kind: DiagnosticKind,
    kind: DiagnosticContractObligationKind, failed: &UnprovenExecutionCondition, callable: Option<bray_symbols::CallableInstanceData>,
) -> CheckerQueryResult<Diagnostic, C::UpstreamError> {
    let mut diagnostic = with_obligation(domain, kind, failed,
        super::super::check::contract_diagnostic(domain.request, domain.semantics, node, diagnostic_kind)?)?;

    if let Some(anchor) = callable.and_then(|callable| domain.request.symbols().declaration_syntax_anchor(callable.definition().symbol())) {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::RequirementOrigin, domain.request.source_syntax(anchor).span(),
        ));
    }

    Ok(diagnostic)
}
