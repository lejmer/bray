use bray_binder::{
    BinderFactError, BinderFactResult, CallableTypeQualifiers, TypeExpressionBinder,
    TypeParameterBinding,
};
use bray_compiler_known::{CatalogGenericParameterKind, CatalogSurfaceElement};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractSet, CallableContractTypeFact, CallableContractsFact,
    CallableParameterSymbolId, CallableSignatureFact, CallableSymbolId,
    DependencyContractTemplateData, GenericConstraintSet, GenericConstraintsFact,
    GenericTypeParameterSymbolId, ImplementationSubject, ImplementationSubjectFact,
    ImplementationSymbolId, ImplementedTraitApplicationFact, InherentTypeMemberValueFact,
    ReceiverParameterSymbolId, SelfTypeContext, StructFieldTypeFact, SymbolFactContract,
    SymbolFactRequest, SymbolFactResult, SymbolGraph, SymbolName, TraitTypeFulfillmentValueFact,
};
use bray_syntax::{
    CallableContractDeclarationSyntax, FunctionDeclarationSyntax,
    ImplementationTypeMemberBindingSyntax, NamedTraitImplementationDeclarationSyntax,
    StructFieldDeclarationSyntax, SyntaxKind, TraitCallableMemberDeclarationSyntax,
};

use super::super::context::CompilationBinderFacts;
use super::cache::CompilationSymbolFacts;
use crate::fact::SymbolFactCache;

pub(super) trait CompilationSymbolFactBinding<C>
where
    C: SymbolFactContract,
{
    fn cache(&self) -> &SymbolFactCache<C>;

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<C>,
    ) -> BinderFactResult<SymbolFactResult<C>>;
}

impl CompilationSymbolFactBinding<GenericConstraintsFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<GenericConstraintsFact> {
        &self.generic_constraints
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<GenericConstraintsFact>,
    ) -> BinderFactResult<SymbolFactResult<GenericConstraintsFact>> {
        let surface = compiler_known_surface(context.symbols, request.symbol())?;

        if surface.elements().iter().any(|element| {
            matches!(
                element,
                CatalogSurfaceElement::EnterNode(SyntaxKind::WithClause)
            )
        }) {
            return Err(BinderFactError::DependencyUnavailable);
        }

        Ok(DiagnosticResult::without_diagnostics(
            GenericConstraintSet::new([]),
        ))
    }
}

impl CompilationSymbolFactBinding<CallableSignatureFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<CallableSignatureFact> {
        &self.callable_signatures
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<CallableSignatureFact>,
    ) -> BinderFactResult<SymbolFactResult<CallableSignatureFact>> {
        bind_callable_signature(context, request.owner())
    }
}

impl CompilationSymbolFactBinding<CallableContractsFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<CallableContractsFact> {
        &self.callable_contracts
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<CallableContractsFact>,
    ) -> BinderFactResult<SymbolFactResult<CallableContractsFact>> {
        let surface = compiler_known_surface(context.symbols, request.symbol())?;
        let has_contracts = surface.elements().iter().any(|element| {
            matches!(
                element,
                CatalogSurfaceElement::EnterNode(
                    SyntaxKind::RequiresClause | SyntaxKind::EnsuresClause | SyntaxKind::UsesClause
                )
            )
        });

        if has_contracts {
            return Err(BinderFactError::DependencyUnavailable);
        }

        let dependency = context
            .semantic_values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

        Ok(DiagnosticResult::without_diagnostics(
            CallableContractSet::new([], [], dependency),
        ))
    }
}

impl CompilationSymbolFactBinding<CallableContractTypeFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<CallableContractTypeFact> {
        &self.callable_contract_types
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<CallableContractTypeFact>,
    ) -> BinderFactResult<SymbolFactResult<CallableContractTypeFact>> {
        let symbol = AnySymbolId::from(request.owner());
        let syntax =
            declaration_syntax::<CallableContractDeclarationSyntax>(context.symbols, symbol)?;

        type_binder(context, symbol)?.bind_type_expression(&syntax.type_expression())
    }
}

impl CompilationSymbolFactBinding<StructFieldTypeFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<StructFieldTypeFact> {
        &self.struct_field_types
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<StructFieldTypeFact>,
    ) -> BinderFactResult<SymbolFactResult<StructFieldTypeFact>> {
        let symbol = AnySymbolId::from(request.owner());
        let syntax = declaration_syntax::<StructFieldDeclarationSyntax>(context.symbols, symbol)?;

        type_binder(context, symbol)?.bind_type_expression(&syntax.type_expression())
    }
}

impl CompilationSymbolFactBinding<InherentTypeMemberValueFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<InherentTypeMemberValueFact> {
        &self.inherent_type_member_values
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<InherentTypeMemberValueFact>,
    ) -> BinderFactResult<SymbolFactResult<InherentTypeMemberValueFact>> {
        bind_type_member_value::<InherentTypeMemberValueFact>(context, request.symbol())
    }
}

