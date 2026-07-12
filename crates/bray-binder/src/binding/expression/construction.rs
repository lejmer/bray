use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructConstructionExpression,
    BoundStructFieldInitializer,
};
use bray_symbols::LocalScopeId;
use bray_syntax::{SourceSyntaxNode, StructConstructionBodySyntax};

use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binding::BindingResult;
use crate::binding::name::symbol_name;
use crate::request::BinderRequestContext;

impl ExpressionBinder {
    pub(super) fn bind_struct_construction<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &StructConstructionBodySyntax,
        head: Option<BoundExpressionId>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut fields = Vec::new();

        for field in syntax.field_initializers() {
            let expression = self.bind_expression(request, scope, Some(&field.expression()))?;
            let token = field.identifier_token();
            let name = symbol_name(field.source(), &token);
            let is_recovered = field.is_recovered()
                || name.is_none()
                || request.expression_is_recovered(expression);

            fields.push(BoundStructFieldInitializer::new(
                name,
                expression,
                is_recovered,
            ));
        }

        let is_recovered = syntax.is_recovered()
            || head.is_some_and(|head| request.expression_is_recovered(head))
            || fields.iter().any(BoundStructFieldInitializer::is_recovered);

        let expression = BoundStructConstructionExpression::new(
            request.source_origin(syntax),
            head,
            fields,
            None,
            is_recovered,
        );

        self.push(request, BoundExpression::StructConstruction(expression))
    }
}
