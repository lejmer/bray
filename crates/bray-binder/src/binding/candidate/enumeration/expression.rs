use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit, SelectionKind,
};
use bray_checker::{ExpressionCandidateSet, OperationCandidateSource};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractTemplateQuery, CallableOverloadTemplateQuery,
    CallableParameterDefaultTemplateQuery, CallableSignatureQuery, GenericDeclarationTemplateQuery,
    MemberLookupResult, PredicateSignatureTemplateQuery,
};
use bray_syntax::GenericArgumentSyntax;

use super::callable::bind_call_candidates;
use crate::{
    BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider,
    TypeExpressionScope,
};

/// Enumerates candidate surfaces for one exact bound expression without selecting a target.
///
/// The bound unit and expression must belong to the same semantic unit. Cancellation and
/// unavailable or inconsistent inputs are returned as binding-query errors.
pub fn bind_expression_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    expression: BoundExpressionId,
    type_scope: &TypeExpressionScope,
) -> BindingQueryResult<DiagnosticResult<ExpressionCandidateSet>, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    if context.is_cancelled() {
        return Err(BindingQueryError::Cancelled);
    }

    if unit.unit() != expression.unit() {
        return Err(BindingQueryError::DependencyUnavailable);
    }

    let Some(bound) = unit.view().expression(expression) else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let candidates = match bound {
        BoundExpression::Call(call) if is_union_variant_call(context, unit, call.callee()) => {
            operation(
                expression,
                SelectionKind::Construction,
                call.arguments()
                    .iter()
                    .map(bray_bound_tree::BoundArgument::expression),
            )
        }
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
        BoundExpression::MemberAccess(_)
            if qualified_union_variant(context, unit, expression).is_some() =>
        {
            operation(expression, SelectionKind::Construction, [])
        }
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            operation(expression, SelectionKind::Member, bound.child_expressions())
        }
        BoundExpression::Unary(_) | BoundExpression::Binary(_) => operation(
            expression,
            SelectionKind::Operator,
            bound.child_expressions(),
        ),
        BoundExpression::Assignment(assignment)
            if assignment.operator().binary_operator().is_some() =>
        {
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
        BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_) => {
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
        BoundExpression::Assignment(_) => ExpressionCandidateSet::NotApplicable(expression),
        _ => ExpressionCandidateSet::NotApplicable(expression),
    };

    Ok(DiagnosticResult::without_diagnostics(candidates))
}

fn generic_argument_syntax<C>(
    context: &C,
    arguments: &[bray_bound_tree::BoundGenericArgument],
) -> BindingQueryResult<Vec<GenericArgumentSyntax>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    arguments
        .iter()
        .map(|argument| {
            argument
                .syntax()
                .find_descendant::<GenericArgumentSyntax>(context.syntax())
                .ok_or(BindingQueryError::DependencyUnavailable)
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

fn is_union_variant_call<C>(context: &C, unit: &BoundUnit, callee: BoundExpressionId) -> bool
where
    C: BindingQueryContext + ?Sized,
{
    match unit.view().expression(callee) {
        Some(BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_)) => {
            true
        }
        Some(BoundExpression::Name(name)) => matches!(
            name.target(),
            bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::AnySymbolId::UnionVariant(_)
            )
        ),
        Some(BoundExpression::MemberAccess(_)) => {
            qualified_union_variant(context, unit, callee).is_some()
        }
        _ => false,
    }
}

/// Returns the variant named by an explicitly union-qualified member expression.
pub fn qualified_union_variant<C>(
    context: &C,
    unit: &BoundUnit,
    expression: BoundExpressionId,
) -> Option<bray_symbols::UnionVariantSymbolId>
where
    C: BindingQueryContext + ?Sized,
{
    let Some(BoundExpression::MemberAccess(member)) = unit.view().expression(expression) else {
        return None;
    };

    let Some(BoundExpression::Name(receiver)) = unit.view().expression(member.receiver()) else {
        return None;
    };

    let bray_bound_tree::BoundReferenceTarget::Surface(AnySymbolId::Union(union)) =
        receiver.target()
    else {
        return None;
    };

    let Some(bray_bound_tree::BoundMemberSelector::Name(name)) = member.selector() else {
        return None;
    };

    match context.lookup_member(union.into(), name.as_str()).ok()? {
        MemberLookupResult::Found(AnySymbolId::UnionVariant(variant)) => Some(variant),
        _ => None,
    }
}
