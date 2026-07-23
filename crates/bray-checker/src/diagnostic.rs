use bray_bound_tree::{BoundExpressionId, BoundNodeOrigin, BoundPatternId};
use bray_diagnostics::DiagnosticId;
use bray_source::SourceSpan;

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn expression_span<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
    };

    source_span(request, expression.origin())
}

pub(crate) fn pattern_span<C>(
    request: CheckerUnitView<'_, C>,
    pattern: BoundPatternId,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(pattern) = request.view().pattern(pattern) else {
        return Err(CheckerInfrastructureError::InvalidBoundNode {
            node: pattern.into(),
        });
    };

    source_span(request, pattern.origin())
}

pub(crate) fn diagnostic_id(index: usize) -> DiagnosticId {
    DiagnosticId::new(u32::try_from(index).unwrap_or(u32::MAX))
}

fn source_span<C>(
    request: CheckerUnitView<'_, C>,
    origin: BoundNodeOrigin,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    request
        .source(origin.source_anchor())
        .map(|source| source.span())
}
