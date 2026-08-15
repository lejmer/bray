use bray_binder::SymbolFactProvider;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableContractTemplateQuery, CallableContractTypeQuery,
    CallableContractsQuery, CallableOverloadTemplateQuery, CallableParameterDefaultQuery,
    CallableParameterDefaultTemplateQuery, CallableParameterSymbolId, CallableSignatureQuery,
    CallableSymbolId, ConstantDeclaredTypeQuery, ConstantDefinitionQuery, DeclarationDirectivesQuery,
    ExactSymbolId, GenericConstParameterDeclaredTypeQuery, GenericConstraintsQuery,
    GenericDeclarationTemplateQuery, GenericOwnerId, ImplementationCoherenceQuery,
    ImplementationHeadTemplateQuery, ImplementationOverloadTemplateQuery, ImplementationSubjectQuery,
    ImplementationSymbolId, ImplementedTraitApplicationQuery, InherentTypeMemberValueQuery,
    ModuleSurfaceQuery, ModuleSymbolId, PredicateDefinitionQuery, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateQuery, StructFieldDefaultQuery, StructFieldDefaultTemplateQuery,
    StructFieldSymbolId, StructFieldTypeQuery, SymbolCompletionLevel, SymbolFactCompletionRequest,
    SymbolFactContract, SymbolFactForcer, SymbolFactKind, SymbolFactRequest,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantFulfillmentDefinitionQuery,
    TraitConstantMemberDeclaredTypeQuery, TraitConstantMemberDefinitionQuery,
    TraitPredicateFulfillmentDefinitionQuery, TraitPredicateMemberDefinitionQuery,
    TraitTypeFulfillmentValueQuery, UnionPayloadFieldDefaultQuery,
    UnionPayloadFieldDefaultTemplateQuery, UnionPayloadFieldSymbolId, UnionPayloadFieldTypeQuery,
};

use super::super::binder_fact_error;
use super::super::context::CompilationBindingContext;
use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError, SymbolCompletionError};

impl SymbolFactForcer for CompilationBindingContext<'_> {
    type Error = FactQueryError;

    fn force(&self, request: SymbolFactCompletionRequest) -> Result<DiagnosticBag, Self::Error> {
        match request.kind() {
            // These surfaces are frozen into the immutable symbol graph before semantic binding.
            SymbolFactKind::Members
            | SymbolFactKind::GenericParameters
            | SymbolFactKind::UnionVariantPayload => Ok(DiagnosticBag::new()),
            SymbolFactKind::Imports => {
                force_exact::<ModuleSurfaceQuery, ModuleSymbolId>(self, request.symbol())
            }
            SymbolFactKind::Directives => {
                force_typed::<DeclarationDirectivesQuery>(self, request.symbol())
            }
            SymbolFactKind::GenericConstraints => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<GenericConstraintsQuery>(self, owner)
            }
            SymbolFactKind::GenericDeclarationTemplate => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<GenericDeclarationTemplateQuery>(self, owner)
            }
            SymbolFactKind::CallableSignature => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<CallableSignatureQuery>(self, owner)
            }
            SymbolFactKind::CallableContracts => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<CallableContractsQuery>(self, owner)
            }
            SymbolFactKind::CallableContractTemplate => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<CallableContractTemplateQuery>(self, owner)
            }
            SymbolFactKind::PredicateSignatureTemplate => {
                let owner = PredicateDefinitionSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<PredicateSignatureTemplateQuery>(self, owner)
            }
            SymbolFactKind::CallableContractType => force_exact::<
                CallableContractTypeQuery,
                CallableContractSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::ConstantDeclaredType => {
                force_constant_declared_type(self, request.symbol())
            }
            SymbolFactKind::ConstantDefinition => force_constant_definition(self, request.symbol()),
            SymbolFactKind::StructFieldType => {
                force_exact::<StructFieldTypeQuery, StructFieldSymbolId>(self, request.symbol())
            }
            SymbolFactKind::TypeMemberValue => force_type_member_value(self, request.symbol()),
            SymbolFactKind::ImplementationSubject => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementationSubjectQuery>(self, owner)
            }
            SymbolFactKind::ImplementedTraitApplication => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementedTraitApplicationQuery>(self, owner)
            }
            SymbolFactKind::ImplementationCoherence => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementationCoherenceQuery>(self, owner)
            }
            SymbolFactKind::UnionPayloadFieldType => force_exact::<
                UnionPayloadFieldTypeQuery,
                UnionPayloadFieldSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::UnevaluatedDefaultTemplate => {
                force_unevaluated_default(self, request.symbol())
            }
            SymbolFactKind::CallableParameterDefault => force_exact::<
                CallableParameterDefaultQuery,
                CallableParameterSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::StructFieldDefault => {
                force_exact::<StructFieldDefaultQuery, StructFieldSymbolId>(self, request.symbol())
            }
            SymbolFactKind::UnionPayloadFieldDefault => force_exact::<
                UnionPayloadFieldDefaultQuery,
                UnionPayloadFieldSymbolId,
            >(self, request.symbol()),
            SymbolFactKind::PredicateDefinition => {
                force_predicate_definition(self, request.symbol())
            }
            SymbolFactKind::ImplementationHeadTemplate => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                force_typed::<ImplementationHeadTemplateQuery>(self, owner)
            }
            SymbolFactKind::OverloadSignatureTemplate => {
                force_overload_template(self, request.symbol())
            }
            SymbolFactKind::OverloadArms => Err(FactQueryError::InfrastructureFailure),
        }
    }
}

