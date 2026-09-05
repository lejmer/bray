use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundGeneratorExpression, BoundIterationSource,
    BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId};
use bray_syntax::{
    GeneratorIterationExpressionSyntax, SourceSyntaxNode, SyntaxNodeView, SyntaxWalkRoot,
    syntax_node_view,
};

use super::ExpressionBinder;
use super::support::iteration_source_mode;
use crate::BindingQueryContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind, PatternBindingMode};
use crate::binding::BindingResult;

impl ExpressionBinder {
    pub(super) fn bind_generator_region<C, S>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &S,
        iteration: &GeneratorIterationExpressionSyntax,
        kind: BoundStructuredExpressionKind,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
        S: SourceSyntaxNode + SyntaxWalkRoot,
    {
        let region = SyntaxAnchor::from_node(syntax);
        let target = ControlTarget::new(ControlTargetKind::GeneratorRegion, region);

        binder.push_control_target(target);

        let iteration = self.bind_generator_iteration(binder, scope, iteration);

        if binder.pop_control_target() != Some(target) {
            return Err(crate::binding::BindingError::ControlTargetMismatch);
        }

        self.push_generator_region(binder, syntax_node_view(syntax), iteration?, kind)
    }

    pub(super) fn bind_generator_iteration<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &GeneratorIterationExpressionSyntax,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let source =
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

        let region = SyntaxAnchor::from_node(syntax);
        let target = ControlTarget::new(ControlTargetKind::GeneratorIteration, region);

        binder.push_control_target(target);

        let body = binder.bind_non_yielding_block(pattern_scope, &syntax.block_expression(), self);

        if binder.pop_control_target() != Some(target) {
            return Err(crate::binding::BindingError::ControlTargetMismatch);
        }

        let body = body?;
        let pattern = pattern.pattern();

        let is_recovered = syntax.is_recovered()
            || region.is_recovered()
            || binder.children_are_recovered(&[source], &[body])
            || binder.pattern_is_recovered(pattern);

        let expression = BoundGeneratorExpression::new(
            binder.source_origin(syntax),
            BoundIterationSource::new(source, source_mode),
            pattern,
            body,
            region,
            None,
            is_recovered,
        );

        self.push(binder, BoundExpression::Generator(expression))
    }

    fn push_generator_region<C>(
        &self,
        binder: &mut Binder<'_, C>,
        syntax: SyntaxNodeView<'_>,
        iteration: BoundExpressionId,
        kind: BoundStructuredExpressionKind,
    ) -> BindingResult<BoundExpressionId, C::UpstreamError>
    where
        C: BindingQueryContext + ?Sized,
    {
        let is_recovered = syntax.is_recovered() || binder.expression_is_recovered(iteration);

        let expression = BoundStructuredExpression::new(
            binder.source_origin(&syntax),
            kind,
            [iteration],
            [],
            [],
            None,
            is_recovered,
        );

        self.push(binder, BoundExpression::Structured(expression))
    }
}
