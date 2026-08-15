macro_rules! impl_declaration_body_query {
    ($contract:ty, $cache:ident, $binding:ident) => {
        impl CompilationSymbolQueryEvaluator<$contract> for CompilationSymbolSemantics {
            fn cache(&self) -> &SymbolQueryCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBindingContext<'_>,
                request: SymbolQueryRequest<$contract>,
            ) -> BindingQueryResult<
                bray_diagnostics::DiagnosticResult<
                    <$contract as bray_symbols::SymbolQueryContract>::Value,
                >,
            > {
                $binding(context, request.owner())
            }
        }
    };
}
