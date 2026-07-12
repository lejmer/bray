use bray_bound_tree::{
    BoundCallableBody, BoundCallableBodyId, BoundNodeOrigin, BoundUnitId, BoundUnitKey,
    CheckedAnonymousCallable, CheckedCallableBody,
};
use bray_checker::ControlFlowChecker;
use bray_syntax::{CallableBodyBlockExpressionSyntax, LambdaExpressionSyntax};

use super::CheckedUnitBindingError;
use super::support::{anchored_descendant, error_type, map_assembly_error, map_binding_error};
use crate::publication::{
    assemble_anonymous_callable, assemble_callable_body, direct_nested_units,
};
use crate::request::{BinderRequestResult, BindingContext};
use crate::{BinderCancellation, BinderFactContext, CheckedUnitComputation};

/// A committed declared callable body awaiting required nested facts and checking.
pub struct PendingCheckedCallableBody {
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    root: BoundCallableBodyId,
}

impl PendingCheckedCallableBody {
    /// Returns directly nested anonymous callable keys in canonical source order.
    pub fn nested_units(&self) -> &[BoundUnitKey] {
        &self.nested_units
    }

    /// Checks and assembles the complete immutable callable body.
    pub fn finish<C, K>(
        self,
        checker: &C,
        cancellation: &K,
    ) -> Result<CheckedUnitComputation<CheckedCallableBody>, CheckedUnitBindingError>
    where
        C: ControlFlowChecker + ?Sized,
        K: BinderCancellation + ?Sized,
    {
        assemble_callable_body(
            self.request,
            self.nested_units,
            checker,
            cancellation,
            self.root,
        )
        .map_err(map_assembly_error)
    }
}

/// A committed anonymous callable awaiting required nested facts and checking.
pub struct PendingCheckedAnonymousCallable {
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    callable: bray_symbols::AnonymousCallableSymbolId,
    root: BoundCallableBodyId,
}

impl PendingCheckedAnonymousCallable {
    /// Returns directly nested anonymous callable keys in canonical source order.
    pub fn nested_units(&self) -> &[BoundUnitKey] {
        &self.nested_units
    }

    /// Checks and assembles the complete immutable anonymous callable.
    pub fn finish<C, K>(
        self,
        checker: &C,
        cancellation: &K,
    ) -> Result<CheckedUnitComputation<CheckedAnonymousCallable>, CheckedUnitBindingError>
    where
        C: ControlFlowChecker + ?Sized,
        K: BinderCancellation + ?Sized,
    {
        assemble_anonymous_callable(
            self.request,
            self.nested_units,
            checker,
            cancellation,
            self.callable,
            self.root,
        )
        .map_err(map_assembly_error)
    }
}

/// Binds one declared callable body into committed task-local state.
pub fn bind_callable_body<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<PendingCheckedCallableBody, CheckedUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let body =
        anchored_descendant::<_, CallableBodyBlockExpressionSyntax>(facts, key.source().syntax())
            .ok_or(CheckedUnitBindingError::MissingSyntax)?;

    let mut request = super::support::request(facts, unit, key, BindingContext::CallableBody)?;
    let root_scope = request.unit().root_scope();
    let path_context = super::support::path_context(&request, root_scope)?;
    let error_type = error_type(facts)?;

    let block = request
        .bind_callable_body_block(
            root_scope,
            &body.block_expression(),
            path_context,
            error_type,
        )
        .map_err(map_binding_error)?;

    let origin = BoundNodeOrigin::source(request.unit().key().source());
    let callable = if request.block_is_recovered(block) {
        BoundCallableBody::error(origin, Some(block))
    } else {
        BoundCallableBody::block(origin, block)
    };

    let root = request
        .unit_mut()
        .tree_mut()
        .push_callable_body(callable)
        .map_err(|_| CheckedUnitBindingError::Construction)?;

    let request = request
        .finish()
        .map_err(|_| CheckedUnitBindingError::Construction)?;

    let nested_units = direct_nested_units(request.unit().key(), request.dependencies());

    Ok(PendingCheckedCallableBody {
        request,
        nested_units,
        root,
    })
}

/// Binds one independently checked anonymous callable into committed task-local state.
pub fn bind_anonymous_callable<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<PendingCheckedAnonymousCallable, CheckedUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let syntax = anchored_descendant::<_, LambdaExpressionSyntax>(facts, key.source().syntax())
        .ok_or(CheckedUnitBindingError::MissingSyntax)?;

    let mut request = super::support::request(facts, unit, key, BindingContext::CallableBody)?;
    let root_scope = request.unit().root_scope();

    let boundary = request
        .bind_anonymous_callable_boundary(root_scope, &syntax)
        .map_err(map_binding_error)?;

    let path_context = super::support::path_context(&request, boundary.scope())?;
    let error_type = error_type(facts)?;
    let body = syntax.callable_body_block_expression().block_expression();

    let block = request
        .bind_callable_body_block(boundary.scope(), &body, path_context, error_type)
        .map_err(map_binding_error)?;

    let origin = BoundNodeOrigin::source(request.unit().key().source());
    let callable_body = if request.block_is_recovered(block) {
        BoundCallableBody::error(origin, Some(block))
    } else {
        BoundCallableBody::block(origin, block)
    };

    let root = request
        .unit_mut()
        .tree_mut()
        .push_callable_body(callable_body)
        .map_err(|_| CheckedUnitBindingError::Construction)?;

    let callable = boundary.callable();
    let request = request
        .finish()
        .map_err(|_| CheckedUnitBindingError::Construction)?;

    let nested_units = direct_nested_units(request.unit().key(), request.dependencies());

    Ok(PendingCheckedAnonymousCallable {
        request,
        nested_units,
        callable,
        root,
    })
}

#[cfg(test)]
mod tests {
    use bray_checker::DefaultControlFlowChecker;

    use super::bind_callable_body;
    use crate::fact::test_support::TestFixture;

    #[test]
    fn production_callable_binding_reaches_checker_finalization() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let facts = fixture.context();
        let (request, _) = crate::binding::request_and_block(&facts);
        let key = request.unit().key().clone();

        let pending = match bind_callable_body(&facts, bray_bound_tree::BoundUnitId::new(40), key) {
            Ok(pending) => pending,
            Err(error) => panic!("source callable body must bind: {error:?}"),
        };

        let computation = match pending.finish(&DefaultControlFlowChecker, &|| false) {
            Ok(computation) => computation,
            Err(error) => panic!("source callable body must finalize: {error:?}"),
        };

        assert_eq!(
            computation.result().value().unit(),
            bray_bound_tree::BoundUnitId::new(40)
        );
    }
}
