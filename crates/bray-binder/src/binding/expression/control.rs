use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpression, BoundStructuredExpressionKind,
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
        let recovered = syntax.is_recovered()
            || request.expression_is_recovered(iteration)
            || request.pattern_is_recovered(pattern_id)
            || blocks
                .iter()
                .any(|block| request.block_is_recovered(*block));

        let expression = BoundStructuredExpression::new(
            request.source_origin(syntax),
            BoundStructuredExpressionKind::For,
            [iteration],
            blocks,
            [pattern_id],
            self.error_type,
            recovered,
        );

        self.push(request, BoundExpression::Structured(expression))
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

        let mut operands = vec![subject];
        let mut blocks = Vec::new();
        let mut patterns = Vec::new();

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
            patterns.push(pattern.pattern());

            if let Some(guard) = arm.guard_expression() {
                operands.push(self.bind_expression(request, arm_scope, Some(&guard))?);
            }

            blocks.push(request.bind_block(arm_scope, &arm.block_expression(), self)?);
        }

        let recovered = syntax.is_recovered()
            || operands
                .iter()
                .any(|operand| request.expression_is_recovered(*operand))
            || patterns
                .iter()
                .any(|pattern| request.pattern_is_recovered(*pattern))
            || blocks
                .iter()
                .any(|block| request.block_is_recovered(*block));

        let expression = BoundStructuredExpression::new(
            request.source_origin(syntax),
            BoundStructuredExpressionKind::Match,
            operands,
            blocks,
            patterns,
            self.error_type,
            recovered,
        );

        self.push(request, BoundExpression::Structured(expression))
    }
}
