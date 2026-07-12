use std::sync::Arc;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundSpawnExpression, BoundSpawnInput, BoundSpawnMode,
};
use bray_symbols::LocalScopeId;
use bray_syntax::SpawnExpressionSyntax;

use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binding::{BindingError, BindingResult};
use crate::request::BinderRequestContext;

impl ExpressionBinder {
    pub(super) fn bind_spawn<C>(
        &mut self,
        request: &mut BinderRequestContext<'_, C>,
        scope: LocalScopeId,
        syntax: &SpawnExpressionSyntax,
    ) -> BindingResult<BoundExpressionId>
    where
        C: BinderFactContext + ?Sized,
    {
        let (mode, input) = if syntax.thread_keyword().is_some() {
            let Some(callee) = syntax.access_expression() else {
                return Err(BindingError::UnsupportedSyntax);
            };

            let Some(argument_list) = syntax.argument_list() else {
                return Err(BindingError::UnsupportedSyntax);
            };

            let callee = self.bind_access(request, scope, &callee)?;
            let arguments = self.bind_arguments(request, scope, &argument_list)?;

            (
                BoundSpawnMode::Thread,
                BoundSpawnInput::Thread {
                    callee,
                    arguments: Arc::from(arguments),
                },
            )
        } else {
            let Some(task) = syntax.expression() else {
                return Err(BindingError::UnsupportedSyntax);
            };

            let task = self.bind_expression(request, scope, Some(&task))?;
            let mode = if syntax.detached_keyword().is_some() {
                BoundSpawnMode::DetachedTask
            } else {
                BoundSpawnMode::Task
            };

            (mode, BoundSpawnInput::Task(task))
        };

        let is_recovered = syntax.is_recovered() || spawn_input_is_recovered(request, &input);

        let expression = BoundSpawnExpression::new(
            request.source_origin(syntax),
            mode,
            input,
            None,
            is_recovered,
        );

        self.push(request, BoundExpression::Spawn(expression))
    }
}

fn spawn_input_is_recovered<C>(
    request: &BinderRequestContext<'_, C>,
    input: &BoundSpawnInput,
) -> bool
where
    C: BinderFactContext + ?Sized,
{
    match input {
        BoundSpawnInput::Task(expression) => request.expression_is_recovered(*expression),
        BoundSpawnInput::Thread { callee, arguments } => {
            request.expression_is_recovered(*callee)
                || arguments.iter().any(|argument| {
                    argument.is_recovered()
                        || request.expression_is_recovered(argument.expression())
                })
        }
    }
}
