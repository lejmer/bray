use bray_binder::BinderFactResult;
use bray_symbols::{
    AnyConstantDefinitionId, ConstantDefinitionFact, SymbolFactRequest, SymbolFactResult,
    TraitConstantFulfillmentDefinitionFact, TraitConstantMemberDefinitionFact,
};

use super::binding::{CompilationSymbolFactBinding, binder_error};
use super::cache::CompilationSymbolFacts;
use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::SymbolFactCache;

macro_rules! impl_constant_definition_fact {
    ($contract:ty, $cache:ident, $definition:expr) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolFacts {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBinderFacts<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                context
                    .compilation()
                    .compute_checked_constant_template(
                        $definition(request.owner()),
                        context.cancellation,
                    )
                    .map_err(binder_error)
            }
        }
    };
}

impl_constant_definition_fact!(
    ConstantDefinitionFact,
    constant_definitions,
    AnyConstantDefinitionId::Constant
);

impl_constant_definition_fact!(
    TraitConstantMemberDefinitionFact,
    trait_constant_member_definitions,
    AnyConstantDefinitionId::TraitMember
);

impl_constant_definition_fact!(
    TraitConstantFulfillmentDefinitionFact,
    trait_constant_fulfillment_definitions,
    AnyConstantDefinitionId::TraitFulfillment
);
