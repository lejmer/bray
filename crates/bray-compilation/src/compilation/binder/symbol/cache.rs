use std::sync::Arc;

use bray_binder::{BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_symbols::{
    CallableContractTypeFact, CallableContractsFact, CallableSignatureFact, GenericConstraintsFact,
    ImplementationCoherenceFact, ImplementationSubjectFact, ImplementedTraitApplicationFact,
    InherentTypeMemberValueFact, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest,
    SymbolFactResult, TraitTypeFulfillmentValueFact, UnionPayloadFieldTypeFact,
};

use super::super::context::CompilationBinderFacts;
use super::compute::CompilationSymbolFactBinding;
use crate::fact::{FactQueryError, SymbolFactCache};

pub(in crate::compilation) struct CompilationSymbolFacts {
    pub(super) generic_constraints: SymbolFactCache<GenericConstraintsFact>,
    pub(super) callable_signatures: SymbolFactCache<CallableSignatureFact>,
    pub(super) callable_contracts: SymbolFactCache<CallableContractsFact>,
    pub(super) callable_contract_types: SymbolFactCache<CallableContractTypeFact>,
    pub(super) struct_field_types: SymbolFactCache<StructFieldTypeFact>,
    pub(super) union_payload_field_types: SymbolFactCache<UnionPayloadFieldTypeFact>,
    pub(super) inherent_type_member_values: SymbolFactCache<InherentTypeMemberValueFact>,
    pub(super) trait_type_fulfillment_values: SymbolFactCache<TraitTypeFulfillmentValueFact>,
    pub(super) implementation_subjects: SymbolFactCache<ImplementationSubjectFact>,
    pub(super) implemented_traits: SymbolFactCache<ImplementedTraitApplicationFact>,
    pub(super) implementation_coherence: SymbolFactCache<ImplementationCoherenceFact>,
}

impl CompilationSymbolFacts {
    pub(in crate::compilation) const fn new() -> Self {
        Self {
            generic_constraints: SymbolFactCache::new(),
            callable_signatures: SymbolFactCache::new(),
            callable_contracts: SymbolFactCache::new(),
            callable_contract_types: SymbolFactCache::new(),
            struct_field_types: SymbolFactCache::new(),
            union_payload_field_types: SymbolFactCache::new(),
            inherent_type_member_values: SymbolFactCache::new(),
            trait_type_fulfillment_values: SymbolFactCache::new(),
            implementation_subjects: SymbolFactCache::new(),
            implemented_traits: SymbolFactCache::new(),
            implementation_coherence: SymbolFactCache::new(),
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
        FactQueryError::Cycle(_) | FactQueryError::InfrastructureFailure => {
            BinderFactError::DependencyUnavailable
        }
    }
}
