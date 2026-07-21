use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit, SelectionKind,
};
use bray_checker::ExpressionCandidateSet;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableOverloadTemplateFact, CallableParameterDefaultTemplateFact, CallableSignatureFact,
    GenericDeclarationTemplateFact,
};

use super::callable::bind_call_candidates;
use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

/// Enumerates candidate surfaces for one exact bound expression without selecting a target.
///
/// The bound unit and expression must belong to the same semantic unit. Cancellation and
/// unavailable or inconsistent inputs are returned as binder fact errors.
pub fn bind_expression_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    expression: BoundExpressionId,
) -> BinderFactResult<DiagnosticResult<ExpressionCandidateSet>>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    if context.is_cancelled() {
        return Err(BinderFactError::Cancelled);
    }

    if unit.unit() != expression.unit() {
        return Err(BinderFactError::DependencyUnavailable);
    }

    let Some(bound) = unit.view().expression(expression) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let candidates = match bound {
        BoundExpression::Call(call) => {
            bind_call_candidates(context, unit, expression, call.callee())?
        }
        BoundExpression::ErrorCall(call) => {
            bind_call_candidates(context, unit, expression, call.callee())?
        }
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            unsupported(expression, SelectionKind::Member)
        }
        BoundExpression::Unary(_) | BoundExpression::Binary(_) => {
            unsupported(expression, SelectionKind::Operator)
        }
        BoundExpression::Conversion(_) | BoundExpression::ErrorConversion(_) => {
            unsupported(expression, SelectionKind::Conversion)
        }
        BoundExpression::StructConstruction(_) | BoundExpression::LeadingDotVariant(_) => {
            unsupported(expression, SelectionKind::Construction)
        }
        BoundExpression::Structured(structured)
            if matches!(
                structured.kind(),
                BoundStructuredExpressionKind::ElementIndex
                    | BoundStructuredExpressionKind::SliceIndex
            ) =>
        {
            unsupported(expression, SelectionKind::Index)
        }
        BoundExpression::Structured(structured)
            if structured.kind() == BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            unsupported(expression, SelectionKind::Construction)
        }
        _ => ExpressionCandidateSet::NotApplicable(expression),
    };

    Ok(DiagnosticResult::without_diagnostics(candidates))
}

const fn unsupported(expression: BoundExpressionId, kind: SelectionKind) -> ExpressionCandidateSet {
    // TODO(BRA-122): Add candidate providers for the remaining semantic selection categories.
    ExpressionCandidateSet::Unsupported { expression, kind }
}
