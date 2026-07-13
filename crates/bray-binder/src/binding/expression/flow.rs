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
use crate::binder::{Binder, ControlTarget, ControlTargetKind};
use crate::binding::{BindingError, BindingResult};

impl ExpressionBinder {
    pub(super) fn bind_control_transfer<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
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

        let operand = self.bind_optional_operand(binder, scope, syntax)?;
        let target = control_transfer_target(binder, kind);

        let is_recovered = syntax.is_recovered()
            || target.is_none()
            || operand.is_some_and(|operand| binder.expression_is_recovered(operand));

        let expression = BoundControlTransferExpression::new(
            binder.source_origin(&syntax),
            kind,
            operand,
            target,
            None,
            is_recovered,
        );

        self.push(binder, BoundExpression::ControlTransfer(expression))
    }

    fn bind_optional_operand<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
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

            result = Some(self.bind_expression(binder, scope, Some(&expression)));

            SyntaxWalkControl::Stop
        });

        result.transpose()
    }
}

fn control_transfer_target<C>(
    binder: &Binder<'_, C>,
    kind: BoundControlTransferKind,
) -> Option<bray_declarations::SyntaxAnchor>
where
    C: BinderFactContext + ?Sized,
{
    let target = match kind {
        BoundControlTransferKind::Yield => binder
            .control_target_of_kind(ControlTargetKind::Generator)
            .or_else(|| binder.control_target_of_kind(ControlTargetKind::Block)),
        BoundControlTransferKind::Return => {
            binder.control_target_of_kind(ControlTargetKind::Callable)
        }
        BoundControlTransferKind::Break | BoundControlTransferKind::Continue => binder
            .control_target_of_kind(ControlTargetKind::Generator)
            .or_else(|| binder.control_target_of_kind(ControlTargetKind::Loop)),
    };

    target.map(ControlTarget::syntax)
}
