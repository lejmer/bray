// rust-style: allow(module-too-large, reason = "public surface validation shares one recursive semantic dependency closure")

use std::collections::BTreeSet;

use bray_binder::SymbolQueryProvider;
use bray_declarations::DeclarationTable;
use bray_diagnostics::{
    DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_symbols::{
    AnySymbolId, CallableContractTypeQuery, CallableInstanceData, CallableSignatureQuery,
    CallableSymbolId, ConstantDeclaredTypeQuery, ConstantField, ConstantProjectionKind,
    ConstantTermData, ConstantTermId, GenericArgument, GenericArgumentTemplate,
    GenericConstParameterDeclaredTypeQuery, GenericSubstitutionId, ImplementationInstanceId,
    InherentTypeMemberValueQuery, PredicateDefinitionSymbolId, PredicateSignatureTemplateQuery,
    SemanticValueStore, StructFieldTypeQuery, SymbolGraph, SymbolKey, SymbolOrigin,
    SymbolQueryContract, SymbolQueryRequest, TraitApplicationId,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantMemberDeclaredTypeQuery,
    TraitTypeFulfillmentValueQuery, TypeData, TypeExpressionTemplate, TypeId,
    UnionPayloadFieldTypeQuery,
};

use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::compilation::diagnostics::source_diagnostic;
use crate::fact::FactQueryError;

pub(super) fn validate_public_surface(
    binder: &CompilationBindingContext<'_>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    public_symbols: &BTreeSet<AnySymbolId>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, FactQueryError> {
    let mut is_recovered = false;

    for symbol in public_symbols {
        let exposes_internal = match *symbol {
            AnySymbolId::Constant(owner) => validate_type_template::<ConstantDeclaredTypeQuery>(
                binder,
                owner,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?,
            AnySymbolId::TraitConstantMember(owner) => {
                validate_type_template::<TraitConstantMemberDeclaredTypeQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::TraitConstantFulfillment(owner) => {
                validate_type_template::<TraitConstantFulfillmentDeclaredTypeQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::GenericConstParameter(owner) => {
                validate_type_template::<GenericConstParameterDeclaredTypeQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::StructField(owner) => validate_type_template::<StructFieldTypeQuery>(
                binder,
                owner,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?,
            AnySymbolId::UnionPayloadField(owner) => {
                validate_type_template::<UnionPayloadFieldTypeQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::InherentTypeMember(owner) => {
                validate_type_template::<InherentTypeMemberValueQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::TraitTypeFulfillment(owner) => {
                validate_type_template::<TraitTypeFulfillmentValueQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            AnySymbolId::CallableContract(owner) => {
                validate_type_template::<CallableContractTypeQuery>(
                    binder,
                    owner,
                    semantic_values,
                    symbols,
                    declarations,
                    diagnostics,
                )?
            }
            _ => None,
        };

        let exposes_internal = if let Some(callable) = CallableSymbolId::try_from_any(*symbol) {
            validate_callable_signature(
                binder,
                callable,
                semantic_values,
                symbols,
                declarations,
                diagnostics,
            )?
            .or(exposes_internal)
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
                )?
                .or(exposes_internal)
            } else {
                exposes_internal
            };

        if let Some(internal) = exposes_internal {
            add_internal_dependency_diagnostic(*symbol, internal, symbols, diagnostics)?;

            is_recovered = true;
        }
    }

    Ok(is_recovered)
}

fn validate_callable_signature(
    binder: &CompilationBindingContext<'_>,
    callable: CallableSymbolId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let signature = binder
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
        .map_err(binding_query_error)?;

    diagnostics.add_range(signature.diagnostics().iter().cloned());

    Ok(template_internal_dependency(
        signature.value().callable_type(),
        semantic_values,
        symbols,
        declarations,
    ))
}

fn validate_predicate_signature(
    binder: &CompilationBindingContext<'_>,
    predicate: PredicateDefinitionSymbolId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<AnySymbolId>, FactQueryError> {
    let signature = binder
        .resolve_symbol_query(SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
            predicate,
        ))
        .map_err(binding_query_error)?;

    diagnostics.add_range(signature.diagnostics().iter().cloned());

    Ok(signature.value().parameters().iter().find_map(|parameter| {
        template_internal_dependency(parameter.ty(), semantic_values, symbols, declarations)
    }))
}

fn validate_type_template<C>(
    binder: &CompilationBindingContext<'_>,
    owner: C::Owner,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<AnySymbolId>, FactQueryError>
where
    C: SymbolQueryContract<Value = TypeExpressionTemplate>,
    for<'binding> CompilationBindingContext<'binding>: SymbolQueryProvider<C>,
{
    let result = binder
        .resolve_symbol_query(SymbolQueryRequest::<C>::new(owner))
        .map_err(binding_query_error)?;

    diagnostics.add_range(result.diagnostics().iter().cloned());

    Ok(template_internal_dependency(
        result.value(),
        semantic_values,
        symbols,
        declarations,
    ))
}

pub(super) fn add_internal_dependency_diagnostic(
    owner: AnySymbolId,
    internal: AnySymbolId,
    symbols: &SymbolGraph,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), FactQueryError> {
    let Some(anchor) = symbols.declaration_syntax_anchor(owner) else {
        return Ok(());
    };

    let identity =
        crate::compilation::diagnostics::symbol_diagnostic_identity(symbols, None, internal)?;

    let mut diagnostic = source_diagnostic(
        anchor,
        DiagnosticKind::CheckingExportDependsOnInternalDeclaration,
    )
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::InvalidProductConfiguration,
        bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range()),
    ))
    .with_arg(DiagnosticArg::interface_symbol_identity(identity))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::PublicDependencyRequired,
    ));

    if let Some(internal_anchor) = symbols.declaration_syntax_anchor(internal) {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::RequirementOrigin,
            bray_source::SourceSpan::new(internal_anchor.source_id(), internal_anchor.full_range()),
        ));
    }

    diagnostics.add(diagnostic);

    Ok(())
}

fn template_internal_dependency(
    template: &TypeExpressionTemplate,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let mut pending = vec![template];

    while let Some(template) = pending.pop() {
        match template {
            TypeExpressionTemplate::Resolved(ty) => {
                if let Some(internal) =
                    resolved_type_internal_dependency(*ty, semantic_values, symbols, declarations)
                {
                    return Some(internal);
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
                    return Some(definition.into_any());
                }

                if let Some(internal) = template_arguments_internal_dependency(
                    arguments,
                    &mut pending,
                    semantic_values,
                    symbols,
                    declarations,
                ) {
                    return Some(internal);
                }
            }
            TypeExpressionTemplate::CallableContract {
                definition,
                target,
                arguments,
                ..
            } => {
                if source_symbol_is_not_publicly_reachable(
                    (*definition).into(),
                    declarations,
                    symbols,
                ) {
                    return Some((*definition).into());
                }

                pending.push(target);

                if let Some(internal) = template_arguments_internal_dependency(
                    arguments,
                    &mut pending,
                    semantic_values,
                    symbols,
                    declarations,
                ) {
                    return Some(internal);
                }
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
                    return Some(application.definition().into());
                }

                pending.push(subject);

                if let Some(internal) = template_arguments_internal_dependency(
                    application.arguments(),
                    &mut pending,
                    semantic_values,
                    symbols,
                    declarations,
                ) {
                    return Some(internal);
                }
            }
            TypeExpressionTemplate::Tuple(elements) => pending.extend(elements.iter()),
            TypeExpressionTemplate::Array { element, .. }
            | TypeExpressionTemplate::FlexibleArray(element)
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
                    return Some(application.definition().into());
                }

                if let Some(internal) = template_arguments_internal_dependency(
                    application.arguments(),
                    &mut pending,
                    semantic_values,
                    symbols,
                    declarations,
                ) {
                    return Some(internal);
                }
            }
        }
    }

    None
}

