use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundGeneratorExpression, BoundIterationSource,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId};
use bray_syntax::GeneratorIterationExpressionSyntax;

use super::ExpressionBinder;
use super::support::iteration_source_mode;
use crate::BinderFactContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind, PatternBindingMode};
use crate::binding::BindingResult;

impl ExpressionBinder {
    pub(super) fn bind_generator_iteration<C>(
        &mut self,
        binder: &mut Binder<'_, C>,
        scope: LocalScopeId,
        syntax: &GeneratorIterationExpressionSyntax,
        region: SyntaxAnchor,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
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

        let target = ControlTarget::new(ControlTargetKind::Generator, region, None);

        binder.push_control_target(target);

        let body = binder.bind_block(pattern_scope, &syntax.block_expression(), self);

        if binder.pop_control_target() != Some(target) {
            return Err(crate::binding::BindingError::ControlTargetMismatch);
        }

        let body = body?;
        let pattern = pattern.pattern();

        let is_recovered = syntax.is_recovered()
            || region.is_recovered()
            || binder.expression_is_recovered(source)
            || binder.pattern_is_recovered(pattern)
            || binder.block_is_recovered(body);

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
}
