use crate::compilation::binder::BindingQueryResult;
use bray_binder::{TypeExpressionBinder, TypeExpressionScope, TypeParameterBinding};
use bray_compiler_known::CatalogGenericParameterKind;
use bray_symbols::{
    AnySymbolId, GenericConstParameterSymbolId, GenericParameterSymbolId,
    GenericTypeParameterSymbolId, ImportedSymbolSkeleton, SymbolGraph, SymbolName, SymbolOrigin,
};

use super::super::context::CompilationBindingContext;
use crate::compilation::binder::semantic_contract_binding_error as binding_contract;
use crate::compilation::{SemanticDataKind, SemanticQueryContext, SemanticQueryViolation};

pub(in crate::compilation) fn type_binder<'binding>(
    context: &'binding CompilationBindingContext<'binding>,
    symbol: AnySymbolId,
) -> BindingQueryResult<TypeExpressionBinder<'binding, crate::fact::FactQueryError>> {
    let scope = type_scope(context, symbol)?;

    Ok(TypeExpressionBinder::new(
        context.symbols,
        context,
        context.semantic_values,
        scope,
        context.cancellation,
    ))
}

pub(in crate::compilation) fn type_scope(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> BindingQueryResult<TypeExpressionScope> {
    let type_parameters = type_parameter_bindings(context, symbol)?;

    let module = context
        .symbols
        .containing_module(symbol)
        .map(|module| module.id());

    let self_type = context.symbols.contextual_self_scope(symbol);

    Ok(TypeExpressionScope::new(
        symbol,
        module,
        type_parameters,
        self_type,
    ))
}

fn type_parameter_bindings(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> BindingQueryResult<Vec<TypeParameterBinding>> {
    let mut bindings = Vec::new();

    for owner in symbol_ancestry(context.symbols, symbol).into_iter().rev() {
        let Some(parameters) =
            GenericParameterAccess::generic_type_parameters(context.symbols, owner)
        else {
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

pub(in crate::compilation) fn visible_generic_const_parameters(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> Vec<GenericConstParameterSymbolId> {
    visible_generic_parameters(symbols, symbol)
        .into_iter()
        .filter_map(|parameter| match parameter {
            GenericParameterSymbolId::Const(parameter) => Some(parameter),
            GenericParameterSymbolId::Type(_) => None,
        })
        .collect()
}

pub(in crate::compilation) fn visible_generic_parameters(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> Vec<GenericParameterSymbolId> {
    symbol_ancestry(symbols, symbol)
        .into_iter()
        .rev()
        .filter_map(|owner| generic_parameter_ids(symbols, owner).ok())
        .flatten()
        .collect()
}

pub(in crate::compilation) fn has_visible_generic_parameters(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> bool {
    symbol_ancestry(symbols, symbol).into_iter().any(|owner| {
        GenericParameterAccess::generic_type_parameters(symbols, owner)
            .is_some_and(|parameters| !parameters.is_empty())
            || GenericParameterAccess::generic_const_parameters(symbols, owner)
                .is_some_and(|parameters| !parameters.is_empty())
    })
}

fn type_parameter_name(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    parameter: GenericTypeParameterSymbolId,
) -> BindingQueryResult<SymbolName> {
    let record = context
        .symbols
        .generic_type_parameter(parameter)
        .ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(parameter.into()),
                SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
            )
        })?;

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
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    origin: SymbolOrigin,
    declaration: Option<bray_declarations::DeclarationId>,
    ordinal: u32,
    kind: CatalogGenericParameterKind,
) -> BindingQueryResult<SymbolName> {
    match origin {
        SymbolOrigin::Source => {
            let declaration = declaration.ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
                )
            })?;

            let declaration_record =
                context
                    .declarations()
                    .declaration(declaration)
                    .ok_or_else(|| {
                        binding_contract(
                            SemanticQueryContext::Declaration(declaration),
                            SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
                        )
                    })?;

            declaration_record
                .name()
                .and_then(bray_declarations::DeclarationName::as_identifier)
                .and_then(SymbolName::try_new)
                .ok_or_else(|| {
                    binding_contract(
                        SemanticQueryContext::Declaration(declaration),
                        SemanticQueryViolation::Unsupported(SemanticDataKind::MemberName),
                    )
                })
        }
        SymbolOrigin::CompilerKnown | SymbolOrigin::CompilerProvided => {
            let semantics = context
                .symbols
                .compiler_known_provider()
                .declaration_semantics_for_symbol(owner)
                .ok_or_else(|| {
                    binding_contract(
                        SemanticQueryContext::Symbol(owner),
                        SemanticQueryViolation::Missing(SemanticDataKind::DeclarationRecord),
                    )
                })?;

            let ordinal = usize::try_from(ordinal).map_err(|_| {
                crate::compilation::binder::semantic_contract_binding_error(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::CountOverflow {
                        data: SemanticDataKind::GenericSubstitution,
                        value: u64::from(ordinal),
                    },
                )
            })?;

            let signature = semantics.surface().signature();

            let parameters = signature.generic_parameters();

            let parameter = parameters.get(ordinal).ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::CountMismatch {
                        data: SemanticDataKind::GenericSubstitution,
                        expected: ordinal.saturating_add(1),
                        actual: parameters.len(),
                    },
                )
            })?;

            if parameter.kind() != kind {
                return Err(binding_contract(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::GenericParameterKindMismatch {
                        expected: kind,
                        actual: parameter.kind(),
                    },
                ));
            }

            SymbolName::try_new(parameter.name()).ok_or_else(|| {
                binding_contract(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::Unsupported(SemanticDataKind::MemberName),
                )
            })
        }
        origin @ (SymbolOrigin::Imported | SymbolOrigin::Synthesized) => Err(binding_contract(
            SemanticQueryContext::Symbol(owner),
            SemanticQueryViolation::UnexpectedSymbolOrigin(origin),
        )),
    }
}