fn template_arguments_internal_dependency<'a>(
    arguments: &'a [GenericArgumentTemplate],
    pending: &mut Vec<&'a TypeExpressionTemplate>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let internal = arguments.iter().find_map(|argument| {
        resolved_template_argument_internal_dependency(
            argument,
            semantic_values,
            symbols,
            declarations,
        )
    });

    if internal.is_none() {
        pending.extend(arguments.iter().filter_map(|argument| match argument {
            GenericArgumentTemplate::Type(ty) => Some(ty),
            GenericArgumentTemplate::Resolved(_) | GenericArgumentTemplate::Constant(_) => None,
        }));
    }

    internal
}

fn resolved_template_argument_internal_dependency(
    argument: &GenericArgumentTemplate,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let GenericArgumentTemplate::Resolved(GenericArgument::Type(ty)) = argument else {
        return None;
    };

    resolved_type_internal_dependency(*ty, semantic_values, symbols, declarations)
}

pub(super) fn resolved_type_internal_dependency(
    ty: TypeId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    semantic_values_internal_dependency(
        [SemanticValueDependency::Type(ty)],
        semantic_values,
        symbols,
        declarations,
    )
}

pub(super) fn substitution_internal_dependency(
    substitution: GenericSubstitutionId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    semantic_values_internal_dependency(
        [SemanticValueDependency::Substitution(substitution)],
        semantic_values,
        symbols,
        declarations,
    )
}

pub(super) fn callable_instance_internal_dependency(
    instance: CallableInstanceData,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    if source_symbol_is_not_publicly_reachable(
        instance.definition().symbol(),
        declarations,
        symbols,
    ) {
        Some(instance.definition().symbol())
    } else {
        substitution_internal_dependency(
            instance.substitution(),
            semantic_values,
            symbols,
            declarations,
        )
    }
}

