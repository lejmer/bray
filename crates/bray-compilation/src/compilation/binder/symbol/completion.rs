use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableContractTemplateFact, CallableContractTypeFact,
    CallableContractsFact, CallableOverloadTemplateFact, CallableParameterDefaultFact,
    CallableParameterDefaultTemplateFact, CallableParameterSymbolId, CallableSignatureFact,
    CallableSymbolId, ConstantDeclaredTypeFact, ConstantDefinitionFact, ExactSymbolId,
    GenericConstParameterDeclaredTypeFact, GenericConstraintsFact, GenericDeclarationTemplateFact,
    GenericOwnerId, ImplementationCoherenceFact, ImplementationHeadTemplateFact,
    ImplementationOverloadTemplateFact, ImplementationSubjectFact, ImplementationSymbolId,
    ImplementedTraitApplicationFact, InherentTypeMemberValueFact, ModuleSurfaceFact,
    ModuleSymbolId, PredicateDefinitionFact, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateFact, StructFieldDefaultFact, StructFieldDefaultTemplateFact,
    StructFieldSymbolId, StructFieldTypeFact, SymbolCompletionLevel, SymbolFactCompletionRequest,
    SymbolFactContract, SymbolFactForcer, SymbolFactKind, SymbolFactRequest,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantFulfillmentDefinitionFact,
    TraitConstantMemberDeclaredTypeFact, TraitConstantMemberDefinitionFact,
    TraitPredicateFulfillmentDefinitionFact, TraitPredicateMemberDefinitionFact,
    TraitTypeFulfillmentValueFact, UnionPayloadFieldDefaultFact,
    UnionPayloadFieldDefaultTemplateFact, UnionPayloadFieldSymbolId, UnionPayloadFieldTypeFact,
};

use super::super::binder_fact_error;
use super::super::context::CompilationBinderFacts;
use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError, SymbolCompletionError};

impl SymbolFactForcer for CompilationBinderFacts<'_> {
    type Error = FactQueryError;

    fn force(&self, request: SymbolFactCompletionRequest) -> Result<DiagnosticBag, Self::Error> {
        match request.kind() {
            // These surfaces are frozen into the immutable symbol graph before semantic facts.
            SymbolFactKind::Members
            | SymbolFactKind::Directives
            | SymbolFactKind::GenericParameters
            | SymbolFactKind::UnionVariantPayload => Ok(DiagnosticBag::new()),
            SymbolFactKind::Imports => {
                force_exact::<ModuleSurfaceFact, ModuleSymbolId>(self, request.symbol())
            }
            SymbolFactKind::GenericConstraints => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<GenericConstraintsFact>(self, owner)
            }
            SymbolFactKind::GenericDeclarationTemplate => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<GenericDeclarationTemplateFact>(self, owner)
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
            SymbolFactKind::CallableContractTemplate => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<CallableContractTemplateFact>(self, owner)
            }
            SymbolFactKind::PredicateSignatureTemplate => {
                let owner = PredicateDefinitionSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<PredicateSignatureTemplateFact>(self, owner)
            }
            SymbolFactKind::CallableContractType => force_exact::<
                CallableContractTypeFact,
                CallableContractSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::ConstantDeclaredType => {
                force_constant_declared_type(self, request.symbol())
            }
            SymbolFactKind::ConstantDefinition => force_constant_definition(self, request.symbol()),
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
            SymbolFactKind::UnevaluatedDefaultTemplate => {
                force_unevaluated_default(self, request.symbol())
            }
            SymbolFactKind::CallableParameterDefault => force_exact::<
                CallableParameterDefaultFact,
                CallableParameterSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::StructFieldDefault => {
                force_exact::<StructFieldDefaultFact, StructFieldSymbolId>(self, request.symbol())
            }
            SymbolFactKind::UnionPayloadFieldDefault => force_exact::<
                UnionPayloadFieldDefaultFact,
                UnionPayloadFieldSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::PredicateDefinition => {
                force_predicate_definition(self, request.symbol())
            }
            SymbolFactKind::ImplementationHeadTemplate => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementationHeadTemplateFact>(self, owner)
            }
            SymbolFactKind::OverloadSignatureTemplate => {
                force_overload_template(self, request.symbol())
            }
            SymbolFactKind::OverloadArms => Err(FactQueryError::InfrastructureFailure),
        }
    }
}

