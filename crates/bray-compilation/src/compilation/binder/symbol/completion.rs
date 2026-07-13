use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableContractTypeFact, CallableContractsFact,
    CallableSignatureFact, CallableSymbolId, ExactSymbolId, GenericConstraintsFact, GenericOwnerId,
    ImplementationSubjectFact, ImplementationSymbolId, ImplementedTraitApplicationFact,
    InherentTypeMemberValueFact, StructFieldSymbolId, StructFieldTypeFact, SymbolCompletionLevel,
    SymbolFactCompletionRequest, SymbolFactContract, SymbolFactForcer, SymbolFactKind,
    SymbolFactRequest, TraitTypeFulfillmentValueFact,
};

use super::super::context::CompilationBinderFacts;
use super::cache::fact_error;
use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError, SymbolCompletionError};

impl SymbolFactForcer for CompilationBinderFacts<'_> {
    type Error = FactQueryError;

    fn force(&self, request: SymbolFactCompletionRequest) -> Result<DiagnosticBag, Self::Error> {
        match request.kind() {
            // These surfaces are frozen into the immutable symbol graph before semantic facts.
            SymbolFactKind::Members
            | SymbolFactKind::Imports
            | SymbolFactKind::Directives
            | SymbolFactKind::GenericParameters
            | SymbolFactKind::UnionVariantPayload => Ok(DiagnosticBag::new()),
            SymbolFactKind::GenericConstraints => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<GenericConstraintsFact>(self, owner)
            }
            SymbolFactKind::CallableSignature => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<CallableSignatureFact>(self, owner)
            }
            SymbolFactKind::CallableContracts => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<CallableContractsFact>(self, owner)
            }
            SymbolFactKind::CallableContractType => force_exact::<
                CallableContractTypeFact,
                CallableContractSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::StructFieldType => {
                force_exact::<StructFieldTypeFact, StructFieldSymbolId>(self, request.symbol())
            }
            SymbolFactKind::TypeMemberValue => force_type_member_value(self, request.symbol()),
            SymbolFactKind::ImplementationSubject => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementationSubjectFact>(self, owner)
            }
            SymbolFactKind::ImplementedTraitApplication => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementedTraitApplicationFact>(self, owner)
            }
            SymbolFactKind::ImplementationCoherence => {
                force_implementation_coherence_dependencies(self, request.symbol())
            }
            SymbolFactKind::ConstantDeclaredType
            | SymbolFactKind::ConstantDefinition
            | SymbolFactKind::CallableParameterDefault
            | SymbolFactKind::StructFieldDefault
            | SymbolFactKind::UnionPayloadFieldType
            | SymbolFactKind::UnionPayloadFieldDefault
            | SymbolFactKind::PredicateDefinition
            | SymbolFactKind::OverloadArms => Err(FactQueryError::InfrastructureFailure),
        }
    }
}

fn force_implementation_coherence_dependencies(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    let owner = ImplementationSymbolId::try_from_any(symbol)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    force_typed::<ImplementationSubjectFact>(facts, owner)?;
    force_typed::<ImplementedTraitApplicationFact>(facts, owner)?;

    Ok(DiagnosticBag::new())
}

