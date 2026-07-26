use std::collections::BTreeSet;

use bray_binder::SymbolFactProvider;
use bray_declarations::DeclarationTable;
use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_symbols::{
    AnySymbolId, CallableContractTypeFact, CallableInstanceData, CallableSignatureFact,
    CallableSymbolId, ConstantDeclaredTypeFact, ConstantProjectionKind, ConstantTermData,
    ConstantTermId, GenericArgument, GenericArgumentTemplate,
    GenericConstParameterDeclaredTypeFact, GenericSubstitutionId, ImplementationInstanceId,
    InherentTypeMemberValueFact, PredicateDefinitionSymbolId, PredicateSignatureTemplateFact,
    SemanticValueStore, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest, SymbolGraph,
    SymbolKey, SymbolOrigin, TraitApplicationId, TraitConstantFulfillmentDeclaredTypeFact,
    TraitConstantMemberDeclaredTypeFact, TraitTypeFulfillmentValueFact, TypeData,
    TypeExpressionTemplate, TypeId, UnionPayloadFieldTypeFact,
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

        let exposes_internal =
            if let Some(predicate) = PredicateDefinitionSymbolId::try_from_any(*symbol) {
                validate_predicate_signature(
                    binder,
                    predicate,
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

pub(super) fn add_internal_dependency_diagnostic(
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
                if source_symbol_is_not_publicly_reachable(
                    definition.into_any(),
                    declarations,
                    symbols,
                ) {
                    return true;
                }

                pending.extend(arguments.iter().filter_map(|argument| match argument {
                    GenericArgumentTemplate::Type(ty) => Some(ty),
                    GenericArgumentTemplate::Constant(_) => None,
                }));
            }
            TypeExpressionTemplate::TypeValuedMemberProjection {
                subject,
                application,
                ..
            } => {
                if source_symbol_is_not_publicly_reachable(
                    application.definition().into(),
                    declarations,
                    symbols,
                ) {
                    return true;
                }

                pending.push(subject);

                pending.extend(application.arguments().iter().filter_map(
                    |argument| match argument {
                        GenericArgumentTemplate::Type(ty) => Some(ty),
                        GenericArgumentTemplate::Constant(_) => None,
                    },
                ));
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
            TypeExpressionTemplate::TraitView(application) => {
                if source_symbol_is_not_publicly_reachable(
                    application.definition().into(),
                    declarations,
                    symbols,
                ) {
                    return true;
                }

                pending.extend(application.arguments().iter().filter_map(
                    |argument| match argument {
                        GenericArgumentTemplate::Type(ty) => Some(ty),
                        GenericArgumentTemplate::Constant(_) => None,
                    },
                ));
            }
        }
    }

    false
}

pub(super) fn resolved_type_exposes_internal(
    ty: TypeId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    semantic_values_expose_internal(
        [SemanticValueDependency::Type(ty)],
        semantic_values,
        symbols,
        declarations,
    )
}

pub(super) fn substitution_exposes_internal(
    substitution: GenericSubstitutionId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    semantic_values_expose_internal(
        [SemanticValueDependency::Substitution(substitution)],
        semantic_values,
        symbols,
        declarations,
    )
}

pub(super) fn callable_instance_exposes_internal(
    instance: CallableInstanceData,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    source_symbol_is_not_publicly_reachable(instance.definition().symbol(), declarations, symbols)
        || substitution_exposes_internal(
            instance.substitution(),
            semantic_values,
            symbols,
            declarations,
        )
}

pub(super) fn implementation_instance_exposes_internal(
    instance: ImplementationInstanceId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    semantic_values_expose_internal(
        [SemanticValueDependency::Implementation(instance)],
        semantic_values,
        symbols,
        declarations,
    )
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SemanticValueDependency {
    Type(TypeId),
    ConstantTerm(ConstantTermId),
    Substitution(GenericSubstitutionId),
    TraitApplication(TraitApplicationId),
    Implementation(ImplementationInstanceId),
}

fn semantic_values_expose_internal(
    roots: impl IntoIterator<Item = SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();

    while let Some(dependency) = pending.pop() {
        if !visited.insert(dependency) {
            continue;
        }

        let exposes_internal = match dependency {
            SemanticValueDependency::Type(ty) => {
                type_exposes_internal(ty, &mut pending, semantic_values, symbols, declarations)
            }
            SemanticValueDependency::ConstantTerm(term) => constant_term_exposes_internal(
                term,
                &mut pending,
                semantic_values,
                symbols,
                declarations,
            ),
            SemanticValueDependency::Substitution(substitution) => {
                substitution_exposes_internal_value(substitution, &mut pending, semantic_values)
            }
            SemanticValueDependency::TraitApplication(application) => {
                trait_application_exposes_internal(
                    application,
                    &mut pending,
                    semantic_values,
                    symbols,
                    declarations,
                )
            }
            SemanticValueDependency::Implementation(implementation) => {
                implementation_exposes_internal(
                    implementation,
                    &mut pending,
                    semantic_values,
                    symbols,
                    declarations,
                )
            }
        };

        if exposes_internal {
            return true;
        }
    }

    false
}

fn type_exposes_internal(
    ty: TypeId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let Ok(data) = semantic_values.type_data(ty) else {
        return false;
    };

    match data.as_ref() {
        TypeData::Error => {}
        TypeData::Named {
            definition,
            substitution,
        } => {
            if source_symbol_is_not_publicly_reachable(definition.into_any(), declarations, symbols)
            {
                return true;
            }

            pending.push(SemanticValueDependency::Substitution(*substitution));
        }
        TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => {}
        TypeData::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => {
            if source_symbol_is_not_publicly_reachable((*member).into(), declarations, symbols) {
                return true;
            }

            pending.push(SemanticValueDependency::Type(*subject));
            pending.push(SemanticValueDependency::TraitApplication(*application));
        }
        TypeData::Tuple(elements) => {
            pending.extend(elements.iter().copied().map(SemanticValueDependency::Type));
        }
        TypeData::Array { element, length } => {
            pending.push(SemanticValueDependency::Type(*element));
            pending.push(SemanticValueDependency::ConstantTerm(*length));
        }
        TypeData::Slice(element) | TypeData::Generator(element) | TypeData::Nullable(element) => {
            pending.push(SemanticValueDependency::Type(*element));
        }
        TypeData::Borrow { target, .. } => {
            pending.push(SemanticValueDependency::Type(*target));
        }
        TypeData::TraitView(application) => {
            pending.push(SemanticValueDependency::TraitApplication(*application));
        }
        TypeData::OwnedIndirection { storage, target } => {
            pending.push(SemanticValueDependency::Type(*storage));
            pending.push(SemanticValueDependency::Type(*target));
        }
        TypeData::Callable(callable) => {
            pending.extend(
                callable
                    .parameters()
                    .iter()
                    .map(|parameter| SemanticValueDependency::Type(parameter.ty())),
            );

            pending.push(SemanticValueDependency::Type(callable.result()));
        }
    }

    false
}

fn substitution_exposes_internal_value(
    substitution: GenericSubstitutionId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
) -> bool {
    let Ok(substitution) = semantic_values.generic_substitution_data(substitution) else {
        return false;
    };

    pending.extend(
        substitution
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => SemanticValueDependency::Type(ty),
                GenericArgument::Constant(term) => SemanticValueDependency::ConstantTerm(term),
            }),
    );

    false
}