impl CompilationSymbolFactBinding<TraitTypeFulfillmentValueFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<TraitTypeFulfillmentValueFact> {
        &self.trait_type_fulfillment_values
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<TraitTypeFulfillmentValueFact>,
    ) -> BinderFactResult<SymbolFactResult<TraitTypeFulfillmentValueFact>> {
        bind_type_member_value::<TraitTypeFulfillmentValueFact>(context, request.symbol())
    }
}

fn bind_type_member_value<C>(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<SymbolFactResult<C>>
where
    C: SymbolFactContract<Value = bray_symbols::TypeId>,
{
    let syntax =
        declaration_syntax::<ImplementationTypeMemberBindingSyntax>(context.symbols, symbol)?;

    type_binder(context, symbol)?.bind_type_expression(&syntax.type_expression())
}

impl CompilationSymbolFactBinding<ImplementationSubjectFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ImplementationSubjectFact> {
        &self.implementation_subjects
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ImplementationSubjectFact>,
    ) -> BinderFactResult<SymbolFactResult<ImplementationSubjectFact>> {
        let symbol = request.symbol();
        let syntax = declaration_syntax::<NamedTraitImplementationDeclarationSyntax>(
            context.symbols,
            symbol,
        )?;
        let result = type_binder(context, symbol)?
            .bind_implementation_subject(&syntax.implementation_subject())?;

        Ok(result.map(ImplementationSubject::new))
    }
}

impl CompilationSymbolFactBinding<ImplementedTraitApplicationFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ImplementedTraitApplicationFact> {
        &self.implemented_traits
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ImplementedTraitApplicationFact>,
    ) -> BinderFactResult<SymbolFactResult<ImplementedTraitApplicationFact>> {
        let symbol = request.symbol();

        if matches!(request.owner(), ImplementationSymbolId::Inherent(_)) {
            compiler_known_surface(context.symbols, symbol)?;

            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let syntax = declaration_syntax::<NamedTraitImplementationDeclarationSyntax>(
            context.symbols,
            symbol,
        )?;
        let result =
            type_binder(context, symbol)?.bind_trait_application(&syntax.trait_application())?;

        Ok(result.map(Some))
    }
}

fn bind_callable_signature(
    context: &CompilationBinderFacts<'_>,
    callable: CallableSymbolId,
) -> BinderFactResult<SymbolFactResult<CallableSignatureFact>> {
    let symbol = callable.into_any();
    let surface = compiler_known_surface(context.symbols, symbol)?;

    let fragment = surface
        .syntax_fragment()
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let (parameters, receiver) = callable_relationships(context.symbols, callable)?;

    match callable {
        CallableSymbolId::Function(_) => {
            let syntax = fragment
                .cast::<FunctionDeclarationSyntax>()
                .ok_or(BinderFactError::DependencyUnavailable)?;

            let modifiers = syntax.function_modifiers();
            let qualifiers = callable_qualifiers(CallableModifierPresence {
                is_constant: modifiers.const_token().is_some(),
                is_async: modifiers.async_token().is_some(),
                is_trusted: modifiers.trusted_token().is_some(),
            });

            let result = syntax.callable_result_clause();

            type_binder(context, symbol)?.bind_callable_signature(
                callable,
                parameters,
                receiver,
                &syntax.parameter_list(),
                result
                    .as_ref()
                    .map(|clause| clause.type_expression())
                    .as_ref(),
                qualifiers,
            )
        }
        CallableSymbolId::TraitMember(_) => {
            let syntax = fragment
                .cast::<TraitCallableMemberDeclarationSyntax>()
                .ok_or(BinderFactError::DependencyUnavailable)?;

            let modifiers = syntax.trait_callable_member_modifiers();
            let qualifiers = callable_qualifiers(CallableModifierPresence {
                is_constant: modifiers.const_token().is_some(),
                is_async: modifiers.async_token().is_some(),
                is_trusted: modifiers.trusted_token().is_some(),
            });

            let result = syntax.callable_result_clause();

            type_binder(context, symbol)?.bind_callable_signature(
                callable,
                parameters,
                receiver,
                &syntax.parameter_list(),
                result
                    .as_ref()
                    .map(|clause| clause.type_expression())
                    .as_ref(),
                qualifiers,
            )
        }
        _ => Err(BinderFactError::DependencyUnavailable),
    }
}

#[derive(Clone, Copy)]
struct CallableModifierPresence {
    is_constant: bool,
    is_async: bool,
    is_trusted: bool,
}

