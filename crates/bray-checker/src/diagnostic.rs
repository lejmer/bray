use bray_bound_tree::BoundExpressionId;
use bray_diagnostics::DiagnosticId;
use bray_source::SourceSpan;

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

pub(crate) fn expression_span<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
    };

    request
        .source(expression.origin().source_anchor())
        .map(|source| source.span())
}

pub(crate) fn diagnostic_id(index: usize) -> DiagnosticId {
    DiagnosticId::new(u32::try_from(index).unwrap_or(u32::MAX))
}
