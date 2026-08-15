use bray_binder::BindingQueryResult;
use bray_symbols::{
    AnyConstantDefinitionId, ConstantDefinitionQuery, SymbolQueryRequest,
    TraitConstantFulfillmentDefinitionQuery, TraitConstantMemberDefinitionQuery,
};

use super::binding::{CompilationSymbolQueryEvaluator, binder_error};
use super::cache::CompilationSymbolSemantics;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::SymbolQueryCache;

macro_rules! impl_constant_definition_query {
    ($contract:ty, $cache:ident, $definition:expr) => {
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
                context
                    .compilation()
                    .compute_constant_definition($definition(request.owner()), context.cancellation)
                    .map_err(binder_error)
            }
        }
    };
}

impl_constant_definition_query!(
    ConstantDefinitionQuery,
    constant_definitions,
    AnyConstantDefinitionId::Constant
);

impl_constant_definition_query!(
    TraitConstantMemberDefinitionQuery,
    trait_constant_member_definitions,
    AnyConstantDefinitionId::TraitMember
);

impl_constant_definition_query!(
    TraitConstantFulfillmentDefinitionQuery,
    trait_constant_fulfillment_definitions,
    AnyConstantDefinitionId::TraitFulfillment
);