pub(super) fn implementation_instance_internal_dependency(
    instance: ImplementationInstanceId,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    semantic_values_internal_dependency(
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

fn semantic_values_internal_dependency(
    roots: impl IntoIterator<Item = SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();

    while let Some(dependency) = pending.pop() {
        if !visited.insert(dependency) {
            continue;
        }

        let internal = match dependency {
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

        if internal.is_some() {
            return internal;
        }
    }

    None
}

fn type_exposes_internal(
    ty: TypeId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let Ok(data) = semantic_values.type_data(ty) else {
        return None;
    };

    match data.as_ref() {
        TypeData::Error => {}
        TypeData::Named {
            definition,
            substitution,
        } => {
            if source_symbol_is_not_publicly_reachable(definition.into_any(), declarations, symbols)
            {
                return Some(definition.into_any());
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
                return Some((*member).into());
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
        TypeData::FlexibleArray(element)
        | TypeData::Slice(element)
        | TypeData::Generator(element)
        | TypeData::Nullable(element) => {
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

    None
}

fn substitution_exposes_internal_value(
    substitution: GenericSubstitutionId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
) -> Option<AnySymbolId> {
    let Ok(substitution) = semantic_values.generic_substitution_data(substitution) else {
        return None;
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

    None
}

fn constant_term_exposes_internal(
    term: ConstantTermId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let Ok(data) = semantic_values.constant_term_data(term) else {
        return None;
    };

    match data.as_ref() {
        ConstantTermData::Typed { term, ty } => {
            pending.push(SemanticValueDependency::ConstantTerm(*term));
            pending.push(SemanticValueDependency::Type(*ty));
        }
        ConstantTermData::Value(value) => {
            let Ok(value) = semantic_values.constant_value_data(*value) else {
                return None;
            };

            pending.push(SemanticValueDependency::Type(value.ty()));
        }
        ConstantTermData::IntegerLiteral { .. } => {}
        ConstantTermData::CallableArgument(_) => {}
        ConstantTermData::Parameter(_) => {}
        ConstantTermData::TargetProperty(constant) => {
            return source_symbol_is_not_publicly_reachable(
                (*constant).into(),
                declarations,
                symbols,
            )
            .then_some((*constant).into());
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
            if let Some(internal) =
                constant_fields_expose_internal(fields, pending, symbols, declarations)
            {
                return Some(internal);
            }
        }
        ConstantTermData::Union { variant, fields } => {
            if source_symbol_is_not_publicly_reachable((*variant).into(), declarations, symbols) {
                return Some((*variant).into());
            }

            if let Some(internal) =
                constant_fields_expose_internal(fields, pending, symbols, declarations)
            {
                return Some(internal);
            }
        }
        ConstantTermData::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => {
            if source_symbol_is_not_publicly_reachable(definition.into_any(), declarations, symbols)
            {
                return Some(definition.into_any());
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
                return None;
            };

            if source_symbol_is_not_publicly_reachable(
                callable.definition().symbol(),
                declarations,
                symbols,
            ) {
                return Some(callable.definition().symbol());
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
        ConstantTermData::PredicateCall {
            predicate,
            arguments,
        } => {
            if source_symbol_is_not_publicly_reachable(
                predicate.definition().into_any(),
                declarations,
                symbols,
            ) {
                return Some(predicate.definition().into_any());
            }

            pending.push(SemanticValueDependency::Substitution(
                predicate.substitution(),
            ));

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
                    )
                    .then_some(field.into());
                }
                ConstantProjectionKind::UnionPayloadField(field) => {
                    return source_symbol_is_not_publicly_reachable(
                        field.into(),
                        declarations,
                        symbols,
                    )
                    .then_some(field.into());
                }
                ConstantProjectionKind::TupleElement(_) | ConstantProjectionKind::NullableValue => {
                }
            }
        }
    }

    None
}

fn constant_fields_expose_internal<I>(
    fields: &[ConstantField<I, ConstantTermId>],
    pending: &mut Vec<SemanticValueDependency>,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId>
where
    I: Copy + Into<AnySymbolId>,
{
    for entry in fields {
        let field = (*entry.field()).into();

        if source_symbol_is_not_publicly_reachable(field, declarations, symbols) {
            return Some(field);
        }

        pending.push(SemanticValueDependency::ConstantTerm(*entry.value()));
    }

    None
}

fn trait_application_exposes_internal(
    application: TraitApplicationId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let Ok(application) = semantic_values.trait_application_data(application) else {
        return None;
    };

    if source_symbol_is_not_publicly_reachable(
        application.definition().into(),
        declarations,
        symbols,
    ) {
        return Some(application.definition().into());
    }

    pending.push(SemanticValueDependency::Substitution(
        application.substitution(),
    ));

    None
}

fn implementation_exposes_internal(
    implementation: ImplementationInstanceId,
    pending: &mut Vec<SemanticValueDependency>,
    semantic_values: &SemanticValueStore,
    symbols: &SymbolGraph,
    declarations: &DeclarationTable,
) -> Option<AnySymbolId> {
    let Ok(implementation) = semantic_values.implementation_instance_data(implementation) else {
        return None;
    };

    if source_symbol_is_not_publicly_reachable(
        implementation.definition().into_any(),
        declarations,
        symbols,
    ) {
        return Some(implementation.definition().into_any());
    }

    pending.push(SemanticValueDependency::Substitution(
        implementation.substitution(),
    ));

    None
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
