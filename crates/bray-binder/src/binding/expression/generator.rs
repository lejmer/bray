use bray_bound_tree::{BoundExpression, BoundExpressionId, BoundGeneratorExpression};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeBoundary, LocalScopeId};
use bray_syntax::GeneratorIterationExpressionSyntax;

use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binding::BindingResult;
use crate::request::{BinderRequestContext, ControlTarget, ControlTargetKind, PatternBindingMode};

impl ExpressionBinder {
    pub(super) fn bind_generator_iteration<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &GeneratorIterationExpressionSyntax,
        region: SyntaxAnchor,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let source = self.bind_expression(
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

        let target = ControlTarget::new(ControlTargetKind::Generator, region, None);

        request.push_control_target(target);

        let body = request.bind_block(pattern_scope, &syntax.block_expression(), self);

        if request.pop_control_target() != Some(target) {
            return Err(crate::binding::BindingError::ControlTargetMismatch);
        }

        let body = body?;
        let pattern = pattern.pattern();
        let is_recovered = syntax.is_recovered()
            || region.is_recovered()
            || request.expression_is_recovered(source)
            || request.pattern_is_recovered(pattern)
            || request.block_is_recovered(body);

        let expression = BoundGeneratorExpression::new(
            request.source_origin(syntax),
            source,
            pattern,
            body,
            region,
            None,
            is_recovered,
        );

        self.push(request, BoundExpression::Generator(expression))
    }
}
