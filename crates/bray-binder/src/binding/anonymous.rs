use bray_bound_tree::BoundSourceAnchor;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalScopeId, SymbolOrdinal};
use bray_syntax::{LambdaExpressionSyntax, SourceSyntaxNode};

use super::name::symbol_name;
use super::{BindingError, BindingResult};
use crate::BinderFactContext;
use crate::binder::{Binder, BinderDependency};
use crate::unit::AnonymousCallableBoundary;

impl<C> Binder<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn bind_anonymous_callable_reference(
        &mut self,
        syntax: &LambdaExpressionSyntax,
    ) -> BindingResult<bray_bound_tree::BoundUnitKey> {
        self.check_cancellation()?;

        let source = self.anonymous_source(syntax);
        let unit = self.unit().nested_anonymous_callable_key(source)?;

        // The dependency set and caller retain the same Arc-backed immutable unit key.
        self.record_dependency(BinderDependency::Unit(unit.clone()));

        Ok(unit)
    }

    pub(crate) fn bind_anonymous_callable_boundary(
        &mut self,
        introduction_scope: LocalScopeId,
        syntax: &LambdaExpressionSyntax,
    ) -> BindingResult<AnonymousCallableBoundary> {
        self.bind_transaction(|binder| {
            binder.bind_anonymous_callable_boundary_transaction(introduction_scope, syntax)
        })
    }

    fn bind_anonymous_callable_boundary_transaction(
        &mut self,
        introduction_scope: LocalScopeId,
        syntax: &LambdaExpressionSyntax,
    ) -> BindingResult<AnonymousCallableBoundary> {
        self.check_cancellation()?;

        let source = self.anonymous_source(syntax);

        let boundary = self.unit_mut().push_root_anonymous_callable(
            introduction_scope,
            source,
            syntax.full_range().start(),
            None,
            syntax.is_recovered(),
        )?;

        for (index, parameter) in syntax.parameter_list().parameters().enumerate() {
            self.check_cancellation()?;

            let ordinal = u32::try_from(index)
                .map(SymbolOrdinal::new)
                .map_err(|_| BindingError::IdentityCapacityExceeded)?;

            let token = parameter.identifier_token();

            let Some(name) = symbol_name(parameter.source(), &token) else {
                continue;
            };

            self.unit_mut().push_anonymous_parameter(
                &boundary,
                name,
                [SyntaxAnchor::from_node(&parameter)],
                ordinal,
                parameter.is_recovered() || token.is_missing(),
            )?;
        }

        Ok(boundary)
    }

    fn anonymous_source(&self, syntax: &LambdaExpressionSyntax) -> BoundSourceAnchor {
        BoundSourceAnchor::new(
            SyntaxAnchor::from_node(syntax),
            self.unit().key().source().source_version(),
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitKind;
    use bray_declarations::SyntaxAnchor;
    use bray_symbols::{LocalSymbolRegionId, MemberLookupResult, SymbolName, SymbolOrdinal};
    use bray_syntax::LambdaExpressionSyntax;

    use crate::BinderFactContext;
    use crate::binder::Binder;
    use crate::fact::test_support::TestFixture;
    use crate::unit::BoundUnitLocalBuilder;

    #[test]
    fn anonymous_boundaries_publish_parameters_and_nested_unit_dependencies() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda(value: i32)\n",
            "    {\n",
            "    };\n",
            "}",
        ));

        let facts = fixture.context();

        let (mut binder, block) = crate::binding::test_support::binder_and_block(&facts);

        let Some(lambda) =
            crate::binding::test_support::first_descendant::<LambdaExpressionSyntax>(&block)
        else {
            panic!("test block must contain a lambda expression");
        };

        let root = binder.unit().root_scope();

        let name = match SymbolName::try_new("captured") {
            Some(name) => name,
            None => panic!("test local name must be valid"),
        };

        let captured = match binder.unit_mut().push_binding(
            root,
            name,
            [SyntaxAnchor::from_node(&lambda)],
            Some(SymbolOrdinal::new(0)),
            false,
        ) {
            Ok(binding) => binding,
            Err(error) => panic!("test enclosing binding must build: {error:?}"),
        };

        if let Err(error) = binder.unit_mut().activate_local(root, captured) {
            panic!("test enclosing binding must activate: {error:?}");
        }

        let nested_key = match binder.bind_anonymous_callable_reference(&lambda) {
            Ok(key) => key,
            Err(error) => panic!("valid lambda reference must bind: {error:?}"),
        };

        assert_eq!(nested_key.kind(), BoundUnitKind::AnonymousCallable);

        let outer = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("anonymous reference must freeze: {error:?}"),
        };

        assert_eq!(
            outer.dependencies(),
            &[crate::BinderDependency::Unit(nested_key.clone())]
        );

        let nested_unit = match BoundUnitLocalBuilder::new(
            bray_bound_tree::BoundUnitId::new(31),
            nested_key.clone(),
            LocalSymbolRegionId::new(31),
            lambda.full_range().start(),
        ) {
            Ok(unit) => unit,
            Err(error) => panic!("nested lambda unit must build: {error:?}"),
        };

        let mut binder = Binder::new(&facts, nested_unit);
        let root = binder.unit().root_scope();

        let boundary = match binder.bind_anonymous_callable_boundary(root, &lambda) {
            Ok(boundary) => boundary,
            Err(error) => panic!("valid lambda boundary must bind: {error:?}"),
        };

        assert_eq!(boundary.unit(), &nested_key);

        let Some(module) = facts.symbols().modules().first() else {
            panic!("test graph must contain a module");
        };

        let capture_lookup = crate::lookup::lookup_unqualified_name(
            binder.unit(),
            facts.symbols(),
            boundary.scope(),
            Some(module.id()),
            "captured",
            crate::lookup::NameAccess::Internal,
        );

        assert_eq!(capture_lookup, MemberLookupResult::NotFound);

        let parameter_lookup = crate::lookup::lookup_unqualified_name(
            binder.unit(),
            facts.symbols(),
            boundary.scope(),
            Some(module.id()),
            "value",
            crate::lookup::NameAccess::Internal,
        );

        assert!(matches!(
            parameter_lookup,
            MemberLookupResult::Found(crate::lookup::ResolvedName::Local(_))
        ));

        let nested = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("anonymous boundary must freeze: {error:?}"),
        };

        let Some(callable) = nested
            .unit()
            .local_symbols()
            .anonymous_callable(boundary.callable())
        else {
            panic!("anonymous callable identity must be published");
        };

        assert_eq!(callable.parameters().len(), 1);

        assert_eq!(
            nested.unit().local_symbols().key().role(),
            bray_symbols::LocalSymbolRegionRole::AnonymousCallable
        );
    }
}
