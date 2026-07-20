use std::sync::Arc;

use bray_binder::{BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_symbols::{
    CallableContractTemplateFact, CallableContractTypeFact, CallableContractsFact,
    CallableOverloadTemplateFact, CallableParameterDefaultTemplateFact, CallableSignatureFact,
    ConstantDeclaredTypeFact, GenericConstParameterDeclaredTypeFact, GenericConstraintsFact,
    GenericDeclarationTemplateFact, ImplementationCoherenceFact, ImplementationHeadTemplateFact,
    ImplementationOverloadTemplateFact, ImplementationSubjectFact, ImplementedTraitApplicationFact,
    InherentTypeMemberValueFact, PredicateSignatureTemplateFact, StructFieldDefaultTemplateFact,
    StructFieldTypeFact, SymbolFactContract, SymbolFactRequest, SymbolFactResult,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
    TraitTypeFulfillmentValueFact, UnionPayloadFieldDefaultTemplateFact, UnionPayloadFieldTypeFact,
};

use super::super::context::CompilationBinderFacts;
use super::compute::CompilationSymbolFactBinding;
use crate::fact::{FactQueryError, SymbolFactCache};

pub(in crate::compilation) struct CompilationSymbolFacts {
    pub(super) generic_constraints: SymbolFactCache<GenericConstraintsFact>,
    pub(super) generic_declaration_templates: SymbolFactCache<GenericDeclarationTemplateFact>,
    pub(super) callable_signatures: SymbolFactCache<CallableSignatureFact>,
    pub(super) callable_contracts: SymbolFactCache<CallableContractsFact>,
    pub(super) callable_contract_templates: SymbolFactCache<CallableContractTemplateFact>,
    pub(super) predicate_signature_templates: SymbolFactCache<PredicateSignatureTemplateFact>,
    pub(super) callable_contract_types: SymbolFactCache<CallableContractTypeFact>,
    pub(super) constant_declared_types: SymbolFactCache<ConstantDeclaredTypeFact>,
    pub(super) generic_const_parameter_declared_types:
        SymbolFactCache<GenericConstParameterDeclaredTypeFact>,
    pub(super) trait_constant_member_declared_types:
        SymbolFactCache<TraitConstantMemberDeclaredTypeFact>,
    pub(super) trait_constant_fulfillment_declared_types:
        SymbolFactCache<TraitConstantFulfillmentDeclaredTypeFact>,
    pub(super) struct_field_types: SymbolFactCache<StructFieldTypeFact>,
    pub(super) union_payload_field_types: SymbolFactCache<UnionPayloadFieldTypeFact>,
    pub(super) inherent_type_member_values: SymbolFactCache<InherentTypeMemberValueFact>,
    pub(super) trait_type_fulfillment_values: SymbolFactCache<TraitTypeFulfillmentValueFact>,
    pub(super) implementation_subjects: SymbolFactCache<ImplementationSubjectFact>,
    pub(super) implemented_traits: SymbolFactCache<ImplementedTraitApplicationFact>,
    pub(super) implementation_coherence: SymbolFactCache<ImplementationCoherenceFact>,
    pub(super) implementation_head_templates: SymbolFactCache<ImplementationHeadTemplateFact>,
    pub(super) callable_parameter_default_templates:
        SymbolFactCache<CallableParameterDefaultTemplateFact>,
    pub(super) struct_field_default_templates: SymbolFactCache<StructFieldDefaultTemplateFact>,
    pub(super) union_payload_field_default_templates:
        SymbolFactCache<UnionPayloadFieldDefaultTemplateFact>,
    pub(super) callable_overload_templates: SymbolFactCache<CallableOverloadTemplateFact>,
    pub(super) implementation_overload_templates:
        SymbolFactCache<ImplementationOverloadTemplateFact>,
}

impl CompilationSymbolFacts {
    pub(in crate::compilation) const fn new() -> Self {
        Self {
            generic_constraints: SymbolFactCache::new(),
            generic_declaration_templates: SymbolFactCache::new(),
            callable_signatures: SymbolFactCache::new(),
            callable_contracts: SymbolFactCache::new(),
            callable_contract_templates: SymbolFactCache::new(),
            predicate_signature_templates: SymbolFactCache::new(),
            callable_contract_types: SymbolFactCache::new(),
            constant_declared_types: SymbolFactCache::new(),
            generic_const_parameter_declared_types: SymbolFactCache::new(),
            trait_constant_member_declared_types: SymbolFactCache::new(),
            trait_constant_fulfillment_declared_types: SymbolFactCache::new(),
            struct_field_types: SymbolFactCache::new(),
            union_payload_field_types: SymbolFactCache::new(),
            inherent_type_member_values: SymbolFactCache::new(),
            trait_type_fulfillment_values: SymbolFactCache::new(),
            implementation_subjects: SymbolFactCache::new(),
            implemented_traits: SymbolFactCache::new(),
            implementation_coherence: SymbolFactCache::new(),
            implementation_head_templates: SymbolFactCache::new(),
            callable_parameter_default_templates: SymbolFactCache::new(),
            struct_field_default_templates: SymbolFactCache::new(),
            union_payload_field_default_templates: SymbolFactCache::new(),
            callable_overload_templates: SymbolFactCache::new(),
            implementation_overload_templates: SymbolFactCache::new(),
        }
    }
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
        let facts = &self.compilation.state.symbol_facts;
        let cache = facts.cache();

        cache
            .get_or_compute(
                &self.compilation.state.fact_runtime,
                self.cancellation,
                request,
                || facts.bind(self, request).map_err(fact_error),
            )
            .map_err(binder_error)
    }
}

pub(super) const fn fact_error(error: BinderFactError) -> FactQueryError {
    match error {
        BinderFactError::Cancelled => FactQueryError::Cancelled,
        BinderFactError::DependencyUnavailable => FactQueryError::InfrastructureFailure,
    }
}

fn binder_error(error: FactQueryError) -> BinderFactError {
    match error {
        FactQueryError::Cancelled => BinderFactError::Cancelled,
        FactQueryError::Cycle(_)
        | FactQueryError::InfrastructureFailure
        | FactQueryError::SemanticUnitContext(_)
        | FactQueryError::CheckerInfrastructure(_) => BinderFactError::DependencyUnavailable,
    }
}
