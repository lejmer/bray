use crate::compilation::template::callable_template_inputs;
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_diagnostics::DiagnosticResult;
use bray_package_interface::{
    InterfaceCallableParameterDefault, InterfaceCallableSignature, InterfaceCheckedTemplate,
    InterfaceCheckedTemplateId, InterfaceConstraint, InterfaceDeclarationTemplate,
    InterfaceDeclaredType, InterfaceGenericDeclaration, InterfacePredicateDefinition,
    InterfaceSupportEntity, InterfaceTypeRepresentation,
};
use bray_symbols::{
    AnySymbolId, CallableContractSet, CallableContractTemplate, CallableContractTemplateQuery,
    CallableContractsQuery, CallableParameterDefaultTemplateQuery, CallableParameterDefaultValue,
    CallableSignatureQuery, CallableSymbolId, CheckedConstraintKind,
    DeclarationPredicateClauseKind, GenericConstraintsQuery, GenericDeclarationTemplateQuery,
    GenericOwnerId, InterfaceSupportEntityId, NamedTypeSymbolId, RuntimeDefaultPresence,
    StructFieldDefaultValue, SymbolQueryRequest, UnionPayloadDefaultValue,
};

use super::super::PackageInterfaceExportError;
use super::super::template::export_checked_source_template;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;

use super::context::SemanticExporter;
use super::defaults::{generic_parameters, push_declaration_template};

use super::templates::{checked_constraint_expression, incomplete, index};

#[derive(Default)]
pub(super) struct ExportedDeclarations {
    pub(super) callable_contract: Option<Arc<DiagnosticResult<CallableContractSet>>>,
    pub(super) signatures: Vec<InterfaceCallableSignature>,
    pub(super) generic_declarations: Vec<InterfaceGenericDeclaration>,
    pub(super) parameter_defaults: Vec<InterfaceCallableParameterDefault>,
    pub(super) constraints: Vec<InterfaceConstraint>,
    pub(super) predicate_definitions: Vec<InterfacePredicateDefinition>,
    pub(super) declared_types: Vec<InterfaceDeclaredType>,
    pub(super) type_representations: Vec<InterfaceTypeRepresentation>,
    pub(super) checked_templates: Vec<InterfaceCheckedTemplate>,
    pub(super) declaration_templates: Vec<InterfaceDeclarationTemplate>,
    pub(super) support_entities: Vec<InterfaceSupportEntity>,
}

