use bray_binder::BinderFactResult;
use bray_symbols::{
    AnyConstantDefinitionId, ConstantDefinitionQuery, SymbolFactRequest, SymbolFactResult,
    TraitConstantFulfillmentDefinitionQuery, TraitConstantMemberDefinitionQuery,
};

use super::binding::{CompilationSymbolFactBinding, binder_error};
use super::cache::CompilationSymbolSemantics;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::SymbolFactCache;

macro_rules! impl_constant_definition_query {
    ($contract:ty, $cache:ident, $definition:expr) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolSemantics {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBindingContext<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
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
