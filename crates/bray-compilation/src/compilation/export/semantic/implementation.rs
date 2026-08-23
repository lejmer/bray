use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::CheckedTemplateKind;
use bray_package_interface::{
    InterfaceCoherenceRecord, InterfaceDependencyRequirementKind, InterfaceImplementationRecord,
    InterfacePredicateDefinitionState, InterfaceSymbolReference, InterfaceTraitApplicationId,
    InterfaceTypeId,
};
use bray_symbols::{
    AnySymbolId, ConstantDefinitionState, DependencyRequirementKind, ImplementationCoherenceQuery,
    ImplementationSymbolId, PredicateDefinition, PredicateDefinitionQuery,
    PredicateDefinitionState, StaticInstanceTemplateQuery, StaticStorageDuration, SymbolKind,
    SymbolQueryContract, SymbolQueryRequest, TraitPredicateFulfillmentDefinitionQuery,
    TraitPredicateMemberDefinitionQuery,
};

use super::super::PackageInterfaceExportError;
use super::super::template::export_checked_source_template;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;

use super::context::{CheckedConstantExpression, SemanticExporter};
use super::declarations::ExportedDeclarations;
use super::defaults::{generic_parameters, generic_template_inputs, push_declaration_template};
use super::templates::incomplete;

pub(super) fn implementation_semantics(
    export: &mut SemanticExporter<'_>,
    binder: &CompilationBindingContext<'_>,
    selected: &BTreeSet<AnySymbolId>,
) -> Result<
    (
        Vec<InterfaceImplementationRecord>,
        Vec<InterfaceCoherenceRecord>,
    ),
    PackageInterfaceExportError,
> {
    let mut implementations = Vec::new();

    let mut coherence = BTreeMap::<
        (InterfaceTypeId, InterfaceTraitApplicationId),
        Vec<InterfaceSymbolReference>,
    >::new();

    for symbol in selected.iter().copied() {
        let Some(implementation) = ImplementationSymbolId::try_from_any(symbol) else {
            continue;
        };

        let checked = binder
            .resolve_symbol_query(SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
                implementation,
            ))
            .map_err(|_| incomplete(symbol))?;

        if checked.diagnostics().has_errors() {
            return Err(incomplete(symbol));
        }

        let subject = export.type_id(checked.value().subject())?;

        let trait_application = checked
            .value()
            .trait_application()
            .map(|application| export.trait_application_id(application))
            .transpose()?;

        let reference = export.symbol_reference(symbol)?;

        implementations.push(InterfaceImplementationRecord::new(
            reference.clone(),
            subject,
            trait_application,
        ));

        if let Some(application) = trait_application {
            coherence
                .entry((subject, application))
                .or_default()
                .push(reference);
        }
    }

    let coherence = coherence
        .into_iter()
        .map(|((subject, application), implementations)| {
            InterfaceCoherenceRecord::new(subject, application, implementations)
        })
        .collect();

    Ok((implementations, coherence))
}

pub(super) fn export_constant_semantics(
    compilation: &Compilation,
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    if let AnySymbolId::Static(declaration) = symbol {
        return export_static_semantics(compilation, binder, declaration, export, semantics);
    }

    let Some(definition) = crate::compilation::constant::constant_definition_id(symbol) else {
        return Ok(());
    };

    let definition = compilation
        .constant_definition(definition)
        .map_err(|_| incomplete(symbol))?;

    if definition.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let ConstantDefinitionState::Defined(definition) = definition.value() else {
        return Ok(());
    };

    let kind = CheckedTemplateKind::ConstantDefinition;

    let dependency_contract = export
        .values
        .empty_dependency_contract_template()
        .map_err(|_| incomplete(symbol))?;

    let checked = export.checked_constant_template(
        kind,
        CheckedConstantExpression {
            term: definition.term(),
            ty: definition.ty(),
        },
        dependency_contract,
    )?;

    push_declaration_template(
        export,
        symbol,
        kind,
        bray_symbols::SymbolOrdinal::new(0),
        checked,
        &mut semantics.checked_templates,
        &mut semantics.declaration_templates,
        &mut semantics.support_entities,
    )
}