impl Compilation {
    /// Forces one symbol subtree to the requested semantic completion boundary.
    pub fn force_complete_symbol(
        &self,
        root: AnySymbolId,
        level: SymbolCompletionLevel,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, SymbolCompletionError<FactQueryError>> {
        let facts = self
            .binder_facts(cancellation)
            .map_err(SymbolCompletionError::Provider)?;

        crate::fact::force_complete_symbol(
            facts.symbols,
            root,
            level,
            self.worker_budget(),
            cancellation,
            &facts,
        )
    }
}

fn force_type_member_value(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::InherentTypeMember(owner) => {
            force_typed::<InherentTypeMemberValueFact>(facts, owner)
        }
        AnySymbolId::TraitTypeFulfillment(owner) => {
            force_typed::<TraitTypeFulfillmentValueFact>(facts, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_exact<C, O>(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError>
where
    C: SymbolFactContract<Owner = O>,
    O: ExactSymbolId,
    for<'facts> CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
{
    let owner = O::try_from_any(symbol).ok_or(FactQueryError::InfrastructureFailure)?;

    force_typed::<C>(facts, owner)
}

fn force_typed<C>(
    facts: &CompilationBinderFacts<'_>,
    owner: C::Owner,
) -> Result<DiagnosticBag, FactQueryError>
where
    C: SymbolFactContract,
    for<'facts> CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
{
    let result = facts
        .symbol_fact(SymbolFactRequest::<C>::new(owner))
        .map_err(fact_error)?;

    // Completion aggregates independently owned fact diagnostics after all work succeeds.
    Ok(result.diagnostics().clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::SymbolFactProvider;
    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        CallableContractSymbolId, CallableContractTypeFact, CallableSignatureFact, ExactSymbolId,
        FunctionSymbolId, SelfTypeContext, StructFieldSymbolId, StructFieldTypeFact,
        StructSymbolId, SymbolFactRequest, TraitCallableMemberSymbolId, TraitSymbolId,
        TraitTypeFulfillmentSymbolId, TraitTypeFulfillmentValueFact, TypeData,
    };

    use super::{CancellationToken, Compilation, SymbolCompletionLevel};
    use crate::test_support::compilation;

    #[test]
    fn compiler_known_completion_binds_current_declaration_surfaces() {
        let compilation = compilation("module app;");
        let symbols = symbol_graph(&compilation);
        let root = symbols.compiler_known_environment().id().into();
        let cancellation = CancellationToken::new();

        let diagnostics = match compilation.force_complete_symbol(
            root,
            SymbolCompletionLevel::DeclarationSurface,
            &cancellation,
        ) {
            Ok(diagnostics) => diagnostics,
            Err(error) => panic!("compiler-known completion must succeed: {error:?}"),
        };

        assert!(diagnostics.is_empty());

        let facts = match compilation.binder_facts(&cancellation) {
            Ok(facts) => facts,
            Err(error) => panic!("compiler-known binder facts must be available: {error:?}"),
        };

        let copy = declaration::<FunctionSymbolId>(symbols, "MemoryCopy");
        let copy_request = SymbolFactRequest::<CallableSignatureFact>::new(copy.into());
        let first_copy = published_fact(&facts, copy_request);
        let second_copy = published_fact(&facts, copy_request);

        assert!(Arc::ptr_eq(&first_copy, &second_copy));
        assert_eq!(first_copy.value().parameters().len(), 3);

        let unary = declaration::<CallableContractSymbolId>(symbols, "UnaryCallable");
        let unary_type = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractTypeFact>::new(unary),
        );

        assert!(matches!(
            type_data(&compilation, *unary_type.value()),
            TypeData::Callable(_)
        ));

        let element = declaration::<StructFieldSymbolId>(symbols, "RawPointerElement");
        let element_type = published_fact(
            &facts,
            SymbolFactRequest::<StructFieldTypeFact>::new(element),
        );

        assert!(matches!(
            type_data(&compilation, *element_type.value()),
            TypeData::TypeParameter(_)
        ));

        let item = declaration::<TraitTypeFulfillmentSymbolId>(symbols, "BoolStorageItem");
        let item_type = published_fact(
            &facts,
            SymbolFactRequest::<TraitTypeFulfillmentValueFact>::new(item),
        );

        assert!(matches!(
            type_data(&compilation, *item_type.value()),
            TypeData::Named { definition, .. }
                if definition == declaration::<StructSymbolId>(symbols, "Bool").into()
        ));

        let load = declaration::<TraitCallableMemberSymbolId>(symbols, "StorageLoad");

        let load_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(load.into()),
        );

        let Some(receiver) = load_signature.value().receiver() else {
            panic!("trait callable must retain its contextual receiver");
        };

        assert_eq!(
            type_data(&compilation, receiver.ty()),
            TypeData::ContextualSelf(SelfTypeContext::Trait(declaration::<TraitSymbolId>(
                symbols, "Storage"
            )))
        );
    }

    fn published_fact<C>(
        facts: &super::CompilationBinderFacts<'_>,
        request: SymbolFactRequest<C>,
    ) -> Arc<bray_symbols::SymbolFactResult<C>>
    where
        C: bray_symbols::SymbolFactContract,
        for<'facts> super::CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
    {
        match facts.symbol_fact(request) {
            Ok(result) => result,
            Err(error) => panic!("compiler-known symbol fact must bind: {error:?}"),
        }
    }

    fn symbol_graph(compilation: &Compilation) -> &bray_symbols::SymbolGraph {
        match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("compiler-known symbol graph must build: {error:?}"),
        }
    }

    fn declaration<I>(symbols: &bray_symbols::SymbolGraph, key: &str) -> I
    where
        I: ExactSymbolId,
    {
        let Some(key) = CompilerKnownDeclarationKey::try_new(key) else {
            panic!("compiler-known test key must be valid");
        };

        match symbols
            .compiler_known_provider()
            .declaration_symbol::<I>(&key)
        {
            Some(symbol) => symbol,
            None => panic!("compiler-known declaration must have the requested symbol kind"),
        }
    }

    fn type_data(compilation: &Compilation, ty: bray_symbols::TypeId) -> TypeData {
        let values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic value store must be available: {error:?}"),
        };

        match values.type_data(ty) {
            Ok(data) => data.as_ref().clone(),
            Err(error) => panic!("semantic type must be interned: {error:?}"),
        }
    }
}
