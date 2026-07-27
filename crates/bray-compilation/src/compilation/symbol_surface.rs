use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractTypeFact, CallableSignatureFact, CallableSignatureTemplate,
    CallableSymbolId, ConstantDeclaredTypeFact, GenericConstParameterDeclaredTypeFact,
    InherentTypeMemberValueFact, PredicateDefinitionSymbolId, PredicateSignatureTemplate,
    PredicateSignatureTemplateFact, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
    TraitTypeFulfillmentValueFact, TypeExpressionTemplate, UnionPayloadFieldTypeFact,
};

use super::Compilation;
use super::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

impl Compilation {
    /// Returns one callable declaration's signature template.
    pub fn callable_signature_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<CallableSignatureTemplate>>>, FactQueryError> {
        let Some(owner) = CallableSymbolId::try_from_any(symbol) else {
            return Ok(None);
        };

        self.symbol_fact::<CallableSignatureFact>(owner).map(Some)
    }

    /// Returns one predicate declaration's signature template.
    pub fn predicate_signature_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<PredicateSignatureTemplate>>>, FactQueryError> {
        let Some(owner) = PredicateDefinitionSymbolId::try_from_any(symbol) else {
            return Ok(None);
        };

        self.symbol_fact::<PredicateSignatureTemplateFact>(owner)
            .map(Some)
    }

    /// Returns the declared type template carried directly by one symbol.
    pub fn symbol_type_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<TypeExpressionTemplate>>>, FactQueryError> {
        match symbol {
            AnySymbolId::CallableContract(owner) => self
                .symbol_fact::<CallableContractTypeFact>(owner)
                .map(Some),
            AnySymbolId::Constant(owner) => self
                .symbol_fact::<ConstantDeclaredTypeFact>(owner)
                .map(Some),
            AnySymbolId::GenericConstParameter(owner) => self
                .symbol_fact::<GenericConstParameterDeclaredTypeFact>(owner)
                .map(Some),
            AnySymbolId::TraitConstantMember(owner) => self
                .symbol_fact::<TraitConstantMemberDeclaredTypeFact>(owner)
                .map(Some),
            AnySymbolId::TraitConstantFulfillment(owner) => self
                .symbol_fact::<TraitConstantFulfillmentDeclaredTypeFact>(owner)
                .map(Some),
            AnySymbolId::StructField(owner) => {
                self.symbol_fact::<StructFieldTypeFact>(owner).map(Some)
            }
            AnySymbolId::UnionPayloadField(owner) => self
                .symbol_fact::<UnionPayloadFieldTypeFact>(owner)
                .map(Some),
            AnySymbolId::InherentTypeMember(owner) => self
                .symbol_fact::<InherentTypeMemberValueFact>(owner)
                .map(Some),
            AnySymbolId::TraitTypeFulfillment(owner) => self
                .symbol_fact::<TraitTypeFulfillmentValueFact>(owner)
                .map(Some),
            _ => Ok(None),
        }
    }

    fn symbol_fact<C>(
        &self,
        owner: C::Owner,
    ) -> Result<Arc<DiagnosticResult<C::Value>>, FactQueryError>
    where
        C: SymbolFactContract,
        for<'facts> CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
    {
        let facts = self.binder_facts(&self.state.cancellation)?;

        facts
            .symbol_fact(SymbolFactRequest::<C>::new(owner))
            .map_err(binder_fact_error)
    }
}
