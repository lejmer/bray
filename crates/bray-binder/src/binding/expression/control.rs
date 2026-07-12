use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundForExpression, BoundMatchArm, BoundMatchExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId};
use bray_syntax::{ForExpressionSyntax, MatchExpressionSyntax};

use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binding::BindingResult;
use crate::request::{BinderRequestContext, ControlTarget, ControlTargetKind, PatternBindingMode};

impl ExpressionBinder {
    pub(super) fn bind_for_expression<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &ForExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let iteration = self.bind_expression(
            request,
            scope,
            Some(&syntax.iteration_source().expression()),
        )?;

        let pattern_syntax = syntax.irrefutable_pattern();
        let pattern_scope = request.unit_mut().push_scope(
            scope,
            LocalScopeBoundary::PatternArm,
            SyntaxAnchor::from_node(&pattern_syntax),
            pattern_syntax.full_range().start(),
        )?;

        let pattern = request.bind_irrefutable_pattern(
            self.path_context_for(pattern_scope, self.path_context.access()),
            &pattern_syntax,
            self.error_type,
            PatternBindingMode::Declaration,
        )?;

        request.activate_pattern_bindings(pattern_scope, &pattern)?;

        let target = ControlTarget::new(
            ControlTargetKind::Loop,
            SyntaxAnchor::from_node(syntax),
            None,
        );

        request.push_control_target(target);

        let mut blocks = Vec::new();
        let mut failure = None;

        for (index, block) in syntax.block_expressions().enumerate() {
            let block_scope = if index == 0 { pattern_scope } else { scope };

            match request.bind_block(block_scope, &block, self) {
                Ok(block) => blocks.push(block),
                Err(error) => {
                    failure = Some(error);

                    break;
                }
            }
        }

        if request.pop_control_target() != Some(target) {
            return Err(crate::binding::BindingError::ControlTargetMismatch);
        }

        if let Some(error) = failure {
            return Err(error);
        }

        let pattern_id = pattern.pattern();
        let mut blocks = blocks.into_iter();

        let Some(body) = blocks.next() else {
            return Err(crate::binding::BindingError::UnsupportedSyntax);
        };

        let else_body = blocks.next();
        let recovered = syntax.is_recovered()
            || request.expression_is_recovered(iteration)
            || request.pattern_is_recovered(pattern_id)
            || request.block_is_recovered(body)
            || else_body.is_some_and(|block| request.block_is_recovered(block));

        let expression = BoundForExpression::new(
            request.source_origin(syntax),
            iteration,
            pattern_id,
            body,
            else_body,
            None,
            recovered,
        );

        self.push(request, BoundExpression::For(expression))
    }

    pub(super) fn bind_match_expression<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &MatchExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let subject =
            self.bind_expression(request, scope, Some(&syntax.match_subject().expression()))?;

        let mut arms = Vec::new();

        for arm in syntax.match_body().match_arms() {
            request.check_cancellation()?;

            let pattern_syntax = arm.case_pattern();
            let arm_scope = request.unit_mut().push_scope(
                scope,
                LocalScopeBoundary::PatternArm,
                SyntaxAnchor::from_node(&pattern_syntax),
                pattern_syntax.full_range().start(),
            )?;

            let pattern = request.bind_case_pattern(
                self.path_context_for(arm_scope, self.path_context.access()),
                &pattern_syntax,
                self.error_type,
                PatternBindingMode::Match,
            )?;

            request.activate_pattern_bindings(arm_scope, &pattern)?;

            let guard = match arm.guard_expression() {
                Some(guard) => Some(self.bind_expression(request, arm_scope, Some(&guard))?),
                None => None,
            };

            let body = request.bind_block(arm_scope, &arm.block_expression(), self)?;

            arms.push(BoundMatchArm::new(pattern.pattern(), guard, body));
        }

        let recovered = syntax.is_recovered()
            || request.expression_is_recovered(subject)
            || arms.iter().any(|arm| match_arm_is_recovered(request, *arm));

        let expression = BoundMatchExpression::new(
            request.source_origin(syntax),
            subject,
            arms,
            None,
            recovered,
        );

        self.push(request, BoundExpression::Match(expression))
    }
}

fn match_arm_is_recovered<C>(request: &BinderRequestContext<'_, C>, arm: BoundMatchArm) -> bool
where
    C: BinderFactContext + ?Sized,
{
    request.pattern_is_recovered(arm.pattern())
        || arm
            .guard()
            .is_some_and(|guard| request.expression_is_recovered(guard))
        || request.block_is_recovered(arm.body())
}