pub(super) fn export_callable_semantics(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
        return Ok(());
    };

    let signature = binder
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
        .map_err(super::super::binding_query_export_error)?;

    if signature.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let output = compilation
        .declared_unit_key(symbol, bray_bound_tree::BoundUnitKind::CallableBody)
        .map_err(super::super::fact_query_export_error)?
        .map(|key| compilation.published_source_output(&key, &compilation.state.cancellation))
        .transpose()
        .map_err(super::super::fact_query_export_error)?
        .flatten();

    let result_dependencies = match output
        .as_ref()
        .and_then(|output| output.result_dependencies.as_ref())
    {
        Some(dependencies) => Arc::clone(dependencies),
        None => binder
            .resolve_symbol_query(SymbolQueryRequest::<
                bray_symbols::CallableResultDependenciesQuery,
            >::new(callable))
            .map_err(super::super::binding_query_export_error)?,
    };

    if result_dependencies.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    semantics.signatures.push(export.callable_signature(
        symbol,
        signature.value(),
        *result_dependencies.value(),
    )?);

    let completed_contract = output
        .as_ref()
        .and_then(|output| output.callable_contract.as_ref())
        .map(Arc::clone);

    let contracts = match completed_contract.or(compilation
        .published_callable_contract(callable, &compilation.state.cancellation)
        .map_err(super::super::fact_query_export_error)?)
    {
        Some(contracts) => contracts,
        None => binder
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(callable))
            .map_err(super::super::binding_query_export_error)?,
    };

    if contracts.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    semantics.callable_contract = Some(Arc::clone(&contracts));

    let template = binder
        .resolve_symbol_query(SymbolQueryRequest::<CallableContractTemplateQuery>::new(
            callable,
        ))
        .map_err(super::super::binding_query_export_error)?;

    if template.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let CallableContractTemplate::Source(template) = template.value() else {
        return Ok(());
    };

    for expression in template.expressions() {
        if expression.kind() == DeclarationPredicateClauseKind::Static {
            continue;
        }

        let clause = contracts
            .value()
            .invocation_preconditions()
            .iter()
            .chain(contracts.value().static_constraints())
            .chain(contracts.value().normal_completion_postconditions())
            .find(|clause| clause.ordinal() == expression.ordinal())
            .ok_or_else(|| incomplete(symbol))?;

        let source = compilation
            .bound_source(expression.unit_syntax())
            .map_err(super::super::fact_query_export_error)?;

        let Some(predicate) = clause.predicate() else {
            continue;
        };

        // The unit key must own its Arc-backed symbol identity beyond this graph borrow.
        let owner = graph
            .symbol_key(symbol)
            .cloned()
            .ok_or_else(|| incomplete(symbol))?;

        let key = BoundUnitKey::contract_clause(owner, source).ok_or_else(|| incomplete(symbol))?;

        let inputs = callable_template_inputs(
            compilation,
            signature.value(),
            generic_parameters(binder, symbol)?,
        )
        .map_err(super::super::fact_query_export_error)?;

        let checked = export_checked_source_template(
            compilation,
            export,
            key,
            CheckedTemplateKind::CallableContract,
            expression.expression().syntax(),
            inputs,
            predicate.dependency_contract(),
        )?;

        push_declaration_template(
            export,
            symbol,
            CheckedTemplateKind::CallableContract,
            expression.ordinal(),
            checked,
            &mut semantics.checked_templates,
            &mut semantics.declaration_templates,
            &mut semantics.support_entities,
        )?;
    }

    Ok(())
}

pub(super) fn export_generic_semantics(
    compilation: &Compilation,
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    let Some(owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(());
    };

    let generic = binder
        .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
            owner,
        ))
        .map_err(super::super::binding_query_export_error)?;

    if generic.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    semantics
        .generic_declarations
        .push(export.generic_declaration(symbol, generic.value())?);

    let checked = binder
        .resolve_symbol_query(SymbolQueryRequest::<GenericConstraintsQuery>::new(owner))
        .map_err(super::super::binding_query_export_error)?;

    if checked.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    semantics
        .constraints
        .extend(export.generic_constraints(symbol, checked.value())?);

    for constraint in checked.value().constraints() {
        let CheckedConstraintKind::Predicate(predicate) = constraint.kind() else {
            continue;
        };

        let source = generic
            .value()
            .constraints()
            .iter()
            .find(|source| source.ordinal() == constraint.ordinal())
            .ok_or_else(|| incomplete(symbol))?;

        let unit = source.unit_syntax().ok_or_else(|| incomplete(symbol))?;
        let expression = source.expression().ok_or_else(|| incomplete(symbol))?;

        let checked_expression =
            checked_constraint_expression(compilation, generic.value(), unit, expression)?;

        let checked_id = InterfaceCheckedTemplateId::new(index(semantics.checked_templates.len())?);
        let entity = InterfaceSupportEntityId::new(index(semantics.support_entities.len())?);

        semantics
            .checked_templates
            .push(export.checked_constant_template(
                CheckedTemplateKind::GenericConstraint,
                checked_expression,
                predicate.dependency_contract(),
            )?);

        semantics
            .support_entities
            .push(InterfaceSupportEntity::CheckedTemplate(checked_id));

        semantics
            .declaration_templates
            .push(InterfaceDeclarationTemplate::new(
                export.symbol_reference(symbol)?,
                CheckedTemplateKind::GenericConstraint,
                constraint.ordinal(),
                entity,
            ));
    }

    Ok(())
}

