use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundForExpression, BoundIterationSource, BoundMatchArm,
    BoundMatchExpression,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId};
use bray_syntax::{ForExpressionSyntax, MatchExpressionSyntax};

use super::ExpressionBinder;
use super::support::iteration_source_mode;
use crate::BinderFactContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind, PatternBindingMode};
use crate::binding::BindingResult;

impl ExpressionBinder {
    pub(super) fn bind_for_expression<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &ForExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let iteration =
            self.bind_expression(binder, scope, Some(&syntax.iteration_source().expression()))?;

        let source_mode = iteration_source_mode(&syntax.iteration_source());

        let pattern_syntax = syntax.irrefutable_pattern();

        let pattern_scope = binder.unit_mut().push_scope(
            scope,
            LocalScopeBoundary::PatternArm,
            SyntaxAnchor::from_node(&pattern_syntax),
            pattern_syntax.full_range().start(),
        )?;

        let pattern = binder.bind_irrefutable_pattern(
            self.path_context_for(pattern_scope, self.path_context.access()),
            &pattern_syntax,
            self.error_type,
            self.error_type,
            PatternBindingMode::Declaration,
        )?;

        binder.activate_pattern_bindings(pattern_scope, &pattern)?;

        let target = ControlTarget::new(ControlTargetKind::Loop, SyntaxAnchor::from_node(syntax));

        binder.push_control_target(target);

        let mut blocks = Vec::new();
        let mut failure = None;

        for (index, block) in syntax.block_expressions().enumerate() {
            let block_scope = if index == 0 { pattern_scope } else { scope };

            match binder.bind_non_yielding_block(block_scope, &block, self) {
                Ok(block) => blocks.push(block),
                Err(error) => {
                    failure = Some(error);

                    break;
                }
            }
        }

        if binder.pop_control_target() != Some(target) {
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
            || binder.expression_is_recovered(iteration)
            || binder.pattern_is_recovered(pattern_id)
            || binder.block_is_recovered(body)
            || else_body.is_some_and(|block| binder.block_is_recovered(block));

        let expression = BoundForExpression::new(
            binder.source_origin(syntax),
            BoundIterationSource::new(iteration, source_mode),
            pattern_id,
            body,
            else_body,
            None,
            recovered,
        );

        self.push(binder, BoundExpression::For(expression))
    }

    pub(super) fn bind_match_expression<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &MatchExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let subject =
            self.bind_expression(binder, scope, Some(&syntax.match_subject().expression()))?;

        let subject_type = binder
            .unit_view()
            .expression(subject)
            .and_then(bray_bound_tree::BoundExpression::ty)
            .unwrap_or(self.error_type);

        let mut arms = Vec::new();

        for arm in syntax.match_body().match_arms() {
            binder.check_cancellation()?;

            let pattern_syntax = arm.case_pattern();

            let arm_scope = binder.unit_mut().push_scope(
                scope,
                LocalScopeBoundary::PatternArm,
                SyntaxAnchor::from_node(&pattern_syntax),
                pattern_syntax.full_range().start(),
            )?;

            let pattern = binder.bind_case_pattern(
                self.path_context_for(arm_scope, self.path_context.access()),
                &pattern_syntax,
                subject_type,
                self.error_type,
                if syntax.match_subject().consume_keyword().is_some() {
                    PatternBindingMode::MatchConsume
                } else {
                    PatternBindingMode::MatchObserve
                },
            )?;

            binder.activate_pattern_bindings(arm_scope, &pattern)?;

            let guard = match arm.guard_expression() {
                Some(guard) => Some(self.bind_expression(binder, arm_scope, Some(&guard))?),
                None => None,
            };

            let body = binder.bind_block(arm_scope, &arm.block_expression(), self)?;

            arms.push(BoundMatchArm::new(pattern.pattern(), guard, body));
        }

        let recovered = syntax.is_recovered()
            || binder.expression_is_recovered(subject)
            || arms.iter().any(|arm| match_arm_is_recovered(binder, *arm));

        let expression =
            BoundMatchExpression::new(binder.source_origin(syntax), subject, arms, None, recovered);

        self.push(binder, BoundExpression::Match(expression))
    }
}

fn match_arm_is_recovered<C>(binder: &Binder<'_, C>, arm: BoundMatchArm) -> bool
where
    C: BinderFactContext + ?Sized,
{
    binder.pattern_is_recovered(arm.pattern())
        || arm
            .guard()
            .is_some_and(|guard| binder.expression_is_recovered(guard))
        || binder.block_is_recovered(arm.body())
}
