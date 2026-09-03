use bray_bound_tree::{
    BoundBinaryExpression, BoundBlock, BoundBlockId, BoundExpression, BoundExpressionId,
    BoundOperator, BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId};
use bray_syntax::{
    ConditionalExpressionSyntax, ExpressionSyntax, SyntaxKind, WhileExpressionSyntax,
};

use super::ExpressionBinder;
use crate::BindingQueryContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind, PatternBindingMode};
use crate::binding::{BindingError, BindingResult};

impl ExpressionBinder {
    pub(super) fn bind_pattern_condition<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        body_scope: LocalScopeId,
        syntax: &ExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let subject = self.bind_expression(binder, scope, syntax.expressions().next().as_ref())?;

        let Some(pattern_syntax) = syntax.case_patterns().next() else {
            return self.push_error(binder, Some(syntax));
        };

        let subject_type = binder
            .unit_view()
            .expression(subject)
            .and_then(BoundExpression::ty)
            .unwrap_or(self.error_type);

        let pattern = binder.bind_case_pattern(
            self.path_context_for(body_scope, self.path_context.access()),
            &pattern_syntax,
            subject_type,
            self.error_type,
            PatternBindingMode::MatchObserve,
        )?;

        let kind = if syntax.let_keyword().is_some() {
            binder.activate_pattern_bindings(body_scope, &pattern)?;

            BoundStructuredExpressionKind::PatternBinding
        } else {
            BoundStructuredExpressionKind::PatternTest
        };

        let pattern = pattern.pattern();

        let recovered = syntax.is_recovered()
            || binder.expression_is_recovered(subject)
            || binder.pattern_is_recovered(pattern);

        let lifetime = if kind == BoundStructuredExpressionKind::PatternTest {
            Some(Self::push_condition_lifetime(binder, syntax, recovered)?)
        } else {
            None
        };

        self.push(
            binder,
            BoundExpression::Structured(BoundStructuredExpression::new(
                binder.source_origin(syntax),
                kind,
                [subject],
                lifetime,
                [pattern],
                None,
                recovered,
            )),
        )
    }

    fn bind_condition<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<(BoundExpressionId, LocalScopeId), C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let (condition, body_scope) = self.bind_condition_operand(binder, scope, syntax)?;

        if body_scope == scope {
            return Ok((condition, body_scope));
        }

        let syntax = syntax.ok_or(BindingError::UnsupportedSyntax)?;
        let recovered = syntax.is_recovered() || binder.expression_is_recovered(condition);
        let lifetime = Self::push_condition_lifetime(binder, syntax, recovered)?;

        let condition = self.push(
            binder,
            BoundExpression::Structured(BoundStructuredExpression::new(
                binder.source_origin(syntax),
                BoundStructuredExpressionKind::Condition,
                [condition],
                [lifetime],
                [],
                None,
                recovered,
            )),
        )?;

        Ok((condition, body_scope))
    }

    fn bind_condition_operand<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: Option<&ExpressionSyntax>,
    ) -> BindingResult<(BoundExpressionId, LocalScopeId), C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        if let Some(syntax) = syntax
            && syntax
                .operator_token()
                .is_some_and(|token| token.kind() == SyntaxKind::AmpersandAmpersandToken)
        {
            let mut operands = Vec::new();
            let mut body_scope = scope;

            for operand in syntax.expressions() {
                let (operand, next_scope) =
                    self.bind_condition_operand(binder, body_scope, Some(&operand))?;

                operands.push(operand);
                body_scope = next_scope;
            }

            let recovered = syntax.is_recovered() || binder.children_are_recovered(&operands, &[]);

            let condition = self.push(
                binder,
                BoundExpression::Binary(BoundBinaryExpression::new(
                    binder.source_origin(syntax),
                    BoundOperator::LogicalAnd,
                    operands,
                    None,
                    recovered,
                )),
            )?;

            return Ok((condition, body_scope));
        }

        let Some(syntax) = syntax.filter(|syntax| syntax.let_keyword().is_some()) else {
            return Ok((self.bind_expression(binder, scope, syntax)?, scope));
        };

        let body_scope = binder.unit_mut().push_scope(
            scope,
            LocalScopeBoundary::PatternArm,
            SyntaxAnchor::from_node(syntax),
            syntax.full_range().start(),
        )?;

        let condition = self.bind_pattern_condition(binder, scope, body_scope, syntax)?;

        Ok((condition, body_scope))
    }

    fn push_condition_lifetime<C>(
        binder: &mut Binder<'_, C>,
        syntax: &ExpressionSyntax,
        recovered: bool,
    ) -> BindingResult<BoundBlockId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let lifetime = BoundBlock::new(binder.source_origin(syntax), [], recovered);

        Ok(binder
            .unit_mut()
            .tree_mut()
            .push_block(lifetime)
            .map_err(crate::unit::BoundUnitConstructionError::from)?)
    }

    pub(super) fn bind_conditional<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &ConditionalExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let mut conditions = Vec::new();
        let mut blocks = Vec::new();

        // The traversal owns a shared syntax handle while moving through else clauses.
        let mut current = Some(syntax.clone());

        while let Some(conditional) = current {
            let (condition, body_scope) =
                self.bind_condition(binder, scope, conditional.condition_expression().as_ref())?;

            conditions.push(condition);
            blocks.push(binder.bind_block(body_scope, &conditional.block_expression(), self)?);

            let clause = conditional.conditional_else();

            if let Some(block) = clause.as_ref().and_then(|clause| clause.block_expression()) {
                blocks.push(binder.bind_block(scope, &block, self)?);
            }

            current = clause.and_then(|clause| clause.conditional_expression());
        }

        let recovered =
            syntax.is_recovered() || binder.children_are_recovered(&conditions, &blocks);

        self.push(
            binder,
            BoundExpression::Structured(BoundStructuredExpression::new(
                binder.source_origin(syntax),
                BoundStructuredExpressionKind::Conditional,
                conditions,
                blocks,
                [],
                None,
                recovered,
            )),
        )
    }

    pub(super) fn bind_while<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &WhileExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let target = ControlTarget::new(ControlTargetKind::Loop, SyntaxAnchor::from_node(syntax));

        binder.push_control_target(target);

        let result = (|| {
            let (condition, body_scope) =
                self.bind_condition(binder, scope, syntax.condition_expression().as_ref())?;

            let mut blocks = Vec::new();

            for (index, block) in syntax.block_expressions().enumerate() {
                let block_scope = if index == 0 { body_scope } else { scope };

                blocks.push(binder.bind_non_yielding_block(block_scope, &block, self)?);
            }

            let recovered =
                syntax.is_recovered() || binder.children_are_recovered(&[condition], &blocks);

            self.push(
                binder,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    binder.source_origin(syntax),
                    BoundStructuredExpressionKind::While,
                    [condition],
                    blocks,
                    [],
                    None,
                    recovered,
                )),
            )
        })();

        if binder.pop_control_target() != Some(target) {
            return Err(BindingError::ControlTargetMismatch);
        }

        result
    }
}
