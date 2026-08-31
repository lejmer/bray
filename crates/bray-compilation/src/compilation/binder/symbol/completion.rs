use bray_binder::SymbolQueryProvider;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableContractSymbolId, CallableContractTemplateQuery,
    CallableContractTypeQuery, CallableContractsQuery, CallableOverloadTemplateQuery,
    CallableParameterDefaultQuery, CallableParameterDefaultTemplateQuery,
    CallableParameterSymbolId, CallableSignatureQuery, CallableSymbolId, ConstantDeclaredTypeQuery,
    ConstantDefinitionQuery, DeclarationDirectivesQuery, ExactSymbolId,
    GenericConstParameterDeclaredTypeQuery, GenericConstraintsQuery,
    GenericDeclarationTemplateQuery, GenericOwnerId, ImplementationCoherenceQuery,
    ImplementationHeadTemplateQuery, ImplementationOverloadTemplateQuery,
    ImplementationSubjectQuery, ImplementationSymbolId, ImplementedTraitApplicationQuery,
    InherentTypeMemberValueQuery, ModuleSurfaceQuery, ModuleSymbolId, PredicateDefinitionQuery,
    PredicateDefinitionSymbolId, PredicateSignatureTemplateQuery, StaticDeclaredTypeQuery,
    StaticInstanceTemplateQuery, StructFieldDefaultQuery, StructFieldDefaultTemplateQuery,
    StructFieldSymbolId, StructFieldTypeQuery, SymbolCompletionEvaluator, SymbolCompletionLevel,
    SymbolCompletionQuery, SymbolQueryContract, SymbolQueryKind, SymbolQueryRequest,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantFulfillmentDefinitionQuery,
    TraitConstantMemberDeclaredTypeQuery, TraitConstantMemberDefinitionQuery,
    TraitPredicateFulfillmentDefinitionQuery, TraitPredicateMemberDefinitionQuery,
    TraitTypeFulfillmentValueQuery, UnionPayloadFieldDefaultQuery,
    UnionPayloadFieldDefaultTemplateQuery, UnionPayloadFieldSymbolId, UnionPayloadFieldTypeQuery,
};

use super::super::binding_query_error;
use super::super::context::CompilationBindingContext;
use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError, SymbolCompletionError};

