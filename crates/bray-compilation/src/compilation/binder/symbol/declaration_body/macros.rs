macro_rules! impl_declaration_body_fact {
    ($contract:ty, $cache:ident, $binding:ident) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolFacts {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBinderFacts<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                $binding(context, request.owner())
            }
        }
    };
}
