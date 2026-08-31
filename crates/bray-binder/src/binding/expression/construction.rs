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
