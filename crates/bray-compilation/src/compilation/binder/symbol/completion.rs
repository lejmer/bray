use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableContractTypeFact, CallableContractsFact,
    CallableSignatureFact, CallableSymbolId, ConstantDeclaredTypeFact, ExactSymbolId,
    GenericConstraintsFact, GenericOwnerId, ImplementationCoherenceFact, ImplementationSubjectFact,
    ImplementationSymbolId, ImplementedTraitApplicationFact, InherentTypeMemberValueFact,
    StructFieldSymbolId, StructFieldTypeFact, SymbolCompletionLevel, SymbolFactCompletionRequest,
    SymbolFactContract, SymbolFactForcer, SymbolFactKind, SymbolFactRequest,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
    TraitTypeFulfillmentValueFact, UnionPayloadFieldSymbolId, UnionPayloadFieldTypeFact,
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
            SymbolFactKind::ConstantDeclaredType => {
                force_constant_declared_type(self, request.symbol())
            }
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
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementationCoherenceFact>(self, owner)
            }
            SymbolFactKind::UnionPayloadFieldType => force_exact::<
                UnionPayloadFieldTypeFact,
                UnionPayloadFieldSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::ConstantDefinition
            | SymbolFactKind::CallableParameterDefault
            | SymbolFactKind::StructFieldDefault
            | SymbolFactKind::UnionPayloadFieldDefault
            | SymbolFactKind::PredicateDefinition
            | SymbolFactKind::OverloadArms => Err(FactQueryError::InfrastructureFailure),
        }
    }
}

fn force_constant_declared_type(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Constant(owner) => force_typed::<ConstantDeclaredTypeFact>(facts, owner),
        AnySymbolId::TraitConstantMember(owner) => {
            force_typed::<TraitConstantMemberDeclaredTypeFact>(facts, owner)
        }
        AnySymbolId::TraitConstantFulfillment(owner) => {
            force_typed::<TraitConstantFulfillmentDeclaredTypeFact>(facts, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
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
        CallableContractSymbolId, CallableContractTypeFact, CallableContractsFact,
        CallableExecution, CallableSignatureFact, ExactSymbolId, FunctionSymbolId,
        GenericConstraintsFact, GenericOwnerId, ImplementationCoherenceFact,
        ImplementationSymbolId, NamedTraitImplementationSymbolId, SelfTypeContext,
        StructFieldSymbolId, StructFieldTypeFact, StructSymbolId, SymbolFactRequest,
        TraitCallableMemberSymbolId, TraitSymbolId, TraitTypeFulfillmentSymbolId,
        TraitTypeFulfillmentValueFact, TypeCallableMemberSymbolId, TypeData,
        UnionPayloadFieldSymbolId, UnionPayloadFieldTypeFact,
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

        let copy_contracts = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractsFact>::new(copy.into()),
        );

        assert_eq!(copy_contracts.value().invocation_preconditions().len(), 1);
        assert_eq!(copy_contracts.value().static_constraints().len(), 1);

        assert_eq!(
            copy_contracts
                .value()
                .normal_completion_postconditions()
                .len(),
            1
        );

        assert!(
            copy_contracts
                .value()
                .deferred_execution_behavior()
                .is_none()
        );

        let pointer = declaration::<StructSymbolId>(symbols, "RawPointer");

        let Some(pointer_owner) = GenericOwnerId::try_new(pointer.into()) else {
            panic!("raw pointer must be a generic owner");
        };

        let pointer_constraints = published_fact(
            &facts,
            SymbolFactRequest::<GenericConstraintsFact>::new(pointer_owner),
        );

        assert_eq!(pointer_constraints.value().constraints().len(), 1);

        let storage_implementation =
            declaration::<NamedTraitImplementationSymbolId>(symbols, "BoolStorageImplementation");

        let coherence = published_fact(
            &facts,
            SymbolFactRequest::<ImplementationCoherenceFact>::new(ImplementationSymbolId::from(
                storage_implementation,
            )),
        );

        assert!(coherence.value().trait_application().is_some());

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

        let completed_value =
            declaration::<UnionPayloadFieldSymbolId>(symbols, "RunResultVariant0CompletedValue");

        let completed_value_type = published_fact(
            &facts,
            SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(completed_value),
        );

        assert!(matches!(
            type_data(&compilation, *completed_value_type.value()),
            TypeData::TypeParameter(_)
        ));

        let start = declaration::<TypeCallableMemberSymbolId>(symbols, "FutureStart");
        let start_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(start.into()),
        );

        assert!(start_signature.value().parameters().is_empty());
        assert!(start_signature.value().receiver().is_some());

        assert!(matches!(
            type_data(&compilation, start_signature.value().result()),
            TypeData::Named { definition, .. }
                if definition == declaration::<StructSymbolId>(symbols, "Task").into()
        ));

        let join = declaration::<TypeCallableMemberSymbolId>(symbols, "TaskJoin");
        let join_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(join.into()),
        );

        assert!(matches!(
            type_data(&compilation, join_signature.value().callable_type()),
            TypeData::Callable(callable)
                if callable.execution() == CallableExecution::Asynchronous
        ));

        let join_contracts = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractsFact>::new(join.into()),
        );

        assert!(
            join_contracts
                .value()
                .deferred_execution_behavior()
                .is_some()
        );

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
