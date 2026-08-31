use std::sync::Arc;

use bray_symbols::{SymbolQueryContract, SymbolQueryRequest};

use crate::{BindingQueryContext, BindingQueryResult};

/// Owns the one upstream failure type shared by every symbol query contract a provider supports.
pub trait SymbolQueryErrorProvider: Send + Sync {
    /// Exact failures owned by the coordinating query layer.
    type UpstreamError;
}

/// Provides shared immutable access to one category of symbol-facing semantic query.
///
/// Implement this trait separately for each supported [`SymbolQueryContract`]. Equivalent completed
/// requests must return equivalent results.
pub trait SymbolQueryProvider<Contract>: SymbolQueryErrorProvider
where
    Contract: SymbolQueryContract,
{
    /// Resolves the semantic value and diagnostics for the typed request.
    fn resolve_symbol_query(
        &self,
        request: SymbolQueryRequest<Contract>,
    ) -> BindingQueryResult<
        Arc<
            bray_diagnostics::DiagnosticResult<
                <Contract as bray_symbols::SymbolQueryContract>::Value,
            >,
        >,
        <Self as SymbolQueryErrorProvider>::UpstreamError,
    >;
}

/// Evaluates one binding-dependent symbol query with its diagnostics.
pub trait BindingSymbolQueryEvaluator<Contract, Context>: Send + Sync
where
    Contract: SymbolQueryContract,
    Context: BindingQueryContext + ?Sized,
{
    /// Evaluates the query selected by `request` from injected read-only dependencies.
    fn evaluate_symbol_query(
        &self,
        context: &Context,
        request: SymbolQueryRequest<Contract>,
    ) -> BindingQueryResult<
        bray_diagnostics::DiagnosticResult<<Contract as bray_symbols::SymbolQueryContract>::Value>,
        Context::UpstreamError,
    >;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use bray_diagnostics::DiagnosticResult;
    use bray_symbols::{
        ConstantDeclaredTypeQuery, ConstantSymbolId, SymbolId, SymbolQueryRequest,
        TypeExpressionTemplate,
    };

    use super::{BindingSymbolQueryEvaluator, SymbolQueryProvider};
    use crate::query::test_support::{TestContext, TestFixture};
    use crate::{BindingQueryContext, BindingQueryError, BindingQueryResult};

    struct DeclaredTypeBinder;

    impl BindingSymbolQueryEvaluator<ConstantDeclaredTypeQuery, TestContext<'_>>
        for DeclaredTypeBinder
    {
        fn evaluate_symbol_query(
            &self,
            context: &TestContext<'_>,
            request: SymbolQueryRequest<ConstantDeclaredTypeQuery>,
        ) -> BindingQueryResult<DiagnosticResult<TypeExpressionTemplate>> {
            if context.is_cancelled() {
                return Err(BindingQueryError::Cancelled);
            }

            let Some(_) = context.symbols().constant(request.owner()) else {
                return Err(BindingQueryError::DependencyUnavailable);
            };

            Ok(DiagnosticResult::without_diagnostics(
                context.symbol_semantics().result.value().clone(),
            ))
        }
    }

    #[test]
    fn origin_neutral_symbol_query_reads_are_repeatable_and_concurrent() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let request = SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(fixture.constant);

        let first = match context.symbol_semantics().resolve_symbol_query(request) {
            Ok(result) => result,
            Err(error) => panic!("test symbol query should succeed: {error:?}"),
        };

        let second = match context.symbol_semantics().resolve_symbol_query(request) {
            Ok(result) => result,
            Err(error) => panic!("test symbol query should remain available: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        thread::scope(|scope| {
            let handles = (0..4)
                .map(|_| {
                    scope.spawn(
                        || match context.symbol_semantics().resolve_symbol_query(request) {
                            Ok(result) => result,
                            Err(error) => panic!("concurrent query read should succeed: {error:?}"),
                        },
                    )
                })
                .collect::<Vec<_>>();

            for handle in handles {
                let concurrent = match handle.join() {
                    Ok(result) => result,
                    Err(error) => panic!("query reader should not panic: {error:?}"),
                };

                assert!(Arc::ptr_eq(&first, &concurrent));
            }
        });
    }

    #[test]
    fn symbol_query_provider_keeps_unavailable_owners_outside_semantic_results() {
        let fixture = TestFixture::new();
        let context = fixture.context();

        let unknown = SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(
            ConstantSymbolId::from_symbol_id(SymbolId::new(99)),
        );

        assert_eq!(
            context.symbol_semantics().resolve_symbol_query(unknown),
            Err(BindingQueryError::DependencyUnavailable)
        );
    }

    #[test]
    fn binding_computation_is_deterministic_and_recovers_from_unknown_owners() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let request = SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(fixture.constant);

        let first = DeclaredTypeBinder.evaluate_symbol_query(&context, request);
        let second = DeclaredTypeBinder.evaluate_symbol_query(&context, request);

        assert_eq!(first, second);

        assert_eq!(
            first,
            Ok(DiagnosticResult::without_diagnostics(
                TypeExpressionTemplate::Resolved(fixture.declared_type)
            ))
        );

        let unknown = SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(
            ConstantSymbolId::from_symbol_id(SymbolId::new(99)),
        );

        assert_eq!(
            DeclaredTypeBinder.evaluate_symbol_query(&context, unknown),
            Err(BindingQueryError::DependencyUnavailable)
        );
    }

    #[test]
    fn binding_computation_observes_cancellation() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let request = SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(fixture.constant);

        context.cancellation().cancel();

        assert_eq!(
            DeclaredTypeBinder.evaluate_symbol_query(&context, request),
            Err(BindingQueryError::Cancelled)
        );
    }
}
