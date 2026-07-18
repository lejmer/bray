use bray_binder::{BinderFactError, BinderFactResult, TypeExpressionBinder, TypeParameterBinding};
use bray_compiler_known::CatalogGenericParameterKind;
use bray_symbols::{
    AnySymbolId, GenericTypeParameterSymbolId, SelfTypeContext, SymbolGraph, SymbolName,
    SymbolOrigin,
};

use super::super::context::CompilationBinderFacts;

pub(super) fn type_binder<'facts>(
    context: &'facts CompilationBinderFacts<'facts>,
    symbol: AnySymbolId,
) -> BinderFactResult<TypeExpressionBinder<'facts>> {
    let parameters = type_parameter_bindings(context, symbol)?;

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
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<Vec<TypeParameterBinding>> {
    let symbols = context.symbols;
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

        for parameter in parameters {
            let name = type_parameter_name(context, owner, *parameter)?;

            bindings.push(TypeParameterBinding::new(name, *parameter));
        }
    }

    Ok(bindings)
}

fn type_parameter_name(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    parameter: GenericTypeParameterSymbolId,
) -> BinderFactResult<SymbolName> {
    let record = context
        .symbols
        .generic_type_parameter(parameter)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    match record.origin() {
        SymbolOrigin::Source => {
            let declaration = record
                .declaration()
                .and_then(|id| context.compilation.declaration_table().declaration(id))
                .ok_or(BinderFactError::DependencyUnavailable)?;

            declaration
                .name()
                .and_then(bray_declarations::DeclarationName::as_identifier)
                .and_then(SymbolName::try_new)
                .ok_or(BinderFactError::DependencyUnavailable)
        }
        SymbolOrigin::CompilerKnown | SymbolOrigin::CompilerProvided => {
            let fact = context
                .symbols
                .compiler_known_provider()
                .declaration_fact_for_symbol(owner)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            let ordinal = usize::try_from(record.ordinal())
                .map_err(|_| BinderFactError::DependencyUnavailable)?;

            let signature = fact.surface().signature();

            let parameter = signature
                .generic_parameters()
                .get(ordinal)
                .filter(|parameter| parameter.kind() == CatalogGenericParameterKind::Type)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            SymbolName::try_new(parameter.name()).ok_or(BinderFactError::DependencyUnavailable)
        }
        SymbolOrigin::Imported | SymbolOrigin::Synthesized => {
            Err(BinderFactError::DependencyUnavailable)
        }
    }
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
        AnySymbolId::Predicate(id) => symbols
            .predicate(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TypeCallableMember(id) => symbols
            .type_callable_member(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitCallableMember(id) => symbols
            .trait_callable_member(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitCallableFulfillment(id) => symbols
            .trait_callable_fulfillment(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitPredicateMember(id) => symbols
            .trait_predicate_member(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitPredicateFulfillment(id) => symbols
            .trait_predicate_fulfillment(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::Constructor(id) => symbols
            .constructor(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::Finalizer(id) => symbols
            .finalizer(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::Destructor(id) => symbols
            .destructor(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::ScopeEnter(id) => symbols
            .scope_enter(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::ScopeExit(id) => symbols
            .scope_exit(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitFinalizerRequirement(id) => symbols
            .trait_finalizer_requirement(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitDestructorRequirement(id) => symbols
            .trait_destructor_requirement(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitScopeEnterRequirement(id) => symbols
            .trait_scope_enter_requirement(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitScopeExitRequirement(id) => symbols
            .trait_scope_exit_requirement(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitScopeEnterFulfillment(id) => symbols
            .trait_scope_enter_fulfillment(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::TraitScopeExitFulfillment(id) => symbols
            .trait_scope_exit_fulfillment(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::InherentImplementation(id) => symbols
            .inherent_implementation(id)
            .map(|symbol| symbol.generic_type_parameters()),
        AnySymbolId::UnnamedTraitImplementation(id) => symbols
            .unnamed_trait_implementation(id)
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
