use bray_bound_tree::{
    BoundCallableBody, BoundCallableBodyId, BoundNodeOrigin, BoundUnitId, BoundUnitKey,
};
use bray_symbols::{CallableExecution, CallableSignatureQuery};
use bray_syntax::{CallableBodyBlockExpressionSyntax, LambdaExpressionSyntax};

use super::BoundUnitBindingError;
use super::support::{
    anchored_descendant, error_type, map_assembly_error, map_binding_error, push_callable_inputs,
};
use crate::binder::BinderOutput;
use crate::publication::{
    assemble_anonymous_callable, assemble_callable_body, direct_nested_units,
};
use crate::{BindingQueryContext, BoundUnitComputation, SymbolQueryProvider};

/// A bound declared callable body ready to complete its semantic unit.
pub struct PendingBoundCallableBody {
    output: BinderOutput,
    nested_units: Vec<BoundUnitKey>,
    execution: CallableExecution,
    root: BoundCallableBodyId,
}

impl PendingBoundCallableBody {
    /// Returns directly nested anonymous callable keys in canonical source order.
    pub fn nested_units(&self) -> &[BoundUnitKey] {
        &self.nested_units
    }

    /// Completes and returns the bound callable unit.
    pub fn finish(self) -> Result<BoundUnitComputation, BoundUnitBindingError> {
        assemble_callable_body(self.output, self.nested_units, self.execution, self.root)
            .map_err(map_assembly_error)
    }
}

/// A bound anonymous callable ready to complete its semantic unit.
pub struct PendingBoundAnonymousCallable {
    output: BinderOutput,
    nested_units: Vec<BoundUnitKey>,
    callable: bray_symbols::AnonymousCallableSymbolId,
    execution: CallableExecution,
    root: BoundCallableBodyId,
}

impl PendingBoundAnonymousCallable {
    /// Returns directly nested anonymous callable keys in canonical source order.
    pub fn nested_units(&self) -> &[BoundUnitKey] {
        &self.nested_units
    }

    /// Completes and returns the bound anonymous callable unit.
    pub fn finish(self) -> Result<BoundUnitComputation, BoundUnitBindingError> {
        assemble_anonymous_callable(
            self.output,
            self.nested_units,
            self.callable,
            self.execution,
            self.root,
        )
        .map_err(map_assembly_error)
    }
}

/// Binds one declared callable body.
pub fn bind_callable_body<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<PendingBoundCallableBody, BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let body = anchored_descendant::<_, CallableBodyBlockExpressionSyntax>(
        binding_context,
        key.source().syntax(),
    )
    .ok_or(BoundUnitBindingError::MissingSyntax)?;

    let mut binder = super::support::create_binder(binding_context, unit, key)?;
    let root_scope = binder.unit().root_scope();

    let execution = push_callable_inputs(&mut binder, root_scope)?;

    let path_context = super::support::path_context(&binder, root_scope)?;
    let error_type = error_type(binding_context)?;

    let block = binder
        .bind_callable_body_block(
            root_scope,
            &body.block_expression(),
            path_context,
            error_type,
        )
        .map_err(map_binding_error)?;

    let origin = BoundNodeOrigin::source(binder.unit().key().source());

    let callable = if binder.block_is_recovered(block) {
        BoundCallableBody::error(origin, Some(block))
    } else {
        BoundCallableBody::block(origin, block)
    };

    let root = binder
        .unit_mut()
        .tree_mut()
        .push_callable_body(callable)
        .map_err(|_| BoundUnitBindingError::Construction)?;

    let output = binder
        .finish()
        .map_err(|_| BoundUnitBindingError::Construction)?;

    let nested_units = direct_nested_units(output.unit().key(), output.dependencies());

    Ok(PendingBoundCallableBody {
        output,
        nested_units,
        execution,
        root,
    })
}

/// Binds one independently analyzed anonymous callable.
pub fn bind_anonymous_callable<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<PendingBoundAnonymousCallable, BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
{
    let syntax =
        anchored_descendant::<_, LambdaExpressionSyntax>(binding_context, key.source().syntax())
            .ok_or(BoundUnitBindingError::MissingSyntax)?;

    let mut binder = super::support::create_binder(binding_context, unit, key)?;
    let root_scope = binder.unit().root_scope();

    let boundary = binder
        .bind_anonymous_callable_boundary(root_scope, &syntax)
        .map_err(map_binding_error)?;

    let path_context = super::support::path_context(&binder, boundary.scope())?;
    let error_type = error_type(binding_context)?;
    let body = syntax.callable_body_block_expression().block_expression();

    let block = binder
        .bind_callable_body_block(boundary.scope(), &body, path_context, error_type)
        .map_err(map_binding_error)?;

    let origin = BoundNodeOrigin::source(binder.unit().key().source());

    let callable_body = if binder.block_is_recovered(block) {
        BoundCallableBody::error(origin, Some(block))
    } else {
        BoundCallableBody::block(origin, block)
    };

    let root = binder
        .unit_mut()
        .tree_mut()
        .push_callable_body(callable_body)
        .map_err(|_| BoundUnitBindingError::Construction)?;

    let callable = boundary.callable();

    let execution = if syntax.callable_modifiers().async_token().is_some() {
        CallableExecution::Asynchronous
    } else {
        CallableExecution::Synchronous
    };

    let output = binder
        .finish()
        .map_err(|_| BoundUnitBindingError::Construction)?;

    let nested_units = direct_nested_units(output.unit().key(), output.dependencies());

    Ok(PendingBoundAnonymousCallable {
        output,
        nested_units,
        callable,
        execution,
        root,
    })
}

#[cfg(test)]
mod tests {
    use super::bind_callable_body;
    use crate::BindingQueryContext;
    use crate::query::test_support::TestFixture;

    #[test]
    fn production_callable_binding_publishes_a_bound_unit() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let binding_context = fixture.context();

        let (binder, _) = crate::binding::binder_and_block(&binding_context);

        let key = binder.unit().key().clone();

        let pending = match bind_callable_body(
            &binding_context,
            bray_bound_tree::BoundUnitId::new(40),
            key,
        ) {
            Ok(pending) => pending,
            Err(error) => panic!("source callable body must bind: {error:?}"),
        };

        let computation = match pending.finish() {
            Ok(computation) => computation,
            Err(error) => panic!("source callable body must finalize: {error:?}"),
        };

        let entry = match crate::semantic_unit_context(
            binding_context.symbols(),
            computation.result().value(),
        ) {
            Ok(entry) => entry,
            Err(error) => {
                panic!("source callable body must establish checker entry: {error:?}")
            }
        };

        assert_eq!(
            computation.result().value().unit(),
            bray_bound_tree::BoundUnitId::new(40)
        );

        assert_eq!(entry.kind(), bray_bound_tree::BoundUnitKind::CallableBody);
        assert_eq!(entry.key(), computation.result().value().key());
    }
}
