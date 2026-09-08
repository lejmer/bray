use std::sync::Arc;

use bray_base::Cancellation;
use bray_declarations::DeclarationTable;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableParameterDefaultProviderSymbolId,
    CallableParameterSymbolId, ImportedSymbolSkeleton, MemberLookupResult, ModuleSymbolId,
    NamedTypeSymbolId, SemanticValueStore, SymbolGraph, SymbolKey, TypeAssociatedSurface,
    TypeExpressionTemplate,
};
use bray_syntax::SyntaxTree;
use bray_target::TargetProfile;

use crate::{BindingQueryResult, ImportedPathRoot, NameAccess, SymbolQueryErrorProvider};

/// Injected read-only services available to one binding computation.
pub trait BindingQueryContext: Send + Sync {
    /// Exact failures owned by the coordinating query layer.
    type UpstreamError;
    /// The origin-neutral provider for symbol-facing semantic queries.
    type SymbolSemantics: SymbolQueryErrorProvider<UpstreamError = Self::UpstreamError> + ?Sized;
    /// The compilation-owned cancellation observer.
    type Cancellation: Cancellation + ?Sized;

    /// Returns the immutable syntax input for this compilation snapshot.
    fn syntax(&self) -> &SyntaxTree;

    /// Returns the immutable declaration-discovery input.
    fn declarations(&self) -> &DeclarationTable;

    /// Returns the immutable compilation-wide symbol identity graph.
    fn symbols(&self) -> &SymbolGraph;

    /// Returns one exact symbol's stable semantic key across supported origins.
    fn symbol_key(
        &self,
        symbol: AnySymbolId,
    ) -> BindingQueryResult<Option<&SymbolKey>, Self::UpstreamError> {
        Ok(self.symbols().symbol_key(symbol))
    }

    /// Returns whether recovery contributed to one exact symbol's surface.
    fn symbol_is_recovered(
        &self,
        symbol: AnySymbolId,
    ) -> BindingQueryResult<Option<bool>, Self::UpstreamError> {
        Ok(self.symbols().symbol_is_recovered(symbol))
    }

    /// Returns one exact symbol's immediate semantic owner across supported origins.
    fn containing_symbol(
        &self,
        symbol: AnySymbolId,
    ) -> BindingQueryResult<Option<AnySymbolId>, Self::UpstreamError> {
        Ok(self.symbols().containing_symbol(symbol))
    }

    /// Returns one callable parameter's default provider across supported origins.
    fn callable_parameter_default_provider(
        &self,
        parameter: CallableParameterSymbolId,
    ) -> BindingQueryResult<Option<CallableParameterDefaultProviderSymbolId>, Self::UpstreamError>
    {
        Ok(self
            .symbols()
            .callable_parameter(parameter)
            .and_then(|parameter| parameter.default_provider()))
    }

    /// Returns the declaration evaluated by one runtime-default provider.
    fn runtime_default_subject(
        &self,
        provider: AnySymbolId,
    ) -> BindingQueryResult<Option<AnySymbolId>, Self::UpstreamError> {
        Ok(self.symbols().runtime_default_subject(provider))
    }

    /// Resolves one ordinary declaration member across supported symbol origins.
    fn lookup_member(
        &self,
        owner: AnySymbolId,
        name: &str,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>, Self::UpstreamError> {
        Ok(self.symbols().lookup_member(owner, name))
    }

    /// Resolves the longest selected dependency package prefix of a qualified source path.
    fn imported_path_root(
        &self,
        components: &[&str],
    ) -> BindingQueryResult<Option<ImportedPathRoot<'_>>, Self::UpstreamError>;

    /// Returns the selected dependencies' immutable imported symbol surface.
    fn imported_symbols(
        &self,
    ) -> BindingQueryResult<Option<&ImportedSymbolSkeleton>, Self::UpstreamError>;

    /// Returns the callable type named by one callable-contract declaration.
    fn callable_contract_type(
        &self,
        definition: CallableContractSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeExpressionTemplate>>, Self::UpstreamError>;

    /// Binds the parameter and result types observed by a callable type's contract clauses.
    fn callable_contract_input_signature(
        &self,
        owner: AnySymbolId,
        source: bray_declarations::SyntaxAnchor,
    ) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableTypeTemplate>, Self::UpstreamError>;

    /// Checks the declared conditions and capability contract of one callable occurrence.
    fn callable_type_contract(
        &self,
        owner: AnySymbolId,
        source: bray_declarations::SyntaxAnchor,
        signature: &bray_symbols::CallableTypeTemplate,
    ) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableContractSet>, Self::UpstreamError>;

    /// Resolves one name introduced by a source module export declaration.
    fn module_re_export_lookup(
        &self,
        module: ModuleSymbolId,
        name: &str,
        access: NameAccess,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>, Self::UpstreamError>;

    /// Returns the complete declaration-level member surface associated with a named type.
    fn type_associated_surface(
        &self,
        subject: NamedTypeSymbolId,
    ) -> BindingQueryResult<Arc<DiagnosticResult<TypeAssociatedSurface>>, Self::UpstreamError>;

    /// Returns the canonical semantic value store associated with the symbol graph.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns the selected language-level target profile.
    fn selected_target(&self) -> &TargetProfile;

    /// Returns the origin-neutral symbol query provider.
    fn symbol_semantics(&self) -> &Self::SymbolSemantics;

    /// Returns the cancellation observer for this binding request.
    fn cancellation(&self) -> &Self::Cancellation;

    /// Returns whether cancellation has been requested.
    fn is_cancelled(&self) -> bool {
        self.cancellation().is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::SymbolOrigin;

    use super::BindingQueryContext;
    use crate::query::test_support::TestFixture;

    #[test]
    fn contexts_expose_exact_immutable_query_inputs() {
        let fixture = TestFixture::new();
        let context = fixture.context();

        assert_eq!(context.syntax().source_units().len(), 1);
        assert_eq!(context.declarations().declarations().len(), 2);

        assert_eq!(
            context
                .symbols()
                .constants()
                .iter()
                .filter(|constant| constant.origin() == SymbolOrigin::Source)
                .count(),
            1
        );

        assert_eq!(context.semantic_values().id(), fixture.semantic_values.id());

        assert_eq!(
            context.selected_target().identity().as_str(),
            "x86_64-unknown-linux-gnu"
        );

        assert!(!context.is_cancelled());
    }

    #[test]
    fn contexts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::query::test_support::TestContext<'static>>();
    }
}
