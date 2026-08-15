use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableParameterDefaultQuery, CallableParameterSymbolId, CheckedCallableParameterDefault,
    CheckedStructFieldDefault, CheckedUnionPayloadDefault, PredicateDefinition,
    PredicateDefinitionQuery, PredicateDefinitionState, PredicateDefinitionSymbolId,
    StructFieldDefaultQuery, StructFieldSymbolId, SymbolFactRequest,
    TraitPredicateFulfillmentDefinitionQuery, TraitPredicateMemberDefinitionQuery,
    UnionPayloadFieldDefaultQuery, UnionPayloadFieldSymbolId,
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
        let binding_context = self.binding_context(&self.state.cancellation)?;

        binding_context
            .symbol_fact(SymbolFactRequest::<CallableParameterDefaultQuery>::new(
                owner,
            ))
            .map_err(binder_fact_error)
    }

    /// Returns the runtime default owned by one struct field.
    pub fn struct_field_default(
        &self,
        owner: StructFieldSymbolId,
    ) -> Result<Arc<DiagnosticResult<CheckedStructFieldDefault>>, FactQueryError> {
        let binding_context = self.binding_context(&self.state.cancellation)?;

        binding_context
            .symbol_fact(SymbolFactRequest::<StructFieldDefaultQuery>::new(owner))
            .map_err(binder_fact_error)
    }

    /// Returns the runtime default owned by one union payload field.
    pub fn union_payload_field_default(
        &self,
        owner: UnionPayloadFieldSymbolId,
    ) -> Result<Arc<DiagnosticResult<CheckedUnionPayloadDefault>>, FactQueryError> {
        let binding_context = self.binding_context(&self.state.cancellation)?;

        binding_context
            .symbol_fact(SymbolFactRequest::<UnionPayloadFieldDefaultQuery>::new(
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
        let binding_context = self.binding_context(&self.state.cancellation)?;

        match owner {
            PredicateDefinitionSymbolId::Predicate(owner) => binding_context
                .symbol_fact(SymbolFactRequest::<PredicateDefinitionQuery>::new(owner))
                .map_err(binder_fact_error),
            PredicateDefinitionSymbolId::TraitMember(owner) => binding_context
                .symbol_fact(SymbolFactRequest::<TraitPredicateMemberDefinitionQuery>::new(owner))
                .map_err(binder_fact_error),
            PredicateDefinitionSymbolId::TraitFulfillment(owner) => binding_context
                .symbol_fact(
                    SymbolFactRequest::<TraitPredicateFulfillmentDefinitionQuery>::new(owner),
                )
                .map_err(binder_fact_error),
        }
    }
}