fn callable_qualifiers(modifiers: CallableModifierPresence) -> CallableTypeQualifiers {
    CallableTypeQualifiers::new(
        if modifiers.is_constant {
            bray_symbols::CallableConstness::Constant
        } else {
            bray_symbols::CallableConstness::Runtime
        },
        if modifiers.is_async {
            bray_symbols::CallableExecution::Asynchronous
        } else {
            bray_symbols::CallableExecution::Synchronous
        },
        if modifiers.is_trusted {
            bray_symbols::CallableTrust::Trusted
        } else {
            bray_symbols::CallableTrust::Safe
        },
    )
}

fn callable_relationships(
    symbols: &SymbolGraph,
    callable: CallableSymbolId,
) -> BinderFactResult<(
    &[CallableParameterSymbolId],
    Option<ReceiverParameterSymbolId>,
)> {
    match callable {
        CallableSymbolId::Function(id) => symbols
            .function(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitMember(id) => symbols
            .trait_callable_member(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        _ => None,
    }
    .ok_or(BinderFactError::DependencyUnavailable)
}

fn type_binder<'facts>(
    context: &'facts CompilationBinderFacts<'facts>,
    symbol: AnySymbolId,
) -> BinderFactResult<TypeExpressionBinder<'facts>> {
    let parameters = type_parameter_bindings(context.symbols, symbol)?;

    let module = context
        .symbols
        .containing_module(symbol)
        .map(|module| module.id());

    let self_type = self_type_context(context.symbols, symbol);

    Ok(TypeExpressionBinder::new(
        context.symbols,
        context.semantic_values,
        module,
        parameters,
        self_type,
        context.cancellation,
    ))
}

fn type_parameter_bindings(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> BinderFactResult<Vec<TypeParameterBinding>> {
    let mut ancestry = Vec::new();
    let mut current = Some(symbol);

    while let Some(owner) = current {
        ancestry.push(owner);
        current = symbols.containing_symbol(owner);
    }

    let mut bindings = Vec::new();

    for owner in ancestry.into_iter().rev() {
        let Some(parameters) = generic_type_parameters(symbols, owner) else {
            continue;
        };

        let Some(fact) = symbols
            .compiler_known_provider()
            .declaration_fact_for_symbol(owner)
        else {
            continue;
        };

        let signature = fact.surface().signature();

        for (ordinal, parameter) in signature.generic_parameters().iter().enumerate() {
            if parameter.kind() != CatalogGenericParameterKind::Type {
                continue;
            }

            let ordinal =
                u32::try_from(ordinal).map_err(|_| BinderFactError::DependencyUnavailable)?;

            let Some(symbol) = parameters.iter().copied().find(|candidate| {
                symbols
                    .generic_type_parameter(*candidate)
                    .is_some_and(|record| record.ordinal() == ordinal)
            }) else {
                return Err(BinderFactError::DependencyUnavailable);
            };

            let Some(name) = SymbolName::try_new(parameter.name()) else {
                return Err(BinderFactError::DependencyUnavailable);
            };

            bindings.push(TypeParameterBinding::new(name, symbol));
        }
    }

    Ok(bindings)
}

fn generic_type_parameters(
    symbols: &SymbolGraph,
    owner: AnySymbolId,
) -> Option<&[GenericTypeParameterSymbolId]> {
    match owner {
        AnySymbolId::Struct(id) => symbols
            .structure(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::Union(id) => symbols
            .union(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::Trait(id) => symbols
            .trait_symbol(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::CallableContract(id) => symbols
            .callable_contract(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::Function(id) => symbols
            .function(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitCallableMember(id) => symbols
            .trait_callable_member(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::NamedTraitImplementation(id) => symbols
            .named_trait_implementation(id)
            .map(|symbol| symbol.generic_type_parameters()),
        _ => None,
    }
}

fn self_type_context(symbols: &SymbolGraph, symbol: AnySymbolId) -> Option<SelfTypeContext> {
    let mut current = symbols.containing_symbol(symbol);

    while let Some(owner) = current {
        if let Some(context) = SelfTypeContext::try_new(owner) {
            return Some(context);
        }

        current = symbols.containing_symbol(owner);
    }

    None
}

fn compiler_known_surface(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> BinderFactResult<&'static bray_compiler_known::CatalogDeclarationSurfaceSyntax> {
    symbols
        .compiler_known_provider()
        .declaration_fact_for_symbol(symbol)
        .map(|fact| fact.surface())
        .ok_or(BinderFactError::DependencyUnavailable)
}

fn declaration_syntax<T>(symbols: &SymbolGraph, symbol: AnySymbolId) -> BinderFactResult<T>
where
    T: bray_syntax::SyntaxCast,
{
    let surface = compiler_known_surface(symbols, symbol)?;

    let fragment = surface
        .syntax_fragment()
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    fragment
        .cast::<T>()
        .ok_or(BinderFactError::DependencyUnavailable)
}
