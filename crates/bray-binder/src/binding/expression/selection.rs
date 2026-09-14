use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundLeadingDotVariantExpression,
    BoundMemberAccessExpression, BoundMemberSelector, BoundReferenceTarget,
    BoundTraitQualifiedMemberExpression,
};
use bray_symbols::{AnySymbolId, MemberLookupResult};
use bray_syntax::{
    LeadingDotVariantExpressionSyntax, MemberAccessOperationSyntax, SourceSyntaxNode,
};

use super::super::name::symbol_name;
use super::ExpressionBinder;
use super::support::member_selector;
use crate::BindingQueryContext;
use crate::binder::Binder;
use crate::binding::BindingResult;

impl ExpressionBinder {
    pub(super) fn bind_leading_dot_variant<C>(
        &self,
        binder: &mut Binder<'_, C>,
        syntax: &LeadingDotVariantExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
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

    pub(super) fn bind_call_member<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        syntax: &MemberAccessOperationSyntax,
        call: BoundExpression,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        if let BoundExpression::Call(call) = &call
            && call.generic_arguments().is_empty()
            && let [argument] = call.arguments()
            && argument.name().is_none()
            && let Some(BoundExpression::Name(name)) =
                binder.unit_view().expression(argument.expression())
            && matches!(
                name.target(),
                BoundReferenceTarget::Surface(AnySymbolId::Trait(_))
            )
        {
            let expression = BoundTraitQualifiedMemberExpression::new(
                binder.source_origin(syntax),
                call.callee(),
                argument.expression(),
                member_selector(syntax),
                None,
                syntax.is_recovered() || call.is_recovered(),
            );

            return self.push(binder, BoundExpression::TraitQualifiedMember(expression));
        }

        let receiver = self.push(binder, call)?;

        self.bind_member_access(binder, syntax, receiver)
    }

    pub(super) fn bind_member_access<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        syntax: &MemberAccessOperationSyntax,
        receiver: BoundExpressionId,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let namespace = match binder.unit_view().expression(receiver) {
            Some(BoundExpression::Name(name)) => match name.target() {
                BoundReferenceTarget::Surface(AnySymbolId::Module(module)) => Some(module),
                BoundReferenceTarget::Local(_) | BoundReferenceTarget::Surface(_) => None,
            },
            _ => None,
        };

        if let Some(namespace) = namespace {
            let Some(token) = syntax.identifier_token() else {
                return self.push_error(binder, Some(syntax));
            };

            let member = binder.bind_member(
                namespace.into(),
                syntax.source(),
                token,
                self.path_context.access(),
            )?;

            return match member {
                MemberLookupResult::Found(member) => self.push_resolved_reference(
                    binder,
                    syntax,
                    BoundReferenceTarget::Surface(member.symbol()),
                ),
                MemberLookupResult::NotFound
                | MemberLookupResult::WrongKind(_)
                | MemberLookupResult::Ambiguous(_)
                | MemberLookupResult::Inaccessible(_)
                | MemberLookupResult::Malformed(_) => self.push_error(binder, Some(syntax)),
            };
        }

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
}
