use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundReferenceTarget, CheckedTemplateKind};
use bray_package_interface::{
    InterfaceCheckedTemplateInputKind, InterfacePredicateDefinition,
    InterfacePredicateDefinitionState,
};
use bray_symbols::{
    AnySymbolId, PredicateDefinitionState, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateQuery, SymbolOrdinal, SymbolQueryRequest,
};

use super::super::PackageInterfaceExportError;
use super::super::template::{SourceTemplateInput, export_checked_source_template};
use super::context::SemanticExporter;
use super::declarations::ExportedDeclarations;
use super::defaults::{generic_parameters, generic_template_inputs, push_declaration_template};
use super::implementation::predicate_definition;
use super::templates::incomplete;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;

pub(super) fn export_predicate_semantics(
    compilation: &Compilation,
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    let Some(definition) = predicate_definition(binder, symbol)? else {
        return Ok(());
    };

    let (state, checked) = match definition {
        PredicateDefinitionState::Defined(definition) => {
            (InterfacePredicateDefinitionState::Defined, Some(definition))
        }
        PredicateDefinitionState::Required => (InterfacePredicateDefinitionState::Required, None),
        PredicateDefinitionState::OpaqueTrusted => {
            (InterfacePredicateDefinitionState::OpaqueTrusted, None)
        }
        PredicateDefinitionState::Error(_) => return Err(incomplete(symbol)),
    };

    let owner =
        PredicateDefinitionSymbolId::try_from_any(symbol).ok_or_else(|| incomplete(symbol))?;

    let signature = binder
        .resolve_symbol_query(SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
            owner,
        ))
        .map_err(super::super::binding_query_export_error)?;

    if signature.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let mut parameters = Vec::new();

    for parameter in signature.value().parameters() {
        let ty = export.resolve_type_template(symbol, parameter.ty())?;

        // Interface records retain immutable parameter names beyond the queried signature.
        parameters.push((
            export.symbol_reference(parameter.parameter().into())?,
            parameter.name().clone(),
            export.type_id(ty)?,
        ));
    }

    semantics.predicate_definitions.push(
        InterfacePredicateDefinition::new(export.symbol_reference(symbol)?, state)
            .with_parameters(parameters),
    );

    let Some(checked) = checked else {
        return Ok(());
    };

    let key = compilation
        .predicate_definition_key(owner)
        .map_err(super::super::fact_query_export_error)?
        .ok_or_else(|| incomplete(symbol))?;

    let expression = key.source().syntax();
    let mut inputs = Vec::new();

    for (index, parameter) in signature.value().parameters().iter().enumerate() {
        let ordinal = u32::try_from(index).map_err(|_| {
            super::super::capacity_export_error("predicate_parameter_ordinal", index)
        })?;

        inputs.push(SourceTemplateInput::new(
            InterfaceCheckedTemplateInputKind::Parameter(SymbolOrdinal::new(ordinal)),
            Some(BoundReferenceTarget::Surface(parameter.parameter().into())),
            export.resolve_type_template(symbol, parameter.ty())?,
        ));
    }

    inputs.extend(generic_template_inputs(
        compilation,
        export,
        generic_parameters(binder, symbol)?,
    )?);

    let kind = CheckedTemplateKind::PredicateDefinition;

    let template = export_checked_source_template(
        compilation,
        export,
        key,
        kind,
        expression,
        inputs,
        checked.semantic().dependency_contract(),
    )?;

    push_declaration_template(
        export,
        symbol,
        kind,
        SymbolOrdinal::new(0),
        template,
        &mut semantics.checked_templates,
        &mut semantics.declaration_templates,
        &mut semantics.support_entities,
    )
}
