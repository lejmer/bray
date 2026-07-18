use std::sync::Arc;

use bray_binder::{BinderFactContext, BinderFactError, SymbolFactProvider};
use bray_bound_tree::{BoundSourceAnchor, BoundUnit, BoundUnitKey};
use bray_checker::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerSource,
};
use bray_source::{SourceSnapshot, SourceSpan};
use bray_symbols::{
    AvailableCompilerKnownSymbols, SemanticValueStore, SymbolFactContract, SymbolFactRequest,
    SymbolFactResult,
};

use super::Compilation;
use super::binder::CompilationBinderFacts;
use crate::fact::{CancellationToken, FactQueryError};

pub(super) struct CompilationCheckerContext<'compilation> {
    facts: CompilationBinderFacts<'compilation>,
    available_compiler_known_symbols: &'compilation AvailableCompilerKnownSymbols,
}

impl<'compilation> CompilationCheckerContext<'compilation> {
    fn new(
        facts: CompilationBinderFacts<'compilation>,
        available_compiler_known_symbols: &'compilation AvailableCompilerKnownSymbols,
    ) -> Self {
        Self {
            facts,
            available_compiler_known_symbols,
        }
    }

    fn source_snapshot(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<&SourceSnapshot, CheckerInfrastructureError> {
        let syntax = anchor.syntax();
        let source_id = syntax.source_id();

        let Some(source) = self.facts.compilation().source(source_id) else {
            return Err(CheckerInfrastructureError::MissingSource { source_id });
        };

        if source.version() != anchor.source_version() {
            return Err(CheckerInfrastructureError::SourceVersionMismatch {
                source_id,
                expected: anchor.source_version(),
                actual: source.version(),
            });
        }

        Ok(source)
    }

    pub(super) fn symbols(&self) -> &bray_symbols::SymbolGraph {
        self.facts.symbols()
    }
}

impl CheckerRequestContext for CompilationCheckerContext<'_> {
    fn entry_context_matches(
        &self,
        unit: &BoundUnit,
        entry: &bray_checker::UnitCheckEntryContext,
    ) -> bool {
        bray_binder::unit_check_entry_context(self.symbols(), unit)
            .is_ok_and(|expected| expected == *entry)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.facts.semantic_values()
    }

    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.available_compiler_known_symbols
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        let source = self.source_snapshot(anchor)?;
        let range = anchor.syntax().full_range();
        let span = SourceSpan::new(source.source_id(), range);

        let Some(text) = source.text_slice(range) else {
            return Err(CheckerInfrastructureError::InvalidSourceRange { span });
        };

        Ok(CheckerSource::new(span, text))
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self.facts.cancellation()
    }
}

impl<'compilation, C> CheckerSemanticFactProvider<C> for CompilationCheckerContext<'compilation>
where
    C: SymbolFactContract,
    CompilationBinderFacts<'compilation>: SymbolFactProvider<C>,
{
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<C>,
    ) -> CheckerFactResult<Arc<SymbolFactResult<C>>> {
        self.facts
            .symbol_fact(request)
            .map_err(|error| match error {
                BinderFactError::Cancelled => CheckerFactError::Cancelled,
                BinderFactError::DependencyUnavailable => CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticFactUnavailable {
                        symbol: request.symbol(),
                        kind: request.kind(),
                    },
                ),
            })
    }
}

impl Compilation {
    pub(super) fn checker_context_for<'compilation>(
        &'compilation self,
        key: &BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationCheckerContext<'compilation>, FactQueryError> {
        let facts = self.binder_facts_for(key, cancellation)?;
        let available_compiler_known_symbols = self.available_compiler_known_symbols();

        Ok(CompilationCheckerContext::new(
            facts,
            available_compiler_known_symbols,
        ))
    }
}

#[cfg(test)]
mod tests {
    use bray_binder::unit_check_entry_context;
    use bray_bound_tree::BoundSourceAnchor;
    use bray_checker::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};
    use bray_source::SourceVersion;
    use bray_symbols::{CallableSignatureFact, CallableSymbolId, SymbolFactRequest, SymbolOrigin};

    use super::Compilation;
    use crate::test_support::{compilation, source_callable_body_key};

    #[test]
    fn contexts_resolve_only_the_source_range_named_by_a_bound_anchor() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let anchor = key.source();
        let source = match context.source(anchor) {
            Ok(source) => source,
            Err(error) => panic!("bound source must resolve: {error:?}"),
        };

        let span = source.span();

        assert_eq!(span.source_id(), anchor.syntax().source_id());
        assert_eq!(span.range(), anchor.syntax().full_range());

        let Some(snapshot) = compilation.source(span.source_id()) else {
            panic!("test compilation must retain its source");
        };

        assert_eq!(snapshot.text_slice(span.range()), Some(source.text()));
        assert_ne!(source.text(), snapshot.text());
    }

    #[test]
    fn contexts_reject_bound_anchors_from_another_source_revision() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let source = key.source();
        let stale = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert!(matches!(
            context.source(stale),
            Err(CheckerInfrastructureError::SourceVersionMismatch { .. })
        ));
    }

    #[test]
    fn contexts_supply_typed_symbol_facts_without_origin_specific_apis() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let entry = match unit_check_entry_context(context.symbols(), bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("checker entry context must be available: {error:?}"),
        };

        let request = match UnitCheckRequest::new(bound.value(), &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("checker request must be valid: {error:?}"),
        };

        let graph = match compilation.symbol_graph() {
            Ok(graph) => graph,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let source_functions = graph
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [function] = source_functions.as_slice() else {
            panic!("test source must contain one function");
        };

        let signature_request =
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function.id()));

        let signature = request.symbol_fact(signature_request);

        assert!(signature.is_ok());
    }

    fn callable_compilation() -> Compilation {
        compilation(
            r#"module app;
func main()
{
}
"#,
        )
    }
}