pub(super) fn export_static_semantics(
    compilation: &Compilation,
    binder: &CompilationBindingContext<'_>,
    declaration: bray_symbols::StaticSymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    let symbol = AnySymbolId::Static(declaration);

    let native = compilation
        .foreign_static_contract(declaration)
        .map_err(|_| incomplete(symbol))?;

    if native.value().as_ref().is_some_and(|contract| {
        contract.direction() == bray_symbols::ForeignCallableDirection::Import
    }) {
        return Ok(());
    }

    let template = binder
        .resolve_symbol_query(SymbolQueryRequest::<StaticInstanceTemplateQuery>::new(
            declaration,
        ))
        .map_err(|_| incomplete(symbol))?;

    if template.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let kind = match template.value().duration() {
        StaticStorageDuration::Product => CheckedTemplateKind::ProductStaticInitializer,
        StaticStorageDuration::ExactThread => CheckedTemplateKind::ThreadLocalStaticInitializer,
    };

    let key = compilation
        .static_initializer_key(declaration)
        .map_err(|_| incomplete(symbol))?
        .ok_or_else(|| incomplete(symbol))?;

    let expression = key.source().syntax();
    let inputs = generic_template_inputs(compilation, export, generic_parameters(binder, symbol)?)?;

    let checked = export_checked_source_template(
        compilation,
        export,
        key,
        kind,
        expression,
        inputs,
        template.value().dependency_contract(),
    )?;

    push_declaration_template(
        export,
        symbol,
        kind,
        bray_symbols::SymbolOrdinal::new(0),
        checked,
        &mut semantics.checked_templates,
        &mut semantics.declaration_templates,
        &mut semantics.support_entities,
    )
}

pub(super) const fn incomplete_type() -> PackageInterfaceExportError {
    PackageInterfaceExportError::IncompletePublicDeclarationSemantics(
        SymbolKind::TypeCallableMember,
    )
}

pub(super) fn predicate_definition(
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<Option<InterfacePredicateDefinitionState>, PackageInterfaceExportError> {
    let state = match symbol {
        AnySymbolId::Predicate(predicate) => resolve_predicate_definition(
            binder,
            symbol,
            SymbolQueryRequest::<PredicateDefinitionQuery>::new(predicate),
        )?,
        AnySymbolId::TraitPredicateMember(predicate) => resolve_predicate_definition(
            binder,
            symbol,
            SymbolQueryRequest::<TraitPredicateMemberDefinitionQuery>::new(predicate),
        )?,
        AnySymbolId::TraitPredicateFulfillment(predicate) => resolve_predicate_definition(
            binder,
            symbol,
            SymbolQueryRequest::<TraitPredicateFulfillmentDefinitionQuery>::new(predicate),
        )?,
        _ => return Ok(None),
    };

    Ok(Some(state))
}

fn resolve_predicate_definition<'a, C>(
    binder: &CompilationBindingContext<'a>,
    symbol: AnySymbolId,
    request: SymbolQueryRequest<C>,
) -> Result<InterfacePredicateDefinitionState, PackageInterfaceExportError>
where
    C: SymbolQueryContract<Value = PredicateDefinitionState<PredicateDefinition>>,
    CompilationBindingContext<'a>: SymbolQueryProvider<C>,
{
    let semantics = binder
        .resolve_symbol_query(request)
        .map_err(|_| incomplete(symbol))?;

    if semantics.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    predicate_definition_state(semantics.value()).ok_or_else(|| incomplete(symbol))
}

const fn predicate_definition_state<T>(
    state: &PredicateDefinitionState<T>,
) -> Option<InterfacePredicateDefinitionState> {
    match state {
        PredicateDefinitionState::Defined(_) => Some(InterfacePredicateDefinitionState::Defined),
        PredicateDefinitionState::Required => Some(InterfacePredicateDefinitionState::Required),
        PredicateDefinitionState::OpaqueTrusted => {
            Some(InterfacePredicateDefinitionState::OpaqueTrusted)
        }
        PredicateDefinitionState::Error(_) => None,
    }
}

pub(super) const fn dependency_requirement_kind(
    kind: DependencyRequirementKind,
) -> InterfaceDependencyRequirementKind {
    match kind {
        DependencyRequirementKind::StorageAlive => InterfaceDependencyRequirementKind::StorageAlive,
        DependencyRequirementKind::StorageInitialized => {
            InterfaceDependencyRequirementKind::StorageInitialized
        }
        DependencyRequirementKind::BorrowCapabilityActive(kind) => {
            InterfaceDependencyRequirementKind::BorrowCapabilityActive(kind)
        }
        DependencyRequirementKind::ExclusiveMutationAuthority => {
            InterfaceDependencyRequirementKind::ExclusiveMutationAuthority
        }
        DependencyRequirementKind::ScopedCapabilityLive => {
            InterfaceDependencyRequirementKind::ScopedCapabilityLive
        }
        DependencyRequirementKind::LifecycleObligation(kind) => {
            InterfaceDependencyRequirementKind::LifecycleObligation(kind)
        }
    }
}
