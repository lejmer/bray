use std::sync::Arc;

use bray_symbols::{SymbolFactContract, SymbolFactRequest, SymbolFactResult};

use crate::{BinderFactContext, BinderFactResult};

/// Provides shared immutable access to one category of symbol-facing semantic fact.
///
/// Implement this trait separately for each supported [`SymbolFactContract`]. The coordinating
/// query layer routes source, compiler-known, synthesized, and imported symbols internally.
/// Binder code does not branch on symbol storage origin. Repeated completed requests must expose
/// the same published result.
pub trait SymbolFactProvider<Contract>: Send + Sync
where
    Contract: SymbolFactContract,
{
    /// Returns the published fact for the typed request.
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<Contract>,
    ) -> BinderFactResult<Arc<SymbolFactResult<Contract>>>;
}

/// Computes one binding-dependent symbol fact without coordinating its publication.
///
/// Binder implementations return one complete immutable value with its owned diagnostics.
/// Compilation wraps this operation in exact keys, dependency tracking, caching, cycle handling,
/// cancellation, and atomic publication.
pub trait BindingSymbolFactProvider<Contract, Context>: Send + Sync
where
    Contract: SymbolFactContract,
    Context: BinderFactContext + ?Sized,
{
    /// Computes the complete fact selected by `request` from injected read-only dependencies.
    fn compute_symbol_fact(
        &self,
        context: &Context,
        request: SymbolFactRequest<Contract>,
    ) -> BinderFactResult<SymbolFactResult<Contract>>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use bray_diagnostics::DiagnosticResult;
    use bray_symbols::{
        ConstantDeclaredTypeFact, ConstantSymbolId, SymbolFactRequest, SymbolId, TypeId,
    };

    use super::{BindingSymbolFactProvider, SymbolFactProvider};
    use crate::fact::test_support::{TestContext, TestFixture};
    use crate::{BinderFactContext, BinderFactError, BinderFactResult};

    struct DeclaredTypeBinder;

    impl BindingSymbolFactProvider<ConstantDeclaredTypeFact, TestContext<'_>> for DeclaredTypeBinder {
        fn compute_symbol_fact(
            &self,
            context: &TestContext<'_>,
            request: SymbolFactRequest<ConstantDeclaredTypeFact>,
        ) -> BinderFactResult<DiagnosticResult<TypeId>> {
            if context.is_cancelled() {
                return Err(BinderFactError::Cancelled);
            }

            let Some(_) = context.symbols().constant(request.owner()) else {
                return Err(BinderFactError::DependencyUnavailable);
            };

            Ok(DiagnosticResult::without_diagnostics(
                *context.symbol_facts().result.value(),
            ))
        }
    }

    #[test]
    fn origin_neutral_symbol_fact_reads_are_repeatable_and_concurrent() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let request = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);

        let first = match context.symbol_facts().symbol_fact(request) {
            Ok(result) => result,
            Err(error) => panic!("test symbol fact should exist: {error:?}"),
        };

        let second = match context.symbol_facts().symbol_fact(request) {
            Ok(result) => result,
            Err(error) => panic!("test symbol fact should remain available: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        thread::scope(|scope| {
            let handles = (0..4)
                .map(|_| {
                    scope.spawn(|| match context.symbol_facts().symbol_fact(request) {
                        Ok(result) => result,
                        Err(error) => panic!("concurrent fact read should succeed: {error:?}"),
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                let concurrent = match handle.join() {
                    Ok(result) => result,
                    Err(error) => panic!("fact reader should not panic: {error:?}"),
                };

                assert!(Arc::ptr_eq(&first, &concurrent));
            }
        });
    }

    #[test]
    fn symbol_fact_provider_keeps_unavailable_owners_outside_semantic_results() {
        let fixture = TestFixture::new();
        let context = fixture.context();

        let unknown = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(
            ConstantSymbolId::from_symbol_id(SymbolId::new(99)),
        );

        assert_eq!(
            context.symbol_facts().symbol_fact(unknown),
            Err(BinderFactError::DependencyUnavailable)
        );
    }

    #[test]
    fn binding_computation_is_deterministic_and_recovers_from_unknown_owners() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let request = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);

        let first = DeclaredTypeBinder.compute_symbol_fact(&context, request);
        let second = DeclaredTypeBinder.compute_symbol_fact(&context, request);

        assert_eq!(first, second);

        assert_eq!(
            first,
            Ok(DiagnosticResult::without_diagnostics(fixture.declared_type))
        );

        let unknown = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(
            ConstantSymbolId::from_symbol_id(SymbolId::new(99)),
        );

        assert_eq!(
            DeclaredTypeBinder.compute_symbol_fact(&context, unknown),
            Err(BinderFactError::DependencyUnavailable)
        );
    }

    #[test]
    fn binding_computation_observes_cancellation() {
        let fixture = TestFixture::new();
        let context = fixture.context();
        let request = SymbolFactRequest::<ConstantDeclaredTypeFact>::new(fixture.constant);

        context.cancellation().cancel();

        assert_eq!(
            DeclaredTypeBinder.compute_symbol_fact(&context, request),
            Err(BinderFactError::Cancelled)
        );
    }
}
