use bray_bound_tree::{
    BoundControlTransferExpression, BoundControlTransferKind, BoundExpression, BoundExpressionId,
};
use bray_symbols::LocalScopeId;
use bray_syntax::{
    ExpressionSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    walk_syntax_node,
};

use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binding::{BindingError, BindingResult};
use crate::request::{BinderRequestContext, ControlTarget, ControlTargetKind};

impl ExpressionBinder {
    pub(super) fn bind_control_transfer<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let kind = match syntax.kind() {
            SyntaxKind::YieldExpression => BoundControlTransferKind::Yield,
            SyntaxKind::ReturnExpression => BoundControlTransferKind::Return,
            SyntaxKind::BreakExpression => BoundControlTransferKind::Break,
            SyntaxKind::ContinueExpression => BoundControlTransferKind::Continue,
            _ => return Err(BindingError::UnsupportedSyntax),
        };

        let operand = self.bind_optional_operand(request, scope, syntax)?;
        let target = control_transfer_target(request, kind);

        let is_recovered = syntax.is_recovered()
            || target.is_none()
            || operand.is_some_and(|operand| request.expression_is_recovered(operand));

        let expression = BoundControlTransferExpression::new(
            request.source_origin(&syntax),
            kind,
            operand,
            target,
            None,
            is_recovered,
        );

        self.push(request, BoundExpression::ControlTransfer(expression))
    }

    fn bind_optional_operand<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: SyntaxNodeView<'_>,
    ) -> BindingResult<Option<BoundExpressionId>>
    where
        C: BinderFactContext + ?Sized,
    {
        let mut result = None;
        let mut first = true;

        walk_syntax_node(&syntax, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            if first {
                first = false;

                return SyntaxWalkControl::Continue;
            }

            if node.kind() != SyntaxKind::Expression {
                return SyntaxWalkControl::Continue;
            }

            let Some(expression) = node.cast::<ExpressionSyntax>() else {
                result = Some(Err(BindingError::UnsupportedSyntax));

                return SyntaxWalkControl::Stop;
            };

            result = Some(self.bind_expression(request, scope, Some(&expression)));

            SyntaxWalkControl::Stop
        });

        result.transpose()
    }
}

fn control_transfer_target<C>(
    request: &BinderRequestContext<'_, C>,
    kind: BoundControlTransferKind,
) -> Option<bray_declarations::SyntaxAnchor>
where
    C: BinderFactContext + ?Sized,
{
    let target = match kind {
        BoundControlTransferKind::Yield => request
            .control_target_of_kind(ControlTargetKind::Generator)
            .or_else(|| request.control_target_of_kind(ControlTargetKind::Block)),
        BoundControlTransferKind::Return => {
            request.control_target_of_kind(ControlTargetKind::Callable)
        }
        BoundControlTransferKind::Break | BoundControlTransferKind::Continue => request
            .control_target_of_kind(ControlTargetKind::Generator)
            .or_else(|| request.control_target_of_kind(ControlTargetKind::Loop)),
    };

    target.map(ControlTarget::syntax)
}
