use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit, SelectionKind,
};
use bray_checker::{ExpressionCandidateSet, OperationCandidateSource};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableContractTemplateFact, CallableOverloadTemplateFact,
    CallableParameterDefaultTemplateFact, CallableSignatureFact, GenericDeclarationTemplateFact,
};
use bray_syntax::GenericArgumentSyntax;

use super::callable::bind_call_candidates;
use crate::{
    BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider, TypeExpressionScope,
};

/// Enumerates candidate surfaces for one exact bound expression without selecting a target.
///
/// The bound unit and expression must belong to the same semantic unit. Cancellation and
/// unavailable or inconsistent inputs are returned as binder fact errors.
pub fn bind_expression_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    expression: BoundExpressionId,
    type_scope: &TypeExpressionScope,
) -> BinderFactResult<DiagnosticResult<ExpressionCandidateSet>>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
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
        BoundExpression::Call(call) if is_union_variant_call(unit, call.callee()) => operation(
            expression,
            SelectionKind::Construction,
            call.arguments()
                .iter()
                .map(bray_bound_tree::BoundArgument::expression),
        ),
        BoundExpression::Call(call) => {
            let arguments = generic_argument_syntax(context, call.generic_arguments())?;

            return bind_call_candidates(
                context,
                unit,
                expression,
                call.callee(),
                &arguments,
                type_scope,
            );
        }
        BoundExpression::ErrorCall(call) => {
            let arguments = generic_argument_syntax(context, call.generic_arguments())?;

            return bind_call_candidates(
                context,
                unit,
                expression,
                call.callee(),
                &arguments,
                type_scope,
            );
        }
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            operation(expression, SelectionKind::Member, bound.child_expressions())
        }
        BoundExpression::Unary(_) | BoundExpression::Binary(_) | BoundExpression::Assignment(_) => {
            operation(
                expression,
                SelectionKind::Operator,
                bound.child_expressions(),
            )
        }
        BoundExpression::Conversion(_) => operation(
            expression,
            SelectionKind::Conversion,
            bound.child_expressions(),
        ),
        BoundExpression::StructConstruction(construction) => operation(
            expression,
            SelectionKind::Construction,
            construction
                .fields()
                .iter()
                .map(bray_bound_tree::BoundStructFieldInitializer::expression),
        ),
        BoundExpression::LeadingDotVariant(_) => {
            operation(expression, SelectionKind::Construction, [])
        }
        BoundExpression::Structured(structured)
            if matches!(
                structured.kind(),
                BoundStructuredExpressionKind::ElementIndex
                    | BoundStructuredExpressionKind::SliceIndex
            ) =>
        {
            operation(expression, SelectionKind::Index, bound.child_expressions())
        }
        BoundExpression::Structured(structured)
            if structured.kind() == BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            operation(
                expression,
                SelectionKind::Construction,
                bound.child_expressions(),
            )
        }
        _ => ExpressionCandidateSet::NotApplicable(expression),
    };

    Ok(DiagnosticResult::without_diagnostics(candidates))
}

fn generic_argument_syntax<C>(
    context: &C,
    arguments: &[bray_bound_tree::BoundGenericArgument],
) -> BinderFactResult<Vec<GenericArgumentSyntax>>
where
    C: BinderFactContext + ?Sized,
{
    arguments
        .iter()
        .map(|argument| {
            argument
                .syntax()
                .find_descendant::<GenericArgumentSyntax>(context.syntax())
                .ok_or(BinderFactError::DependencyUnavailable)
        })
        .collect()
}

fn operation(
    expression: BoundExpressionId,
    kind: SelectionKind,
    operands: impl IntoIterator<Item = BoundExpressionId>,
) -> ExpressionCandidateSet {
    ExpressionCandidateSet::Operation(OperationCandidateSource::new(expression, kind, operands))
}

fn is_union_variant_call(unit: &BoundUnit, callee: BoundExpressionId) -> bool {
    matches!(
        unit.view().expression(callee),
        Some(BoundExpression::Name(name))
            if matches!(
                name.target(),
                bray_bound_tree::BoundReferenceTarget::Surface(
                    bray_symbols::AnySymbolId::UnionVariant(_)
                )
            )
    )
}
