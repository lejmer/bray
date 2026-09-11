use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructConstructionExpression,
    BoundStructFieldInitializer,
};
use bray_symbols::LocalScopeId;
use bray_syntax::{SourceSyntaxNode, StructConstructionBodySyntax};

use super::ExpressionBinder;
use crate::BindingQueryContext;
use crate::binder::Binder;
use crate::binding::BindingResult;
use crate::binding::name::symbol_name;

impl ExpressionBinder {
    pub(super) fn bind_box_construction<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &bray_syntax::TypeFormConstructionExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let arguments = self.bind_arguments(binder, scope, &syntax.argument_list())?;

        let mut recovered = syntax.is_recovered()
            || arguments.iter().any(|argument| {
                argument.is_recovered() || binder.expression_is_recovered(argument.expression())
            });

        let mut policies = syntax.type_form_argument_lists();

        let policy = match crate::binding::type_expression::box_storage_policy(policies.next()) {
            Ok(policy) => policy.map(|policy| {
                bray_bound_tree::BoundTypeReference::new(
                    bray_declarations::SyntaxAnchor::from_node(&policy),
                    None,
                )
            }),
            Err(diagnostic) => {
                binder.add_diagnostic(diagnostic);
                recovered = true;

                None
            }
        };

        recovered |= policies.next().is_some();

        self.push(
            binder,
            BoundExpression::BoxConstruction(bray_bound_tree::BoundBoxConstructionExpression::new(
                binder.source_origin(syntax),
                policy,
                arguments,
                recovered,
            )),
        )
    }

    pub(super) fn bind_struct_construction<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &StructConstructionBodySyntax,
        head: Option<BoundExpressionId>,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let mut fields = Vec::new();

        for field in syntax.field_initializers() {
            let expression = self.bind_expression(binder, scope, Some(&field.expression()))?;
            let token = field.identifier_token();
            let name = symbol_name(field.source(), &token);

            let is_recovered = field.is_recovered()
                || name.is_none()
                || binder.expression_is_recovered(expression);

            fields.push(BoundStructFieldInitializer::new(
                name,
                expression,
                is_recovered,
            ));
        }

        let is_recovered = syntax.is_recovered()
            || head.is_some_and(|head| binder.expression_is_recovered(head))
            || fields.iter().any(BoundStructFieldInitializer::is_recovered);

        let expression = BoundStructConstructionExpression::new(
            binder.source_origin(syntax),
            head,
            fields,
            None,
            is_recovered,
        );

        self.push(binder, BoundExpression::StructConstruction(expression))
    }
}
