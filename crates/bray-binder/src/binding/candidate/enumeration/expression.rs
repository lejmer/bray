use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
    DeclaredValueTypeTemplates, SelectionKind,
};
use bray_checker::ExpressionCandidateSet;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableOverloadTemplateFact, CallableParameterDefaultTemplateFact, CallableSignatureFact,
    GenericDeclarationTemplateFact,
};

use super::callable::bind_call_candidates;
use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

/// Enumerates candidate surfaces for one exact bound expression without selecting a target.
///
/// The bound unit, declared-type templates, and expression must belong to the same semantic unit.
/// Cancellation and unavailable or inconsistent inputs are returned as binder fact errors.
pub fn bind_expression_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    declared_types: &DeclaredValueTypeTemplates,
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

    if unit.unit() != expression.unit()
        || declared_types.unit() != unit.unit()
        || declared_types.kind() != unit.key().kind()
    {
        return Err(BinderFactError::DependencyUnavailable);
    }

    let Some(bound) = unit.view().expression(expression) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let mut diagnostics = DiagnosticBag::new();

    let candidates = match bound {
        BoundExpression::Call(call) => bind_call_candidates(
            context,
            unit,
            declared_types,
            expression,
            call.callee(),
            &mut diagnostics,
        )?,
        BoundExpression::ErrorCall(call) => bind_call_candidates(
            context,
            unit,
            declared_types,
            expression,
            call.callee(),
            &mut diagnostics,
        )?,
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

    Ok(DiagnosticResult::new(candidates, diagnostics))
}

const fn unsupported(expression: BoundExpressionId, kind: SelectionKind) -> ExpressionCandidateSet {
    // TODO(BRA-122): Add candidate providers for the remaining semantic selection categories.
    ExpressionCandidateSet::Unsupported { expression, kind }
}
