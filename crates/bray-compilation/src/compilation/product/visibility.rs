use std::collections::BTreeSet;

use bray_binder::SymbolFactProvider;
use bray_declarations::DeclarationTable;
use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_symbols::{
    AnySymbolId, CallableContractTypeFact, CallableSignatureFact, CallableSymbolId,
    ConstantDeclaredTypeFact, GenericArgument, GenericArgumentTemplate,
    GenericConstParameterDeclaredTypeFact, InherentTypeMemberValueFact,
    PredicateSignatureTemplateFact, SemanticValueStore, StructFieldTypeFact, SymbolFactContract,
    SymbolFactRequest, SymbolGraph, SymbolKey, SymbolOrigin,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
    TraitTypeFulfillmentValueFact, TypeData, TypeExpressionTemplate, UnionPayloadFieldTypeFact,
};

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::compilation::diagnostics::source_diagnostic;
use crate::fact::FactQueryError;

pub(super) fn validate_public_surface(
    binder: &CompilationBinderFacts<'_>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    public_symbols: &BTreeSet<AnySymbolId>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let mut is_recovered = false;

    for symbol in public_symbols {
        let exposes_internal = match *symbol {
            AnySymbolId::Constant(owner) => validate_type_fact::<ConstantDeclaredTypeFact>(
                binder,
                owner,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?,
            AnySymbolId::TraitConstantMember(owner) => {
                validate_type_fact::<TraitConstantMemberDeclaredTypeFact>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::TraitConstantFulfillment(owner) => {
                validate_type_fact::<TraitConstantFulfillmentDeclaredTypeFact>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::GenericConstParameter(owner) => {
                validate_type_fact::<GenericConstParameterDeclaredTypeFact>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::StructField(owner) => validate_type_fact::<StructFieldTypeFact>(
                binder,
                owner,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?,
            AnySymbolId::UnionPayloadField(owner) => {
                validate_type_fact::<UnionPayloadFieldTypeFact>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::InherentTypeMember(owner) => {
                validate_type_fact::<InherentTypeMemberValueFact>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::TraitTypeFulfillment(owner) => {
                validate_type_fact::<TraitTypeFulfillmentValueFact>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::CallableContract(owner) => validate_type_fact::<CallableContractTypeFact>(
                binder,
                owner,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?,
            AnySymbolId::Predicate(owner) => validate_predicate_signature(
                binder,
                owner.into(),
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?,
            _ => false,
        };

        let exposes_internal = if let Some(callable) = CallableSymbolId::try_from_any(*symbol) {
            validate_callable_signature(
                binder,
                callable,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )? || exposes_internal
        } else {
            exposes_internal
        };

        if exposes_internal {
            add_internal_dependency_diagnostic(*symbol, symbols, diagnostics);

            is_recovered = true;
        }
    }

    Ok(is_recovered)
}

fn validate_callable_signature(
    binder: &CompilationBinderFacts<'_>,
    callable: CallableSymbolId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let signature = binder
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(signature.diagnostics().iter().cloned());

    Ok(template_exposes_internal(
        signature.value().callable_type(),
        semantic_values,
        symbols,
        declarations,
    ))
}

fn validate_predicate_signature(
    binder: &CompilationBinderFacts<'_>,
    predicate: bray_symbols::PredicateDefinitionSymbolId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let signature = binder
        .symbol_fact(SymbolFactRequest::<PredicateSignatureTemplateFact>::new(
            predicate,
        ))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(signature.diagnostics().iter().cloned());

    Ok(signature.value().parameters().iter().any(|parameter| {
        template_exposes_internal(parameter.ty(), semantic_values, symbols, declarations)
    }))
}

fn validate_type_fact<C>(
    binder: &CompilationBinderFacts<'_>,
    owner: C::Owner,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError>
where
    C: SymbolFactContract<Value = TypeExpressionTemplate>,
    for<'facts> CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
{
    let fact = binder
        .symbol_fact(SymbolFactRequest::<C>::new(owner))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(fact.diagnostics().iter().cloned());

    Ok(template_exposes_internal(
        fact.value(),
        semantic_values,
        symbols,
        declarations,
    ))
}

fn add_internal_dependency_diagnostic(
    symbol: AnySymbolId,
    symbols: &SymbolGraph,
    diagnostics: &mut DiagnosticBag,
) {
    let Some(anchor) = symbols.declaration_syntax_anchor(symbol) else {
        return;
    };

    diagnostics.add(source_diagnostic(
        anchor,
        DiagnosticKind::CheckingExportDependsOnInternalDeclaration,
    ));
}

fn template_exposes_internal(
    template: &TypeExpressionTemplate,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let mut pending = vec![template];

    while let Some(template) = pending.pop() {
        match template {
            TypeExpressionTemplate::Resolved(ty) => {
                if resolved_type_exposes_internal(*ty, semantic_values, symbols, declarations) {
                    return true;
                }
            }
            TypeExpressionTemplate::Named {
                definition,
                arguments,
                ..
            } => {
                if source_symbol_is_internal(definition.into_any(), declarations, symbols) {
                    return true;
                }

                pending.extend(arguments.iter().filter_map(|argument| match argument {
                    GenericArgumentTemplate::Type(ty) => Some(ty),
                    GenericArgumentTemplate::Constant(_) => None,
                }));
            }
            TypeExpressionTemplate::TypeValuedMemberProjection { subject, .. } => {
                pending.push(subject);
            }
            TypeExpressionTemplate::Tuple(elements) => pending.extend(elements.iter()),
            TypeExpressionTemplate::Array { element, .. }
            | TypeExpressionTemplate::Slice(element)
            | TypeExpressionTemplate::Nullable(element) => pending.push(element),
            TypeExpressionTemplate::Borrow { target, .. } => pending.push(target),
            TypeExpressionTemplate::OwnedIndirection { storage, target } => {
                pending.push(storage);
                pending.push(target);
            }
            TypeExpressionTemplate::Callable(callable) => {
                pending.extend(callable.parameters().iter().map(|parameter| parameter.ty()));
                pending.push(callable.result());
            }
            TypeExpressionTemplate::TraitView(_) => {}
        }
    }

    false
}

fn resolved_type_exposes_internal(
    ty: bray_symbols::TypeId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let mut pending = vec![ty];
    let mut visited = BTreeSet::new();

    while let Some(ty) = pending.pop() {
        if !visited.insert(ty) {
            continue;
        }

        let Ok(data) = semantic_values.type_data(ty) else {
            continue;
        };

        match data.as_ref() {
            TypeData::Named {
                definition,
                substitution,
            } => {
                if source_symbol_is_internal(definition.into_any(), declarations, symbols) {
                    return true;
                }

                let Ok(substitution) = semantic_values.generic_substitution_data(*substitution)
                else {
                    continue;
                };

                pending.extend(substitution.bindings().iter().filter_map(|binding| {
                    match binding.argument() {
                        GenericArgument::Type(ty) => Some(ty),
                        GenericArgument::Constant(_) => None,
                    }
                }));
            }
            TypeData::Tuple(elements) => pending.extend(elements.iter().copied()),
            TypeData::Array { element, .. }
            | TypeData::Slice(element)
            | TypeData::Generator(element)
            | TypeData::Nullable(element) => pending.push(*element),
            TypeData::Borrow { target, .. } => pending.push(*target),
            TypeData::OwnedIndirection { storage, target } => {
                pending.push(*storage);
                pending.push(*target);
            }
            TypeData::Callable(callable) => {
                pending.extend(callable.parameters().iter().map(|parameter| parameter.ty()));
                pending.push(callable.result());
            }
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::TraitView(_) => {}
        }
    }

    false
}

pub(in crate::compilation) fn symbol_is_publicly_reachable(
    declarations: &DeclarationTable,
    symbols: &SymbolGraph,
    mut symbol: AnySymbolId,
) -> bool {
    loop {
        if source_symbol_is_internal(symbol, declarations, symbols) {
            return false;
        }

        if let AnySymbolId::Module(module) = symbol {
            return symbols.module(module).is_some_and(|module| {
                module.origin() == SymbolOrigin::Source && module.visibility().is_public()
            });
        }

        let Some(owner) = symbols.containing_symbol(symbol) else {
            return false;
        };

        symbol = owner;
    }
}

fn source_symbol_is_internal(
    symbol: AnySymbolId,
    declarations: &DeclarationTable,
    symbols: &SymbolGraph,
) -> bool {
    symbols
        .symbol_key(symbol)
        .and_then(SymbolKey::source_declaration_id)
        .and_then(|declaration| declarations.declaration(declaration))
        .is_some_and(|declaration| declaration.surface().is_internal())
}
