use std::sync::Arc;

use bray_binder::{BinderFactResult, SymbolFactProvider};
use bray_symbols::{
    CallableContractTemplateFact, CallableContractTypeFact, CallableContractsFact,
    CallableOverloadTemplateFact, CallableParameterDefaultFact,
    CallableParameterDefaultTemplateFact, CallableSignatureFact, ConstantDeclaredTypeFact,
    ConstantDefinitionFact, DeclarationDirectivesFact, GenericConstParameterDeclaredTypeFact,
    GenericConstraintsFact, GenericDeclarationTemplateFact, ImplementationCoherenceFact,
    ImplementationHeadTemplateFact, ImplementationOverloadTemplateFact, ImplementationSubjectFact,
    ImplementedTraitApplicationFact, InherentTypeMemberValueFact, ModuleSurfaceFact,
    PredicateDefinitionFact, PredicateSignatureTemplateFact, StructFieldDefaultFact,
    StructFieldDefaultTemplateFact, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest,
    SymbolFactResult, TraitConstantFulfillmentDeclaredTypeFact,
    TraitConstantFulfillmentDefinitionFact, TraitConstantMemberDeclaredTypeFact,
    TraitConstantMemberDefinitionFact, TraitPredicateFulfillmentDefinitionFact,
    TraitPredicateMemberDefinitionFact, TraitTypeFulfillmentValueFact,
    UnionPayloadFieldDefaultFact, UnionPayloadFieldDefaultTemplateFact, UnionPayloadFieldTypeFact,
};

use super::super::binder_fact_error;
use super::super::context::CompilationBinderFacts;
use super::binding::{CompilationSymbolFactBinding, binder_error};
use crate::fact::{CompilationFactKey, SymbolFactCache};

macro_rules! define_compilation_symbol_facts {
    ($($field:ident: $contract:ty),+ $(,)?) => {
        pub(in crate::compilation) struct CompilationSymbolFacts {
            $(pub(super) $field: SymbolFactCache<$contract>,)+
        }

        impl CompilationSymbolFacts {
            pub(in crate::compilation) fn new() -> Self {
                Self {
                    $($field: SymbolFactCache::new(),)+
                }
            }

            pub(in crate::compilation) fn updated(
                &self,
                reusable: &std::collections::BTreeSet<CompilationFactKey>,
            ) -> Self {
                Self {
                    $($field: self.$field.updated(reusable),)+
                }
            }
        }
    };
}

define_compilation_symbol_facts! {
    module_surfaces: ModuleSurfaceFact,
    declaration_directives: DeclarationDirectivesFact,
    generic_constraints: GenericConstraintsFact,
    generic_declaration_templates: GenericDeclarationTemplateFact,
    callable_signatures: CallableSignatureFact,
    callable_contracts: CallableContractsFact,
    callable_contract_templates: CallableContractTemplateFact,
    predicate_signature_templates: PredicateSignatureTemplateFact,
    callable_contract_types: CallableContractTypeFact,
    constant_declared_types: ConstantDeclaredTypeFact,
    constant_definitions: ConstantDefinitionFact,
    generic_const_parameter_declared_types: GenericConstParameterDeclaredTypeFact,
    trait_constant_member_declared_types: TraitConstantMemberDeclaredTypeFact,
    trait_constant_member_definitions: TraitConstantMemberDefinitionFact,
    trait_constant_fulfillment_declared_types: TraitConstantFulfillmentDeclaredTypeFact,
    trait_constant_fulfillment_definitions: TraitConstantFulfillmentDefinitionFact,
    struct_field_types: StructFieldTypeFact,
    union_payload_field_types: UnionPayloadFieldTypeFact,
    inherent_type_member_values: InherentTypeMemberValueFact,
    trait_type_fulfillment_values: TraitTypeFulfillmentValueFact,
    implementation_subjects: ImplementationSubjectFact,
    implemented_traits: ImplementedTraitApplicationFact,
    implementation_coherence: ImplementationCoherenceFact,
    implementation_head_templates: ImplementationHeadTemplateFact,
    callable_parameter_default_templates: CallableParameterDefaultTemplateFact,
    callable_parameter_defaults: CallableParameterDefaultFact,
    struct_field_default_templates: StructFieldDefaultTemplateFact,
    struct_field_defaults: StructFieldDefaultFact,
    union_payload_field_default_templates: UnionPayloadFieldDefaultTemplateFact,
    union_payload_field_defaults: UnionPayloadFieldDefaultFact,
    predicate_definitions: PredicateDefinitionFact,
    trait_predicate_member_definitions: TraitPredicateMemberDefinitionFact,
    trait_predicate_fulfillment_definitions: TraitPredicateFulfillmentDefinitionFact,
    callable_overload_templates: CallableOverloadTemplateFact,
    implementation_overload_templates: ImplementationOverloadTemplateFact,
}

impl<C> SymbolFactProvider<C> for CompilationBinderFacts<'_>
where
    C: SymbolFactContract,
    CompilationSymbolFacts: CompilationSymbolFactBinding<C>,
{
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<C>,
    ) -> BinderFactResult<Arc<SymbolFactResult<C>>> {
        let facts = self.symbol_facts;
        let cache = facts.cache();

        cache
            .get_or_compute(
                &self.compilation.state.fact_runtime,
                self.cancellation,
                request,
                || facts.bind(self, request).map_err(binder_fact_error),
            )
            .map_err(binder_error)
    }
}
