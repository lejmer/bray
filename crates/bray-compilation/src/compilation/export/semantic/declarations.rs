use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_package_interface::{
    InterfaceCallableContract, InterfaceCallableParameterDefault, InterfaceCallableSignature,
    InterfaceCheckedTemplate, InterfaceCheckedTemplateId, InterfaceConstraint,
    InterfaceDeclarationTemplate, InterfaceDeclaredType, InterfaceGenericDeclaration,
    InterfacePredicateDefinition, InterfaceSupportEntity, InterfaceTypeRepresentation,
};
use bray_symbols::{
    AnySymbolId, CallableContractTemplate, CallableContractTemplateQuery, CallableContractsQuery,
    CallableParameterDefaultTemplateQuery, CallableParameterDefaultValue, CallableSignatureQuery,
    CallableSymbolId, CheckedConstraintKind, DeclarationPredicateClauseKind,
    GenericConstraintsQuery, GenericDeclarationTemplateQuery, GenericOwnerId,
    InterfaceSupportEntityId, NamedTypeSymbolId, RuntimeDefaultGenericContext,
    RuntimeDefaultPresence, RuntimeDefaultProviderInput, RuntimeDefaultTemplateReference,
    StructFieldDefaultValue, SymbolQueryRequest, UnionPayloadDefaultValue,
};

use super::super::PackageInterfaceExportError;
use super::super::template::export_checked_source_template;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;

use super::context::SemanticExporter;
use super::defaults::{
    callable_template_inputs, generic_parameters, generic_template_inputs,
    push_declaration_template, runtime_default_inputs,
};
use super::implementation::predicate_definition;
use super::templates::{checked_constraint_expression, incomplete, index};

#[derive(Default)]
pub(super) struct ExportedDeclarations {
    pub(super) signatures: Vec<InterfaceCallableSignature>,
    pub(super) generic_declarations: Vec<InterfaceGenericDeclaration>,
    pub(super) parameter_defaults: Vec<InterfaceCallableParameterDefault>,
    pub(super) constraints: Vec<InterfaceConstraint>,
    pub(super) callable_contracts: Vec<InterfaceCallableContract>,
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

    semantics
        .signatures
        .push(export.callable_signature(symbol, signature.value())?);

    let contracts = binder
        .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(callable))
        .map_err(super::super::binding_query_export_error)?;

    if contracts.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    semantics
        .callable_contracts
        .push(export.callable_contract(symbol, contracts.value())?);

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
            export,
            signature.value(),
            generic_parameters(binder, symbol)?,
        )?;

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

pub(super) fn export_predicate_semantics(
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    export: &SemanticExporter<'_>,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    let Some(state) = predicate_definition(binder, symbol)? else {
        return Ok(());
    };

    semantics
        .predicate_definitions
        .push(InterfacePredicateDefinition::new(
            export.symbol_reference(symbol)?,
            state,
        ));

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
                .map_err(|error| {
                    super::super::binding_query_export_error(error)
                })?;

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
                let checked =
                    compilation
                        .callable_parameter_default(parameter)
                        .map_err(|error| {
                            super::super::fact_query_export_error(error)
                        })?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let CallableParameterDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default_surface(
                    compilation,
                    graph,
                    export,
                    checked.value().provider().into(),
                    surface,
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
                    .map_err(|error| {
                        super::super::fact_query_export_error(error)
                    })?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let StructFieldDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default_surface(
                    compilation,
                    graph,
                    export,
                    checked.value().provider().into(),
                    surface,
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
                    .map_err(|error| {
                        super::super::fact_query_export_error(error)
                    })?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let UnionPayloadDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default_surface(
                    compilation,
                    graph,
                    export,
                    checked.value().provider().into(),
                    surface,
                    semantics,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn export_runtime_default_surface(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    provider: AnySymbolId,
    surface: &impl RuntimeDefaultSurface,
    semantics: &mut ExportedDeclarations,
) -> Result<(), PackageInterfaceExportError> {
    export_runtime_default(
        compilation,
        graph,
        export,
        provider,
        surface.inputs(),
        surface.generic_context(),
        surface.template_reference(),
        surface.dependency_contract(),
        &mut semantics.checked_templates,
        &mut semantics.declaration_templates,
        &mut semantics.support_entities,
    )
}

trait RuntimeDefaultSurface {
    fn inputs(&self) -> &[RuntimeDefaultProviderInput];
    fn generic_context(&self) -> &RuntimeDefaultGenericContext;
    fn template_reference(&self) -> &RuntimeDefaultTemplateReference;
    fn dependency_contract(&self) -> bray_symbols::DependencyContractTemplateId;
}

macro_rules! impl_runtime_default_surface {
    ($($surface:ty),+ $(,)?) => {
        $(
            impl RuntimeDefaultSurface for $surface {
                fn inputs(&self) -> &[RuntimeDefaultProviderInput] {
                    self.inputs()
                }

                fn generic_context(&self) -> &RuntimeDefaultGenericContext {
                    self.generic_context()
                }

                fn template_reference(&self) -> &RuntimeDefaultTemplateReference {
                    self.template_reference()
                }

                fn dependency_contract(&self) -> bray_symbols::DependencyContractTemplateId {
                    self.behavior().dependency_contract()
                }
            }
        )+
    };
}

impl_runtime_default_surface!(
    bray_symbols::CallableParameterDefaultSurface,
    bray_symbols::StructFieldDefaultSurface,
    bray_symbols::UnionPayloadDefaultSurface,
);

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

pub(super) fn export_runtime_default(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    provider: AnySymbolId,
    contextual_inputs: &[RuntimeDefaultProviderInput],
    generic_context: &RuntimeDefaultGenericContext,
    template: &RuntimeDefaultTemplateReference,
    dependency: bray_symbols::DependencyContractTemplateId,
    checked_templates: &mut Vec<InterfaceCheckedTemplate>,
    declaration_templates: &mut Vec<InterfaceDeclarationTemplate>,
    support_entities: &mut Vec<InterfaceSupportEntity>,
) -> Result<(), PackageInterfaceExportError> {
    let RuntimeDefaultTemplateReference::Source(expression) = template else {
        return Err(incomplete(provider));
    };

    let key = compilation
        .source_runtime_default_key(provider, *expression)
        .map_err(super::super::fact_query_export_error)?;

    let mut inputs = runtime_default_inputs(compilation, graph, export, contextual_inputs)?;

    inputs.extend(generic_template_inputs(
        compilation,
        export,
        generic_context.parameters().iter().copied(),
    )?);

    let checked = export_checked_source_template(
        compilation,
        export,
        key,
        CheckedTemplateKind::RuntimeDefault,
        *expression,
        inputs,
        dependency,
    )?;

    push_declaration_template(
        export,
        provider,
        CheckedTemplateKind::RuntimeDefault,
        bray_symbols::SymbolOrdinal::new(0),
        checked,
        checked_templates,
        declaration_templates,
        support_entities,
    )
}