pub(in crate::compilation) trait GenericParameterAccess {
    fn generic_type_parameters(
        &self,
        owner: AnySymbolId,
    ) -> Option<&[GenericTypeParameterSymbolId]>;

    fn generic_const_parameters(
        &self,
        owner: AnySymbolId,
    ) -> Option<&[GenericConstParameterSymbolId]>;

    fn generic_parameter_ordinal(&self, parameter: GenericParameterSymbolId) -> Option<u32>;
}

macro_rules! generic_parameters {
    ($symbols:expr, $owner:expr, $accessor:ident) => {
        match $owner {
            AnySymbolId::Static(id) => $symbols.static_symbol(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::Struct(id) => $symbols.structure(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::Union(id) => $symbols.union(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::Trait(id) => $symbols.trait_symbol(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::CallableContract(id) => $symbols
                .callable_contract(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::Function(id) => $symbols.function(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::Predicate(id) => $symbols.predicate(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::TypeCallableMember(id) => $symbols
                .type_callable_member(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitCallableMember(id) => $symbols
                .trait_callable_member(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitCallableFulfillment(id) => $symbols
                .trait_callable_fulfillment(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitPredicateMember(id) => $symbols
                .trait_predicate_member(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitPredicateFulfillment(id) => $symbols
                .trait_predicate_fulfillment(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::Constructor(id) => {
                $symbols.constructor(id).map(|symbol| symbol.$accessor())
            }
            AnySymbolId::Finalizer(id) => $symbols.finalizer(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::Destructor(id) => $symbols.destructor(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::ScopeEnter(id) => {
                $symbols.scope_enter(id).map(|symbol| symbol.$accessor())
            }
            AnySymbolId::ScopeExit(id) => $symbols.scope_exit(id).map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitFinalizerRequirement(id) => $symbols
                .trait_finalizer_requirement(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitDestructorRequirement(id) => $symbols
                .trait_destructor_requirement(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitScopeEnterRequirement(id) => $symbols
                .trait_scope_enter_requirement(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitScopeExitRequirement(id) => $symbols
                .trait_scope_exit_requirement(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitScopeEnterFulfillment(id) => $symbols
                .trait_scope_enter_fulfillment(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::TraitScopeExitFulfillment(id) => $symbols
                .trait_scope_exit_fulfillment(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::InherentImplementation(id) => $symbols
                .inherent_implementation(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::UnnamedTraitImplementation(id) => $symbols
                .unnamed_trait_implementation(id)
                .map(|symbol| symbol.$accessor()),
            AnySymbolId::NamedTraitImplementation(id) => $symbols
                .named_trait_implementation(id)
                .map(|symbol| symbol.$accessor()),
            _ => None,
        }
    };
}

macro_rules! impl_generic_parameter_access {
    ($provider:ty) => {
        impl GenericParameterAccess for $provider {
            fn generic_type_parameters(
                &self,
                owner: AnySymbolId,
            ) -> Option<&[GenericTypeParameterSymbolId]> {
                generic_parameters!(self, owner, generic_type_parameters)
            }

            fn generic_const_parameters(
                &self,
                owner: AnySymbolId,
            ) -> Option<&[GenericConstParameterSymbolId]> {
                generic_parameters!(self, owner, generic_const_parameters)
            }

            fn generic_parameter_ordinal(
                &self,
                parameter: GenericParameterSymbolId,
            ) -> Option<u32> {
                match parameter {
                    GenericParameterSymbolId::Type(parameter) => self
                        .generic_type_parameter(parameter)
                        .map(|record| record.ordinal()),
                    GenericParameterSymbolId::Const(parameter) => self
                        .generic_const_parameter(parameter)
                        .map(|record| record.ordinal()),
                }
            }
        }
    };
}

impl_generic_parameter_access!(SymbolGraph);
impl_generic_parameter_access!(ImportedSymbolSkeleton);

pub(in crate::compilation) fn declaration_generic_parameter_ids(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<Vec<GenericParameterSymbolId>> {
    if context.symbols.symbol_key(owner).is_some() {
        return generic_parameter_ids(context.symbols, owner);
    }

    let imported = context.imported_symbols()?.ok_or_else(|| {
        binding_contract(
            SemanticQueryContext::Symbol(owner),
            SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
        )
    })?;

    generic_parameter_ids(imported, owner)
}

pub(in crate::compilation) fn generic_parameter_ids(
    symbols: &impl GenericParameterAccess,
    owner: AnySymbolId,
) -> BindingQueryResult<Vec<GenericParameterSymbolId>> {
    let mut parameters = Vec::new();

    if let Some(type_parameters) = symbols.generic_type_parameters(owner) {
        parameters.extend(
            type_parameters
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Type),
        );
    }

    if let Some(const_parameters) = symbols.generic_const_parameters(owner) {
        parameters.extend(
            const_parameters
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Const),
        );
    }

    parameters.sort_by_key(|parameter| symbols.generic_parameter_ordinal(*parameter));

    if let Some(parameter) = parameters
        .iter()
        .copied()
        .find(|parameter| symbols.generic_parameter_ordinal(*parameter).is_none())
    {
        let parameter = match parameter {
            GenericParameterSymbolId::Type(parameter) => AnySymbolId::from(parameter),
            GenericParameterSymbolId::Const(parameter) => AnySymbolId::from(parameter),
        };

        return Err(binding_contract(
            SemanticQueryContext::Symbol(parameter),
            SemanticQueryViolation::Missing(SemanticDataKind::GenericSubstitution),
        ));
    }

    Ok(parameters)
}
