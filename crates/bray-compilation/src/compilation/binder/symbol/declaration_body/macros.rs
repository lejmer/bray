macro_rules! impl_declaration_body_query {
    ($contract:ty, $cache:ident, $binding:ident) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolSemantics {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBindingContext<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                $binding(context, request.owner())
            }
        }
    };
}