impl SymbolCompletionEvaluator for CompilationBindingContext<'_> {
    type Error = FactQueryError;

    fn evaluate(&self, request: SymbolCompletionQuery) -> Result<DiagnosticBag, Self::Error> {
        match request.kind() {
            // These surfaces are frozen into the immutable symbol graph before semantic binding.
            SymbolQueryKind::Members
            | SymbolQueryKind::GenericParameters
            | SymbolQueryKind::UnionVariantPayload => Ok(DiagnosticBag::new()),
            SymbolQueryKind::Imports => {
                evaluate_exact::<ModuleSurfaceQuery, ModuleSymbolId>(self, request.symbol())
            }
            SymbolQueryKind::Directives => {
                evaluate_typed::<DeclarationDirectivesQuery>(self, request.symbol())
            }
            SymbolQueryKind::GenericConstraints => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<GenericConstraintsQuery>(self, owner)
            }
            SymbolQueryKind::GenericDeclarationTemplate => {
                let owner = GenericOwnerId::try_new(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<GenericDeclarationTemplateQuery>(self, owner)
            }
            SymbolQueryKind::CallableSignature => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<CallableSignatureQuery>(self, owner)
            }
            SymbolQueryKind::CallableContracts => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<CallableContractsQuery>(self, owner)
            }
            SymbolQueryKind::CallableContractTemplate => {
                let owner = CallableSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<CallableContractTemplateQuery>(self, owner)
            }
            SymbolQueryKind::PredicateSignatureTemplate => {
                let owner = PredicateDefinitionSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<PredicateSignatureTemplateQuery>(self, owner)
            }
            SymbolQueryKind::CallableContractType => evaluate_exact::<
                CallableContractTypeQuery,
                CallableContractSymbolId,
            >(self, request.symbol()),
            SymbolQueryKind::ConstantDeclaredType => {
                evaluate_constant_declared_type(self, request.symbol())
            }
            SymbolQueryKind::ConstantDefinition => {
                evaluate_constant_definition(self, request.symbol())
            }
            SymbolQueryKind::StaticInstanceTemplate => evaluate_exact::<
                StaticInstanceTemplateQuery,
                bray_symbols::StaticSymbolId,
            >(self, request.symbol()),
            SymbolQueryKind::StructFieldType => {
                evaluate_exact::<StructFieldTypeQuery, StructFieldSymbolId>(self, request.symbol())
            }
            SymbolQueryKind::TypeMemberValue => evaluate_type_member_value(self, request.symbol()),
            SymbolQueryKind::ImplementationSubject => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<ImplementationSubjectQuery>(self, owner)
            }
            SymbolQueryKind::ImplementedTraitApplication => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<ImplementedTraitApplicationQuery>(self, owner)
            }
            SymbolQueryKind::ImplementationCoherence => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<ImplementationCoherenceQuery>(self, owner)
            }
            SymbolQueryKind::UnionPayloadFieldType => evaluate_exact::<
                UnionPayloadFieldTypeQuery,
                UnionPayloadFieldSymbolId,
            >(self, request.symbol()),
            SymbolQueryKind::UnevaluatedDefaultTemplate => {
                evaluate_unevaluated_default(self, request.symbol())
            }
            SymbolQueryKind::CallableParameterDefault => evaluate_exact::<
                CallableParameterDefaultQuery,
                CallableParameterSymbolId,
            >(self, request.symbol()),
            SymbolQueryKind::StructFieldDefault => evaluate_exact::<
                StructFieldDefaultQuery,
                StructFieldSymbolId,
            >(self, request.symbol()),
            SymbolQueryKind::UnionPayloadFieldDefault => evaluate_exact::<
                UnionPayloadFieldDefaultQuery,
                UnionPayloadFieldSymbolId,
            >(self, request.symbol()),
            SymbolQueryKind::PredicateDefinition => {
                evaluate_predicate_definition(self, request.symbol())
            }
            SymbolQueryKind::ImplementationHeadTemplate => {
                let owner = ImplementationSymbolId::try_from_any(request.symbol())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                evaluate_typed::<ImplementationHeadTemplateQuery>(self, owner)
            }
            SymbolQueryKind::OverloadSignatureTemplate => {
                evaluate_overload_template(self, request.symbol())
            }
            SymbolQueryKind::OverloadArms => Err(FactQueryError::InfrastructureFailure),
        }
    }
}

