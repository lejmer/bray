use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableParameterDefaultFact, CallableParameterSymbolId, CheckedCallableParameterDefault,
    CheckedStructFieldDefault, CheckedUnionPayloadDefault, PredicateDefinition,
    PredicateDefinitionFact, PredicateDefinitionState, PredicateDefinitionSymbolId,
    StructFieldDefaultFact, StructFieldSymbolId, SymbolFactRequest,
    TraitPredicateFulfillmentDefinitionFact, TraitPredicateMemberDefinitionFact,
    UnionPayloadFieldDefaultFact, UnionPayloadFieldSymbolId,
};

use super::Compilation;
use super::binder::binder_fact_error;
use crate::fact::FactQueryError;

impl Compilation {
    /// Returns the runtime default owned by one callable parameter.
    pub fn callable_parameter_default(
        &self,
        owner: CallableParameterSymbolId,
    ) -> Result<Arc<DiagnosticResult<CheckedCallableParameterDefault>>, FactQueryError> {
        let facts = self.binder_facts(&self.state.cancellation)?;

        facts
            .symbol_fact(SymbolFactRequest::<CallableParameterDefaultFact>::new(
                owner,
            ))
            .map_err(binder_fact_error)
    }

    /// Returns the runtime default owned by one struct field.
    pub fn struct_field_default(
        &self,
        owner: StructFieldSymbolId,
    ) -> Result<Arc<DiagnosticResult<CheckedStructFieldDefault>>, FactQueryError> {
        let facts = self.binder_facts(&self.state.cancellation)?;

        facts
            .symbol_fact(SymbolFactRequest::<StructFieldDefaultFact>::new(owner))
            .map_err(binder_fact_error)
    }

    /// Returns the runtime default owned by one union payload field.
    pub fn union_payload_field_default(
        &self,
        owner: UnionPayloadFieldSymbolId,
    ) -> Result<Arc<DiagnosticResult<CheckedUnionPayloadDefault>>, FactQueryError> {
        let facts = self.binder_facts(&self.state.cancellation)?;

        facts
            .symbol_fact(SymbolFactRequest::<UnionPayloadFieldDefaultFact>::new(
                owner,
            ))
            .map_err(binder_fact_error)
    }

    /// Returns the semantic definition state of one predicate declaration.
    pub fn predicate_definition(
        &self,
        owner: PredicateDefinitionSymbolId,
    ) -> Result<Arc<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>>, FactQueryError>
    {
        let facts = self.binder_facts(&self.state.cancellation)?;

        match owner {
            PredicateDefinitionSymbolId::Predicate(owner) => facts
                .symbol_fact(SymbolFactRequest::<PredicateDefinitionFact>::new(owner))
                .map_err(binder_fact_error),
            PredicateDefinitionSymbolId::TraitMember(owner) => facts
                .symbol_fact(SymbolFactRequest::<TraitPredicateMemberDefinitionFact>::new(owner))
                .map_err(binder_fact_error),
            PredicateDefinitionSymbolId::TraitFulfillment(owner) => facts
                .symbol_fact(
                    SymbolFactRequest::<TraitPredicateFulfillmentDefinitionFact>::new(owner),
                )
                .map_err(binder_fact_error),
        }
    }
}
