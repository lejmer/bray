use bray_bound_tree::{
    BoundCallableBody, BoundNodeOrigin, BoundUnitId, BoundUnitKey, BoundUnitRoot,
};
use bray_symbols::{CallableExecution, CallableSignatureQuery};
use bray_syntax::{CallableBodyBlockExpressionSyntax, LambdaExpressionSyntax};

use super::BoundUnitBindingError;
use super::support::{
    anchored_descendant, error_type, map_binding_error, missing_syntax, push_callable_inputs,
};
use crate::publication::assemble_bound_unit;
use crate::{BindingQueryContext, BoundUnitComputation, SymbolQueryProvider};

/// Binds one declared callable body into its completed semantic unit.
pub fn bind_callable_body<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<BoundUnitComputation, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let body = anchored_descendant::<_, CallableBodyBlockExpressionSyntax>(
        binding_context,
        key.source().syntax(),
    )
    .ok_or_else(|| missing_syntax(&key))?;

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
        .map_err(crate::unit::BoundUnitConstructionError::from)
        .map_err(BoundUnitBindingError::Construction)?;

    let output = binder.finish();

    Ok(assemble_bound_unit(
        output,
        BoundUnitRoot::CallableBody {
            execution,
            body: root,
        },
    ))
}

/// Binds one independently analyzed anonymous callable.
pub fn bind_anonymous_callable<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<BoundUnitComputation, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let syntax =
        anchored_descendant::<_, LambdaExpressionSyntax>(binding_context, key.source().syntax())
            .ok_or_else(|| missing_syntax(&key))?;

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
        .map_err(crate::unit::BoundUnitConstructionError::from)
        .map_err(BoundUnitBindingError::Construction)?;

    let callable = boundary.callable();

    let execution = if syntax.callable_modifiers().async_token().is_some() {
        CallableExecution::Asynchronous
    } else {
        CallableExecution::Synchronous
    };

    let output = binder.finish();

    Ok(assemble_bound_unit(
        output,
        BoundUnitRoot::AnonymousCallable {
            callable,
            execution,
            body: root,
        },
    ))
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

        let computation = match bind_callable_body(
            &binding_context,
            bray_bound_tree::BoundUnitId::new(40),
            key,
        ) {
            Ok(computation) => computation,
            Err(error) => panic!("source callable body must bind: {error:?}"),
        };

        let entry =
            crate::semantic_unit_context(binding_context.symbols(), computation.result().value());

        assert_eq!(
            computation.result().value().unit(),
            bray_bound_tree::BoundUnitId::new(40)
        );

        assert_eq!(entry.kind(), bray_bound_tree::BoundUnitKind::CallableBody);
        assert_eq!(entry.key(), computation.result().value().key());
    }

    #[test]
    #[should_panic(expected = "must have an owner in its symbol graph")]
    fn missing_semantic_context_owners_expose_the_producer_bug() {
        let primary = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let binding_context = primary.context();

        let (binder, _) = crate::binding::binder_and_block(&binding_context);

        let key = binder.unit().key().clone();

        let computation = match bind_callable_body(
            &binding_context,
            bray_bound_tree::BoundUnitId::new(41),
            key,
        ) {
            Ok(computation) => computation,
            Err(error) => panic!("source callable body must bind: {error:?}"),
        };

        let foreign = TestFixture::from_source(concat!(
            "module other;\n",
            "const size: i32 = 1;\n",
            "func other()\n",
            "{\n",
            "}\n",
        ));

        crate::semantic_unit_context(foreign.context().symbols(), computation.result().value());
    }
}
