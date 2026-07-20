use bray_binder::{
    BinderFactError, BinderFactResult, TypeExpressionBinder, TypeExpressionScope,
    TypeParameterBinding,
};
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
    let type_parameters = type_parameter_bindings(context, symbol)?;

    let module = context
        .symbols
        .containing_module(symbol)
        .map(|module| module.id());

    let self_type = self_type_context(context.symbols, symbol);

    let scope = TypeExpressionScope::new(symbol, module, type_parameters, self_type);

    Ok(TypeExpressionBinder::new(
        context.symbols,
        context.semantic_values,
        scope,
        context.cancellation,
    ))
}

fn type_parameter_bindings(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<Vec<TypeParameterBinding>> {
    let mut bindings = Vec::new();

    for owner in symbol_ancestry(context.symbols, symbol).into_iter().rev() {
        let Some(parameters) = generic_type_parameters(context.symbols, owner) else {
            continue;
        };

        for parameter in parameters {
            let name = type_parameter_name(context, owner, *parameter)?;

            bindings.push(TypeParameterBinding::new(name, *parameter));
        }
    }

    Ok(bindings)
}

fn symbol_ancestry(symbols: &SymbolGraph, symbol: AnySymbolId) -> Vec<AnySymbolId> {
    let mut ancestry = Vec::new();
    let mut current = Some(symbol);

    while let Some(owner) = current {
        ancestry.push(owner);
        current = symbols.containing_symbol(owner);
    }

    ancestry
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

    if let Some(name) = record.inferred_name() {
        // Binder environments retain their own cheaply shared semantic name handle.
        return Ok(name.clone());
    }

    parameter_name(
        context,
        owner,
        record.origin(),
        record.declaration(),
        record.ordinal(),
        CatalogGenericParameterKind::Type,
    )
}

fn parameter_name(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    origin: SymbolOrigin,
    declaration: Option<bray_declarations::DeclarationId>,
    ordinal: u32,
    kind: CatalogGenericParameterKind,
) -> BinderFactResult<SymbolName> {
    match origin {
        SymbolOrigin::Source => {
            let declaration = declaration
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

            let ordinal =
                usize::try_from(ordinal).map_err(|_| BinderFactError::DependencyUnavailable)?;

            let signature = fact.surface().signature();

            let parameter = signature
                .generic_parameters()
                .get(ordinal)
                .filter(|parameter| parameter.kind() == kind)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            SymbolName::try_new(parameter.name()).ok_or(BinderFactError::DependencyUnavailable)
        }
        SymbolOrigin::Imported | SymbolOrigin::Synthesized => {
            Err(BinderFactError::DependencyUnavailable)
        }
    }
}

macro_rules! define_generic_parameter_accessor {
    ($name:ident, $parameter:ty, $accessor:ident) => {
        fn $name(symbols: &SymbolGraph, owner: AnySymbolId) -> Option<&[$parameter]> {
            match owner {
                AnySymbolId::Struct(id) => symbols.structure(id).map(|symbol| symbol.$accessor()),
                AnySymbolId::Union(id) => symbols.union(id).map(|symbol| symbol.$accessor()),
                AnySymbolId::Trait(id) => symbols.trait_symbol(id).map(|symbol| symbol.$accessor()),
                AnySymbolId::CallableContract(id) => symbols
                    .callable_contract(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::Function(id) => symbols.function(id).map(|symbol| symbol.$accessor()),
                AnySymbolId::Predicate(id) => {
                    symbols.predicate(id).map(|symbol| symbol.$accessor())
                }
                AnySymbolId::TypeCallableMember(id) => symbols
                    .type_callable_member(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitCallableMember(id) => symbols
                    .trait_callable_member(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitCallableFulfillment(id) => symbols
                    .trait_callable_fulfillment(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitPredicateMember(id) => symbols
                    .trait_predicate_member(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitPredicateFulfillment(id) => symbols
                    .trait_predicate_fulfillment(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::Constructor(id) => {
                    symbols.constructor(id).map(|symbol| symbol.$accessor())
                }
                AnySymbolId::Finalizer(id) => {
                    symbols.finalizer(id).map(|symbol| symbol.$accessor())
                }
                AnySymbolId::Destructor(id) => {
                    symbols.destructor(id).map(|symbol| symbol.$accessor())
                }
                AnySymbolId::ScopeEnter(id) => {
                    symbols.scope_enter(id).map(|symbol| symbol.$accessor())
                }
                AnySymbolId::ScopeExit(id) => {
                    symbols.scope_exit(id).map(|symbol| symbol.$accessor())
                }
                AnySymbolId::TraitFinalizerRequirement(id) => symbols
                    .trait_finalizer_requirement(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitDestructorRequirement(id) => symbols
                    .trait_destructor_requirement(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitScopeEnterRequirement(id) => symbols
                    .trait_scope_enter_requirement(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitScopeExitRequirement(id) => symbols
                    .trait_scope_exit_requirement(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitScopeEnterFulfillment(id) => symbols
                    .trait_scope_enter_fulfillment(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::TraitScopeExitFulfillment(id) => symbols
                    .trait_scope_exit_fulfillment(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::InherentImplementation(id) => symbols
                    .inherent_implementation(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::UnnamedTraitImplementation(id) => symbols
                    .unnamed_trait_implementation(id)
                    .map(|symbol| symbol.$accessor()),
                AnySymbolId::NamedTraitImplementation(id) => symbols
                    .named_trait_implementation(id)
                    .map(|symbol| symbol.$accessor()),
                _ => None,
            }
        }
    };
}

define_generic_parameter_accessor!(
    generic_type_parameters,
    GenericTypeParameterSymbolId,
    generic_type_parameters
);

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
