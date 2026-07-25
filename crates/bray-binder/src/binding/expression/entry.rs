use bray_bound_tree::BoundBlockId;
use bray_symbols::{LocalScopeId, TypeId};

use super::ExpressionBinder;
use crate::BinderFactContext;
use crate::binder::{Binder, ControlTarget, ControlTargetKind};
use crate::binding::BindingResult;
use crate::lookup::PathBindingContext;

impl<C> Binder<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn bind_callable_body_block(
        &mut self,
        parent_scope: LocalScopeId,
        syntax: &bray_syntax::BlockExpressionSyntax,
        path_context: PathBindingContext,
        error_type: TypeId,
    ) -> BindingResult<BoundBlockId> {
        let mut binder = ExpressionBinder::new(path_context, error_type);

        let target = ControlTarget::new(
            ControlTargetKind::Callable,
            self.unit().key().source().syntax(),
        );

        self.push_control_target(target);

        let result = self.bind_non_yielding_block(parent_scope, syntax, &mut binder);

        if self.pop_control_target() != Some(target) {
            return Err(crate::binding::BindingError::ControlTargetMismatch);
        }

        result
    }
}