fn force_predicate_definition(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Predicate(owner) => force_typed::<PredicateDefinitionFact>(facts, owner),
        AnySymbolId::TraitPredicateMember(owner) => {
            force_typed::<TraitPredicateMemberDefinitionFact>(facts, owner)
        }
        AnySymbolId::TraitPredicateFulfillment(owner) => {
            force_typed::<TraitPredicateFulfillmentDefinitionFact>(facts, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_unevaluated_default(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::CallableParameter(owner) => {
            force_typed::<CallableParameterDefaultTemplateFact>(facts, owner)
        }
        AnySymbolId::StructField(owner) => {
            force_typed::<StructFieldDefaultTemplateFact>(facts, owner)
        }
        AnySymbolId::UnionPayloadField(owner) => {
            force_typed::<UnionPayloadFieldDefaultTemplateFact>(facts, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_overload_template(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::CallableOverload(owner) => {
            force_typed::<CallableOverloadTemplateFact>(facts, owner)
        }
        AnySymbolId::ImplementationOverload(owner) => {
            force_typed::<ImplementationOverloadTemplateFact>(facts, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
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
        AnySymbolId::GenericConstParameter(owner) => {
            force_typed::<GenericConstParameterDeclaredTypeFact>(facts, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_constant_definition(
    facts: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Constant(owner) => force_typed::<ConstantDefinitionFact>(facts, owner),
        AnySymbolId::TraitConstantMember(owner) => {
            force_typed::<TraitConstantMemberDefinitionFact>(facts, owner)
        }
        AnySymbolId::TraitConstantFulfillment(owner) => {
            force_typed::<TraitConstantFulfillmentDefinitionFact>(facts, owner)
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
        .map_err(binder_fact_error)?;

    // Completion aggregates independently owned fact diagnostics after all work succeeds.
    Ok(result.diagnostics().clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        CallableContractSymbolId, CallableContractTemplate, CallableContractTemplateFact,
        CallableContractTypeFact, CallableContractsFact, CallableExecution, CallableSignatureFact,
        ExactSymbolId, FunctionSymbolId, GenericConstraintsFact, GenericDeclarationTemplateFact,
        GenericOwnerId, ImplementationCoherenceFact, ImplementationSymbolId,
        NamedTraitImplementationSymbolId, StructFieldSymbolId, StructFieldTypeFact, StructSymbolId,
        SymbolFactRequest, TraitCallableMemberSymbolId, TypeCallableMemberSymbolId, TypeData,
        UnionPayloadFieldSymbolId, UnionPayloadFieldTypeFact,
    };

    use super::{CancellationToken, SymbolCompletionLevel};
    use crate::compilation::binder::symbol::test_support::{
        published_fact, symbol_graph, type_data,
    };
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

        // TODO(BRA-270): Assert checked contracts once capability paths have typed identities.
        let copy_contract_template = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractTemplateFact>::new(copy.into()),
        );

        assert!(matches!(
            copy_contract_template.value(),
            CallableContractTemplate::Source(contract)
                if contract.expressions().len() == 5 && contract.capabilities().len() == 2
        ));

        let pointer = declaration::<StructSymbolId>(symbols, "RawPointer");

        let Some(pointer_owner) = GenericOwnerId::try_new(pointer.into()) else {
            panic!("raw pointer must be a generic owner");
        };

        let pointer_constraints = published_fact(
            &facts,
            SymbolFactRequest::<GenericConstraintsFact>::new(pointer_owner),
        );

        assert_eq!(pointer_constraints.value().constraints().len(), 1);

        let pointer_template = published_fact(
            &facts,
            SymbolFactRequest::<GenericDeclarationTemplateFact>::new(pointer_owner),
        );

        assert_eq!(pointer_template.value().parameters().len(), 1);
        assert_eq!(pointer_template.value().constraints().len(), 1);

        let storage_implementation =
            declaration::<NamedTraitImplementationSymbolId>(symbols, "HeapStorageImplementation");

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
            type_data(&compilation, unary_type.value()).as_ref(),
            TypeData::Callable(_)
        ));

        let element = declaration::<StructFieldSymbolId>(symbols, "RawPointerElement");
        let element_type = published_fact(
            &facts,
            SymbolFactRequest::<StructFieldTypeFact>::new(element),
        );

        assert!(matches!(
            type_data(&compilation, element_type.value()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        let completed_value =
            declaration::<UnionPayloadFieldSymbolId>(symbols, "RunResultVariant0CompletedValue");

        let completed_value_type = published_fact(
            &facts,
            SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(completed_value),
        );

        assert!(matches!(
            type_data(&compilation, completed_value_type.value()).as_ref(),
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
            type_data(&compilation, start_signature.value().result()).as_ref(),
            TypeData::Named { definition, .. }
                if *definition == declaration::<StructSymbolId>(symbols, "Task").into()
        ));

        let join = declaration::<TypeCallableMemberSymbolId>(symbols, "TaskJoin");
        let join_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(join.into()),
        );

        assert!(matches!(
            type_data(&compilation, join_signature.value().callable_type()).as_ref(),
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

        let borrow = declaration::<TraitCallableMemberSymbolId>(symbols, "StorageBorrow");

        let borrow_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(borrow.into()),
        );

        assert!(borrow_signature.value().receiver().is_none());
        assert_eq!(borrow_signature.value().parameters().len(), 1);
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
}
