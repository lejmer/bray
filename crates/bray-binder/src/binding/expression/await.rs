use bray_bound_tree::{BoundAwaitExpression, BoundExpression, BoundExpressionId};
use bray_symbols::LocalScopeId;
use bray_syntax::AwaitExpressionSyntax;

use super::super::BindingResult;
use super::ExpressionBinder;
use crate::BindingQueryContext;
use crate::binder::Binder;

impl ExpressionBinder {
    pub(super) fn bind_await<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &AwaitExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let operand = self.bind_expression(binder, scope, Some(&syntax.expression()))?;
        let is_recovered = syntax.is_recovered() || binder.expression_is_recovered(operand);

        self.push(
            binder,
            BoundExpression::Await(BoundAwaitExpression::pending(
                binder.source_origin(syntax),
                operand,
                is_recovered,
            )),
        )
    }
}