pub(super) fn export_default_semantics(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    match symbol {
        AnySymbolId::CallableParameter(parameter) => {
            let default = binder
                .resolve_symbol_query(
                    SymbolQueryRequest::<CallableParameterDefaultTemplateQuery>::new(parameter),
                )
                .map_err(super::super::binding_query_export_error)?;

            if default.diagnostics().has_errors() {
                return Err(incomplete(symbol));
            }

            semantics
                .parameter_defaults
                .push(InterfaceCallableParameterDefault::new(
                    export.symbol_reference(symbol)?,
                    default.value().is_present(),
                ));

            if default.value().is_present() {
                let checked = compilation
                    .callable_parameter_default(parameter)
                    .map_err(super::super::fact_query_export_error)?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let CallableParameterDefaultValue::Valid(_) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default(
                    compilation,
                    export,
                    checked.value().provider().into(),
                    semantics,
                )?;
            }
        }
        AnySymbolId::StructField(field) => {
            let Some(field) = graph.struct_field(field) else {
                return Err(incomplete(symbol));
            };

            if field.default_presence() != RuntimeDefaultPresence::Absent {
                let checked = compilation
                    .struct_field_default(field.id())
                    .map_err(super::super::fact_query_export_error)?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let StructFieldDefaultValue::Valid(_) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default(
                    compilation,
                    export,
                    checked.value().provider().into(),
                    semantics,
                )?;
            }
        }
        AnySymbolId::UnionPayloadField(field) => {
            let Some(field) = graph.union_payload_field(field) else {
                return Err(incomplete(symbol));
            };

            if field.default_presence() != RuntimeDefaultPresence::Absent {
                let checked = compilation
                    .union_payload_field_default(field.id())
                    .map_err(super::super::fact_query_export_error)?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let UnionPayloadDefaultValue::Valid(_) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default(
                    compilation,
                    export,
                    checked.value().provider().into(),
                    semantics,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

pub(super) fn export_type_semantics(
    compilation: &Compilation,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    if let Some(template) = compilation
        .symbol_type_template(symbol)
        .map_err(super::super::fact_query_export_error)?
    {
        if template.diagnostics().has_errors() {
            return Err(incomplete(symbol));
        }

        let ty = export.resolve_type_template(symbol, template.value())?;

        semantics.declared_types.push(InterfaceDeclaredType::new(
            export.symbol_reference(symbol)?,
            export.type_id(ty)?,
        ));
    }

    let Some(named_type) = NamedTypeSymbolId::try_from_any(symbol) else {
        return Ok(());
    };

    let representation = compilation
        .declared_type_representation(named_type)
        .map_err(super::super::fact_query_export_error)?;

    if representation.diagnostics().has_errors() || representation.value().is_recovered() {
        return Err(incomplete(symbol));
    }

    semantics
        .type_representations
        .push(export.type_representation(symbol, representation.value())?);

    Ok(())
}

fn export_runtime_default(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    provider: AnySymbolId,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    let key = compilation
        .declared_unit_key(provider, bray_bound_tree::BoundUnitKind::RuntimeDefault)
        .map_err(super::super::fact_query_export_error)?
        .expect("source default retains its declaration-owned unit");

    let cached = compilation
        .constant_body_template(key, &compilation.state.cancellation)
        .map_err(super::super::fact_query_export_error)?;

    let template = cached.result().value().as_ref().ok_or_else(|| {
        super::super::error::source_diagnostic_export_error(cached.result().diagnostics())
            .expect("missing checked source default retains its diagnostics")
    })?;

    let checked = super::super::template::encode_template(export, template)?;

    push_declaration_template(
        export,
        provider,
        CheckedTemplateKind::RuntimeDefault,
        bray_symbols::SymbolOrdinal::new(0),
        checked,
        &mut semantics.checked_templates,
        &mut semantics.declaration_templates,
        &mut semantics.support_entities,
    )
}