fn constant_term_exposes_internal(
    term: ConstantTermId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let Ok(data) = semantic_values.constant_term_data(term) else {
        return false;
    };

    match data.as_ref() {
        ConstantTermData::Value(value) => {
            let Ok(value) = semantic_values.constant_value_data(*value) else {
                return false;
            };

            pending.push(SemanticValueDependency::Type(value.ty()));
        }
        ConstantTermData::IntegerLiteral { .. } => {}
        ConstantTermData::Parameter(_) => {}
        ConstantTermData::TargetFact(constant) => {
            return source_symbol_is_not_publicly_reachable(
                (*constant).into(),
                declarations,
                symbols,
            );
        }
        ConstantTermData::Unary { operand, .. } => {
            pending.push(SemanticValueDependency::ConstantTerm(*operand));
        }
        ConstantTermData::Binary { left, right, .. } => {
            pending.push(SemanticValueDependency::ConstantTerm(*left));
            pending.push(SemanticValueDependency::ConstantTerm(*right));
        }
        ConstantTermData::Conversion { operand, target } => {
            pending.push(SemanticValueDependency::ConstantTerm(*operand));
            pending.push(SemanticValueDependency::Type(*target));
        }
        ConstantTermData::NullablePresent(value) => {
            pending.push(SemanticValueDependency::ConstantTerm(*value));
        }
        ConstantTermData::Tuple(values) | ConstantTermData::Array(values) => {
            pending.extend(
                values
                    .iter()
                    .copied()
                    .map(SemanticValueDependency::ConstantTerm),
            );
        }
        ConstantTermData::Product(fields) => {
            for field in fields.iter() {
                if source_symbol_is_not_publicly_reachable(
                    (*field.field()).into(),
                    declarations,
                    symbols,
                ) {
                    return true;
                }

                pending.push(SemanticValueDependency::ConstantTerm(*field.value()));
            }
        }
        ConstantTermData::Union { variant, fields } => {
            if source_symbol_is_not_publicly_reachable((*variant).into(), declarations, symbols) {
                return true;
            }

            for field in fields.iter() {
                if source_symbol_is_not_publicly_reachable(
                    (*field.field()).into(),
                    declarations,
                    symbols,
                ) {
                    return true;
                }

                pending.push(SemanticValueDependency::ConstantTerm(*field.value()));
            }
        }
        ConstantTermData::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => {
            if source_symbol_is_not_publicly_reachable(definition.into_any(), declarations, symbols)
            {
                return true;
            }

            pending.push(SemanticValueDependency::Substitution(*substitution));

            if let Some(implementation) = selected_implementation {
                pending.push(SemanticValueDependency::Implementation(*implementation));
            }
        }
        ConstantTermData::Call {
            callable,
            selected_implementation,
            arguments,
        } => {
            let Ok(callable) = semantic_values.callable_instance_data(*callable) else {
                return false;
            };

            if source_symbol_is_not_publicly_reachable(
                callable.definition().symbol(),
                declarations,
                symbols,
            ) {
                return true;
            }

            pending.push(SemanticValueDependency::Substitution(
                callable.substitution(),
            ));

            if let Some(implementation) = selected_implementation {
                pending.push(SemanticValueDependency::Implementation(*implementation));
            }

            pending.extend(
                arguments
                    .iter()
                    .copied()
                    .map(SemanticValueDependency::ConstantTerm),
            );
        }
        ConstantTermData::Projection(projection) => {
            pending.push(SemanticValueDependency::ConstantTerm(projection.subject()));

            match projection.kind() {
                ConstantProjectionKind::ArrayElement(index) => {
                    pending.push(SemanticValueDependency::ConstantTerm(index));
                }
                ConstantProjectionKind::ProductField(field) => {
                    return source_symbol_is_not_publicly_reachable(
                        field.into(),
                        declarations,
                        symbols,
                    );
                }
                ConstantProjectionKind::UnionPayloadField(field) => {
                    return source_symbol_is_not_publicly_reachable(
                        field.into(),
                        declarations,
                        symbols,
                    );
                }
                ConstantProjectionKind::TupleElement(_) | ConstantProjectionKind::NullableValue => {
                }
            }
        }
    }

    false
}

