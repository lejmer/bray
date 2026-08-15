use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractTypeQuery, CallableSignatureQuery, CallableSignatureTemplate,
    CallableSymbolId, ConstantDeclaredTypeQuery, GenericConstParameterDeclaredTypeQuery,
    InherentTypeMemberValueQuery, PredicateDefinitionSymbolId, PredicateSignatureTemplate,
    PredicateSignatureTemplateQuery, StructFieldTypeQuery, SymbolFactContract, SymbolFactRequest,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantMemberDeclaredTypeQuery,
    TraitTypeFulfillmentValueQuery, TypeExpressionTemplate, UnionPayloadFieldTypeQuery,
};

use super::Compilation;
use super::binder::{CompilationBindingContext, binder_fact_error};
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

        self.symbol_fact::<CallableSignatureQuery>(owner).map(Some)
    }

    /// Returns one predicate declaration's signature template.
    pub fn predicate_signature_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<PredicateSignatureTemplate>>>, FactQueryError> {
        let Some(owner) = PredicateDefinitionSymbolId::try_from_any(symbol) else {
            return Ok(None);
        };

        self.symbol_fact::<PredicateSignatureTemplateQuery>(owner)
            .map(Some)
    }

    /// Returns the declared type template carried directly by one symbol.
    pub fn symbol_type_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<TypeExpressionTemplate>>>, FactQueryError> {
        match symbol {
            AnySymbolId::CallableContract(owner) => self
                .symbol_fact::<CallableContractTypeQuery>(owner)
                .map(Some),
            AnySymbolId::Constant(owner) => self
                .symbol_fact::<ConstantDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::GenericConstParameter(owner) => self
                .symbol_fact::<GenericConstParameterDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::TraitConstantMember(owner) => self
                .symbol_fact::<TraitConstantMemberDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::TraitConstantFulfillment(owner) => self
                .symbol_fact::<TraitConstantFulfillmentDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::StructField(owner) => {
                self.symbol_fact::<StructFieldTypeQuery>(owner).map(Some)
            }
            AnySymbolId::UnionPayloadField(owner) => self
                .symbol_fact::<UnionPayloadFieldTypeQuery>(owner)
                .map(Some),
            AnySymbolId::InherentTypeMember(owner) => self
                .symbol_fact::<InherentTypeMemberValueQuery>(owner)
                .map(Some),
            AnySymbolId::TraitTypeFulfillment(owner) => self
                .symbol_fact::<TraitTypeFulfillmentValueQuery>(owner)
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
        for<'binding_context> CompilationBindingContext<'binding_context>: SymbolFactProvider<C>,
    {
        let binding_context = self.binding_context(&self.state.cancellation)?;

        binding_context
            .symbol_fact(SymbolFactRequest::<C>::new(owner))
            .map_err(binder_fact_error)
    }
}