fn evaluate_predicate_definition(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Predicate(owner) => {
            evaluate_typed::<PredicateDefinitionQuery>(binding_context, owner)
        }
        AnySymbolId::TraitPredicateMember(owner) => {
            evaluate_typed::<TraitPredicateMemberDefinitionQuery>(binding_context, owner)
        }
        AnySymbolId::TraitPredicateFulfillment(owner) => {
            evaluate_typed::<TraitPredicateFulfillmentDefinitionQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn evaluate_unevaluated_default(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::CallableParameter(owner) => {
            evaluate_typed::<CallableParameterDefaultTemplateQuery>(binding_context, owner)
        }
        AnySymbolId::StructField(owner) => {
            evaluate_typed::<StructFieldDefaultTemplateQuery>(binding_context, owner)
        }
        AnySymbolId::UnionPayloadField(owner) => {
            evaluate_typed::<UnionPayloadFieldDefaultTemplateQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn evaluate_overload_template(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::CallableOverload(owner) => {
            evaluate_typed::<CallableOverloadTemplateQuery>(binding_context, owner)
        }
        AnySymbolId::ImplementationOverload(owner) => {
            evaluate_typed::<ImplementationOverloadTemplateQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn evaluate_constant_declared_type(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Constant(owner) => {
            evaluate_typed::<ConstantDeclaredTypeQuery>(binding_context, owner)
        }
        AnySymbolId::Static(owner) => {
            evaluate_typed::<StaticDeclaredTypeQuery>(binding_context, owner)
        }
        AnySymbolId::TraitConstantMember(owner) => {
            evaluate_typed::<TraitConstantMemberDeclaredTypeQuery>(binding_context, owner)
        }
        AnySymbolId::TraitConstantFulfillment(owner) => {
            evaluate_typed::<TraitConstantFulfillmentDeclaredTypeQuery>(binding_context, owner)
        }
        AnySymbolId::GenericConstParameter(owner) => {
            evaluate_typed::<GenericConstParameterDeclaredTypeQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn evaluate_constant_definition(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::Constant(owner) => {
            evaluate_typed::<ConstantDefinitionQuery>(binding_context, owner)
        }
        AnySymbolId::TraitConstantMember(owner) => {
            evaluate_typed::<TraitConstantMemberDefinitionQuery>(binding_context, owner)
        }
        AnySymbolId::TraitConstantFulfillment(owner) => {
            evaluate_typed::<TraitConstantFulfillmentDefinitionQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

impl Compilation {
    /// Completes one symbol subtree to the requested semantic boundary.
    pub fn complete_symbol(
        &self,
        root: AnySymbolId,
        level: SymbolCompletionLevel,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, SymbolCompletionError<FactQueryError>> {
        let binding_context = self
            .binding_context(cancellation)
            .map_err(SymbolCompletionError::Evaluator)?;

        crate::fact::complete_symbol(
            binding_context.symbols,
            root,
            level,
            &self.state.fact_runtime,
            cancellation,
            &binding_context,
        )
    }
}

fn evaluate_type_member_value(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError> {
    match symbol {
        AnySymbolId::InherentTypeMember(owner) => {
            evaluate_typed::<InherentTypeMemberValueQuery>(binding_context, owner)
        }
        AnySymbolId::TraitTypeFulfillment(owner) => {
            evaluate_typed::<TraitTypeFulfillmentValueQuery>(binding_context, owner)
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn evaluate_exact<C, O>(
    binding_context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<DiagnosticBag, FactQueryError>
where
    C: SymbolQueryContract<Owner = O>,
    O: ExactSymbolId,
    for<'binding_context> CompilationBindingContext<'binding_context>:
        bray_binder::SymbolQueryErrorProvider<UpstreamError = FactQueryError>
            + SymbolQueryProvider<C>,
{
    let owner = O::try_from_any(symbol).ok_or(FactQueryError::InfrastructureFailure)?;

    evaluate_typed::<C>(binding_context, owner)
}

fn evaluate_typed<C>(
    binding_context: &CompilationBindingContext<'_>,
    owner: C::Owner,
) -> Result<DiagnosticBag, FactQueryError>
where
    C: SymbolQueryContract,
    for<'binding_context> CompilationBindingContext<'binding_context>:
        bray_binder::SymbolQueryErrorProvider<UpstreamError = FactQueryError>
            + SymbolQueryProvider<C>,
{
    let result = binding_context
        .resolve_symbol_query(SymbolQueryRequest::<C>::new(owner))
        .map_err(binding_query_error)?;

    // Completion aggregates independently owned query diagnostics after all work succeeds.
    Ok(result.diagnostics().clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        CallableContractSymbolId, CallableContractTemplate, CallableContractTemplateQuery,
        CallableContractTypeQuery, CallableContractsQuery, CallableExecution,
        CallableSignatureQuery, ExactSymbolId, FunctionSymbolId, GenericConstraintsQuery,
        GenericDeclarationTemplateQuery, GenericOwnerId, ImplementationCoherenceQuery,
        ImplementationSymbolId, NamedTraitImplementationSymbolId, StructFieldSymbolId,
        StructFieldTypeQuery, StructSymbolId, SymbolOrdinal, SymbolQueryRequest,
        TraitCallableMemberSymbolId, TypeCallableMemberSymbolId, TypeData,
        UnionPayloadFieldSymbolId, UnionPayloadFieldTypeQuery,
    };

    use super::{CancellationToken, SymbolCompletionLevel};
    use crate::compilation::binder::symbol::test_support::{
        resolved_query, symbol_graph, type_data,
    };
    use crate::test_support::compilation;

    #[test]
    fn compiler_known_completion_binds_current_declaration_surfaces() {
        let compilation = compilation("module app;");
        let symbols = symbol_graph(&compilation);
        let root = symbols.compiler_known_environment().id().into();
        let cancellation = CancellationToken::new();

        let diagnostics = match compilation.complete_symbol(
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
        let copy_request = SymbolQueryRequest::<CallableSignatureQuery>::new(copy.into());
        let first_copy = resolved_query(&binding_context, copy_request);
        let second_copy = resolved_query(&binding_context, copy_request);

        assert!(Arc::ptr_eq(&first_copy, &second_copy));
        assert_eq!(first_copy.value().parameters().len(), 3);

        let copy_contract_template = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableContractTemplateQuery>::new(copy.into()),
        );

        assert!(matches!(
            copy_contract_template.value(),
            CallableContractTemplate::Source(contract)
                if contract.expressions().len() == 5 && contract.capabilities().len() == 2
        ));

        let storage_destroy = declaration::<TraitCallableMemberSymbolId>(symbols, "StorageDestroy");

        let storage_destroy_contract = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableContractsQuery>::new(storage_destroy.into()),
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

        let pointer_constraints = resolved_query(
            &binding_context,
            SymbolQueryRequest::<GenericConstraintsQuery>::new(pointer_owner),
        );

        assert_eq!(pointer_constraints.value().constraints().len(), 1);

        let pointer_template = resolved_query(
            &binding_context,
            SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(pointer_owner),
        );

        assert_eq!(pointer_template.value().parameters().len(), 1);
        assert_eq!(pointer_template.value().constraints().len(), 1);

        let storage_implementation =
            declaration::<NamedTraitImplementationSymbolId>(symbols, "HeapStorageImplementation");

        let coherence = resolved_query(
            &binding_context,
            SymbolQueryRequest::<ImplementationCoherenceQuery>::new(ImplementationSymbolId::from(
                storage_implementation,
            )),
        );

        assert!(coherence.value().trait_application().is_some());

        let unary = declaration::<CallableContractSymbolId>(symbols, "UnaryCallable");

        let unary_type = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableContractTypeQuery>::new(unary),
        );

        assert!(matches!(
            type_data(&compilation, unary_type.value()).as_ref(),
            TypeData::Callable(_)
        ));

        let element = declaration::<StructFieldSymbolId>(symbols, "RawPointerElement");

        let element_type = resolved_query(
            &binding_context,
            SymbolQueryRequest::<StructFieldTypeQuery>::new(element),
        );

        assert!(matches!(
            type_data(&compilation, element_type.value()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        let completed_value =
            declaration::<UnionPayloadFieldSymbolId>(symbols, "RunResultVariant0CompletedValue");

        let completed_value_type = resolved_query(
            &binding_context,
            SymbolQueryRequest::<UnionPayloadFieldTypeQuery>::new(completed_value),
        );

        assert!(matches!(
            type_data(&compilation, completed_value_type.value()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        let start = declaration::<TypeCallableMemberSymbolId>(symbols, "FutureStart");

        let start_signature = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableSignatureQuery>::new(start.into()),
        );

        assert!(start_signature.value().parameters().is_empty());
        assert!(start_signature.value().receiver().is_some());

        assert!(matches!(
            type_data(&compilation, start_signature.value().result()).as_ref(),
            TypeData::Named { definition, .. }
                if *definition == declaration::<StructSymbolId>(symbols, "Task").into()
        ));

        let join = declaration::<TypeCallableMemberSymbolId>(symbols, "TaskJoin");

        let join_signature = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableSignatureQuery>::new(join.into()),
        );

        assert!(matches!(
            type_data(&compilation, join_signature.value().callable_type()).as_ref(),
            TypeData::Callable(callable)
                if callable.execution() == CallableExecution::Asynchronous
        ));

        let join_contracts = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableContractsQuery>::new(join.into()),
        );

        assert!(
            join_contracts
                .value()
                .deferred_execution_behavior()
                .is_some()
        );

        let borrow = declaration::<TraitCallableMemberSymbolId>(symbols, "StorageBorrow");

        let borrow_signature = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableSignatureQuery>::new(borrow.into()),
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