fn trait_application_exposes_internal(
    application: TraitApplicationId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let Ok(application) = semantic_values.trait_application_data(application) else {
        return false;
    };

    if source_symbol_is_not_publicly_reachable(
        application.definition().into(),
        declarations,
        symbols,
    ) {
        return true;
    }

    pending.push(SemanticValueDependency::Substitution(
        application.substitution(),
    ));

    false
}

fn implementation_exposes_internal(
    implementation: ImplementationInstanceId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> bool {
    let Ok(implementation) = semantic_values.implementation_instance_data(implementation) else {
        return false;
    };

    if source_symbol_is_not_publicly_reachable(
        implementation.definition().into_any(),
        declarations,
        symbols,
    ) {
        return true;
    }

    pending.push(SemanticValueDependency::Substitution(
        implementation.substitution(),
    ));

    false
}

pub(in crate::compilation) fn symbol_is_publicly_reachable(
    declarations: &DeclarationTable,
    symbols: &SymbolGraph,
    mut symbol: AnySymbolId,
) -> bool {
    loop {
        if symbols
            .symbol_key(symbol)
            .and_then(SymbolKey::source_declaration_id)
            .and_then(|declaration| declarations.declaration(declaration))
            .is_some_and(|declaration| declaration.surface().is_internal())
        {
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

pub(super) fn source_symbol_is_not_publicly_reachable(
    symbol: AnySymbolId,
    declarations: &DeclarationTable,
    symbols: &SymbolGraph,
) -> bool {
    symbols
        .symbol_key(symbol)
        .and_then(SymbolKey::source_declaration_id)
        .is_some()
        && !symbol_is_publicly_reachable(declarations, symbols, symbol)
}
