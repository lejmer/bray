use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractTypeQuery, CallableSignatureQuery, CallableSignatureTemplate,
    CallableSymbolId, ConstantDeclaredTypeQuery, GenericConstParameterDeclaredTypeQuery,
    InherentTypeMemberValueQuery, PredicateDefinitionSymbolId, PredicateSignatureTemplate,
    PredicateSignatureTemplateQuery, StaticDeclaredTypeQuery, StructFieldTypeQuery,
    SymbolQueryContract, SymbolQueryRequest, TraitConstantFulfillmentDeclaredTypeQuery,
    TraitConstantMemberDeclaredTypeQuery,
    TraitTypeFulfillmentValueQuery, TypeExpressionTemplate, UnionPayloadFieldTypeQuery,
};

use super::Compilation;
use super::binder::{CompilationBindingContext, binding_query_error};
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

        self.resolve_symbol_query::<CallableSignatureQuery>(owner)
            .map(Some)
    }

    /// Returns one predicate declaration's signature template.
    pub fn predicate_signature_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<PredicateSignatureTemplate>>>, FactQueryError> {
        let Some(owner) = PredicateDefinitionSymbolId::try_from_any(symbol) else {
            return Ok(None);
        };

        self.resolve_symbol_query::<PredicateSignatureTemplateQuery>(owner)
            .map(Some)
    }

    /// Returns the declared type template carried directly by one symbol.
    pub fn symbol_type_template(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<Arc<DiagnosticResult<TypeExpressionTemplate>>>, FactQueryError> {
        match symbol {
            AnySymbolId::CallableContract(owner) => self
                .resolve_symbol_query::<CallableContractTypeQuery>(owner)
                .map(Some),
            AnySymbolId::Constant(owner) => self
                .resolve_symbol_query::<ConstantDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::Static(owner) => self
                .resolve_symbol_query::<StaticDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::GenericConstParameter(owner) => self
                .resolve_symbol_query::<GenericConstParameterDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::TraitConstantMember(owner) => self
                .resolve_symbol_query::<TraitConstantMemberDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::TraitConstantFulfillment(owner) => self
                .resolve_symbol_query::<TraitConstantFulfillmentDeclaredTypeQuery>(owner)
                .map(Some),
            AnySymbolId::StructField(owner) => self
                .resolve_symbol_query::<StructFieldTypeQuery>(owner)
                .map(Some),
            AnySymbolId::UnionPayloadField(owner) => self
                .resolve_symbol_query::<UnionPayloadFieldTypeQuery>(owner)
                .map(Some),
            AnySymbolId::InherentTypeMember(owner) => self
                .resolve_symbol_query::<InherentTypeMemberValueQuery>(owner)
                .map(Some),
            AnySymbolId::TraitTypeFulfillment(owner) => self
                .resolve_symbol_query::<TraitTypeFulfillmentValueQuery>(owner)
                .map(Some),
            _ => Ok(None),
        }
    }

    fn resolve_symbol_query<C>(
        &self,
        owner: C::Owner,
    ) -> Result<Arc<DiagnosticResult<C::Value>>, FactQueryError>
    where
        C: SymbolQueryContract,
        for<'binding_context> CompilationBindingContext<'binding_context>: SymbolQueryProvider<C>,
    {
        let binding_context = self.binding_context(&self.state.cancellation)?;

        binding_context
            .resolve_symbol_query(SymbolQueryRequest::<C>::new(owner))
            .map_err(binding_query_error)
    }
}
