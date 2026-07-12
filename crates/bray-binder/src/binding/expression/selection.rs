use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundLeadingDotVariantExpression,
    BoundMemberAccessExpression, BoundMemberSelector, BoundTraitQualifiedMemberExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_syntax::{
    LeadingDotVariantExpressionSyntax, MemberAccessOperationSyntax, SourceSyntaxNode,
    TraitQualifiedMemberOperationSyntax,
};

use super::super::name::symbol_name;
use super::ExpressionBinder;
use super::support::member_selector;
use crate::BinderFactContext;
use crate::binding::BindingResult;
use crate::request::BinderRequestContext;

impl ExpressionBinder {
    pub(super) fn bind_leading_dot_variant<C>(
        &self,
        request: &mut BinderRequestContext<'_, C>,
        syntax: &LeadingDotVariantExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let selector =
            symbol_name(syntax.source(), &syntax.identifier_token()).map(BoundMemberSelector::Name);
        let expression = BoundLeadingDotVariantExpression::new(
            request.source_origin(syntax),
            selector,
            None,
            syntax.is_recovered(),
        );

        self.push(request, BoundExpression::LeadingDotVariant(expression))
    }

    pub(super) fn bind_member_access<C>(
        &self,
        request: &mut BinderRequestContext<'_, C>,
        syntax: &MemberAccessOperationSyntax,
        receiver: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let selector = member_selector(syntax);
        let is_recovered = syntax.is_recovered()
            || selector.is_none()
            || request.expression_is_recovered(receiver);

        let expression = BoundMemberAccessExpression::new(
            request.source_origin(syntax),
            receiver,
            selector,
            None,
            is_recovered,
        );

        self.push(request, BoundExpression::MemberAccess(expression))
    }

    pub(super) fn bind_trait_qualified_member<C>(
        &self,
        request: &mut BinderRequestContext<'_, C>,
        syntax: &TraitQualifiedMemberOperationSyntax,
        receiver: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let member = syntax.member_access_operation();
        let selector = member_selector(&member);
        let trait_syntax = SyntaxAnchor::from_node(&syntax.trait_application());
        let is_recovered = syntax.is_recovered()
            || selector.is_none()
            || trait_syntax.is_recovered()
            || request.expression_is_recovered(receiver);

        let expression = BoundTraitQualifiedMemberExpression::new(
            request.source_origin(syntax),
            receiver,
            trait_syntax,
            selector,
            None,
            is_recovered,
        );

        self.push(request, BoundExpression::TraitQualifiedMember(expression))
    }
}
