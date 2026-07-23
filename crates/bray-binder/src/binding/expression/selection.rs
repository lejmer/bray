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
use crate::binder::Binder;
use crate::binding::BindingResult;

impl ExpressionBinder {
    pub(super) fn bind_leading_dot_variant<C>(
        &self,
        binder: &mut Binder<'_, C>,
        syntax: &LeadingDotVariantExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let selector =
            symbol_name(syntax.source(), &syntax.identifier_token()).map(BoundMemberSelector::Name);

        let expression = BoundLeadingDotVariantExpression::new(
            binder.source_origin(syntax),
            selector,
            None,
            syntax.is_recovered(),
        );

        self.push(binder, BoundExpression::LeadingDotVariant(expression))
    }

    pub(super) fn bind_member_access<C>(
        &self,
        binder: &mut Binder<'_, C>,
        syntax: &MemberAccessOperationSyntax,
        receiver: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let selector = member_selector(syntax);

        let is_recovered =
            syntax.is_recovered() || selector.is_none() || binder.expression_is_recovered(receiver);

        let expression = BoundMemberAccessExpression::new(
            binder.source_origin(syntax),
            receiver,
            selector,
            None,
            is_recovered,
        );

        self.push(binder, BoundExpression::MemberAccess(expression))
    }

    pub(super) fn bind_trait_qualified_member<C>(
        &self,
        binder: &mut Binder<'_, C>,
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
            || binder.expression_is_recovered(receiver);

        let expression = BoundTraitQualifiedMemberExpression::new(
            binder.source_origin(syntax),
            receiver,
            trait_syntax,
            selector,
            None,
            is_recovered,
        );

        self.push(binder, BoundExpression::TraitQualifiedMember(expression))
    }
}