fn force_predicate_definition(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Predicate(owner) => force_typed::<PredicateDefinitionQuery>(binding_context, owner),
        AnySymbolId::TraitPredicateMember(owner) => {
            force_typed::<TraitPredicateMemberDefinitionQuery>(binding_context, owner)
        }
        AnySymbolId::TraitPredicateFulfillment(owner) => {
            force_typed::<TraitPredicateFulfillmentDefinitionQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_unevaluated_default(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::CallableParameter(owner) => {
            force_typed::<CallableParameterDefaultTemplateQuery>(binding_context, owner)
        }
        AnySymbolId::StructField(owner) => {
            force_typed::<StructFieldDefaultTemplateQuery>(binding_context, owner)
        }
        AnySymbolId::UnionPayloadField(owner) => {
            force_typed::<UnionPayloadFieldDefaultTemplateQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_overload_template(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::CallableOverload(owner) => {
            force_typed::<CallableOverloadTemplateQuery>(binding_context, owner)
        }
        AnySymbolId::ImplementationOverload(owner) => {
            force_typed::<ImplementationOverloadTemplateQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_constant_declared_type(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Constant(owner) => force_typed::<ConstantDeclaredTypeQuery>(binding_context, owner),
        AnySymbolId::TraitConstantMember(owner) => {
            force_typed::<TraitConstantMemberDeclaredTypeQuery>(binding_context, owner)
        }
        AnySymbolId::TraitConstantFulfillment(owner) => {
            force_typed::<TraitConstantFulfillmentDeclaredTypeQuery>(binding_context, owner)
        }
        AnySymbolId::GenericConstParameter(owner) => {
            force_typed::<GenericConstParameterDeclaredTypeQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_constant_definition(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Constant(owner) => force_typed::<ConstantDefinitionQuery>(binding_context, owner),
        AnySymbolId::TraitConstantMember(owner) => {
            force_typed::<TraitConstantMemberDefinitionQuery>(binding_context, owner)
        }
        AnySymbolId::TraitConstantFulfillment(owner) => {
            force_typed::<TraitConstantFulfillmentDefinitionQuery>(binding_context, owner)
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
        let binding_context = self
            .binding_context(cancellation)
            .map_err(SymbolCompletionError::Provider)?;

        crate::fact::force_complete_symbol(
            binding_context.symbols,
            root,
            level,
            &self.state.fact_runtime,
            cancellation,
            &binding_context,
        )
    }
}

fn force_type_member_value(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::InherentTypeMember(owner) => {
            force_typed::<InherentTypeMemberValueQuery>(binding_context, owner)
        }
        AnySymbolId::TraitTypeFulfillment(owner) => {
            force_typed::<TraitTypeFulfillmentValueQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn force_exact<C, O>(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError>
where
    C: SymbolFactContract<Owner = O>,
    O: ExactSymbolId,
    for<'binding_context> CompilationBindingContext<'binding_context>: SymbolFactProvider<C>,
{
    let owner = O::try_from_any(symbol).ok_or(FactQueryError::InfrastructureFailure)?;

    force_typed::<C>(binding_context, owner)
}

fn force_typed<C>(
    binding_context: &CompilationBindingContext<'_>,
    owner: C::Owner,
) -> Result<DiagnosticBag, FactQueryError>
where
    C: SymbolFactContract,
    for<'binding_context> CompilationBindingContext<'binding_context>: SymbolFactProvider<C>,
{
    let result = binding_context
        .symbol_fact(SymbolFactRequest::<C>::new(owner))
        .map_err(binder_fact_error)?;

    // Completion aggregates independently owned query diagnostics after all work succeeds.
    Ok(result.diagnostics().clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        CallableContractSymbolId, CallableContractTemplate, CallableContractTemplateQuery,
        CallableContractTypeQuery, CallableContractsQuery, CallableExecution, CallableSignatureQuery,
        ExactSymbolId, FunctionSymbolId, GenericConstraintsQuery, GenericDeclarationTemplateQuery,
        GenericOwnerId, ImplementationCoherenceQuery, ImplementationSymbolId,
        NamedTraitImplementationSymbolId, StructFieldSymbolId, StructFieldTypeQuery, StructSymbolId,
        SymbolFactRequest, SymbolOrdinal, TraitCallableMemberSymbolId, TypeCallableMemberSymbolId,
        TypeData, UnionPayloadFieldSymbolId, UnionPayloadFieldTypeQuery,
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

        let binding_context = match compilation.binding_context(&cancellation) {
            Ok(binding_context) => binding_context,
            Err(error) => panic!("compiler-known binder context must be available: {error:?}"),
        };

        let copy = declaration::<FunctionSymbolId>(symbols, "MemoryCopy");
        let copy_request = SymbolFactRequest::<CallableSignatureQuery>::new(copy.into());
        let first_copy = published_fact(&binding_context, copy_request);
        let second_copy = published_fact(&binding_context, copy_request);

        assert!(Arc::ptr_eq(&first_copy, &second_copy));
        assert_eq!(first_copy.value().parameters().len(), 3);

        let copy_contract_template = published_fact(
            &binding_context,
            SymbolFactRequest::<CallableContractTemplateQuery>::new(copy.into()),
        );

        assert!(matches!(
            copy_contract_template.value(),
            CallableContractTemplate::Source(contract)
                if contract.expressions().len() == 5 && contract.capabilities().len() == 2
        ));

        let storage_destroy = declaration::<TraitCallableMemberSymbolId>(symbols, "StorageDestroy");

        let storage_destroy_contract = published_fact(
            &binding_context,
            SymbolFactRequest::<CallableContractsQuery>::new(storage_destroy.into()),
        );

        let trusted_capabilities = storage_destroy_contract
            .value()
            .invocation_behavior()
            .trusted_capabilities();

        assert_eq!(trusted_capabilities.len(), 2);
        assert_eq!(trusted_capabilities[0].ordinal(), SymbolOrdinal::new(0));
        assert_eq!(trusted_capabilities[1].ordinal(), SymbolOrdinal::new(1));

        assert_ne!(
            trusted_capabilities[0].capability(),
            trusted_capabilities[1].capability()
        );

        let pointer = declaration::<StructSymbolId>(symbols, "RawPointer");

        let Some(pointer_owner) = GenericOwnerId::try_new(pointer.into()) else {
            panic!("raw pointer must be a generic owner");
        };

        let pointer_constraints = published_fact(
            &binding_context,
            SymbolFactRequest::<GenericConstraintsQuery>::new(pointer_owner),
        );

        assert_eq!(pointer_constraints.value().constraints().len(), 1);

        let pointer_template = published_fact(
            &binding_context,
            SymbolFactRequest::<GenericDeclarationTemplateQuery>::new(pointer_owner),
        );

        assert_eq!(pointer_template.value().parameters().len(), 1);
        assert_eq!(pointer_template.value().constraints().len(), 1);

        let storage_implementation =
            declaration::<NamedTraitImplementationSymbolId>(symbols, "HeapStorageImplementation");

        let coherence = published_fact(
            &binding_context,
            SymbolFactRequest::<ImplementationCoherenceQuery>::new(ImplementationSymbolId::from(
                storage_implementation,
            )),
        );

        assert!(coherence.value().trait_application().is_some());

        let unary = declaration::<CallableContractSymbolId>(symbols, "UnaryCallable");

        let unary_type = published_fact(
            &binding_context,
            SymbolFactRequest::<CallableContractTypeQuery>::new(unary),
        );

        assert!(matches!(
            type_data(&compilation, unary_type.value()).as_ref(),
            TypeData::Callable(_)
        ));

        let element = declaration::<StructFieldSymbolId>(symbols, "RawPointerElement");

        let element_type = published_fact(
            &binding_context,
            SymbolFactRequest::<StructFieldTypeQuery>::new(element),
        );

        assert!(matches!(
            type_data(&compilation, element_type.value()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        let completed_value =
            declaration::<UnionPayloadFieldSymbolId>(symbols, "RunResultVariant0CompletedValue");

        let completed_value_type = published_fact(
            &binding_context,
            SymbolFactRequest::<UnionPayloadFieldTypeQuery>::new(completed_value),
        );

        assert!(matches!(
            type_data(&compilation, completed_value_type.value()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        let start = declaration::<TypeCallableMemberSymbolId>(symbols, "FutureStart");

        let start_signature = published_fact(
            &binding_context,
            SymbolFactRequest::<CallableSignatureQuery>::new(start.into()),
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
            &binding_context,
            SymbolFactRequest::<CallableSignatureQuery>::new(join.into()),
        );

        assert!(matches!(
            type_data(&compilation, join_signature.value().callable_type()).as_ref(),
            TypeData::Callable(callable)
                if callable.execution() == CallableExecution::Asynchronous
        ));

        let join_contracts = published_fact(
            &binding_context,
            SymbolFactRequest::<CallableContractsQuery>::new(join.into()),
        );

        assert!(
            join_contracts
                .value()
                .deferred_execution_behavior()
                .is_some()
        );

        let borrow = declaration::<TraitCallableMemberSymbolId>(symbols, "StorageBorrow");

        let borrow_signature = published_fact(
            &binding_context,
            SymbolFactRequest::<CallableSignatureQuery>::new(borrow.into()),
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
