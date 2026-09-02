use std::collections::BTreeSet;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundReferenceTarget, CheckedTemplateKind};
use bray_package_interface::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateId, InterfaceCheckedTemplateInputKind,
    InterfaceDeclarationTemplate, InterfaceSupportEntity, InterfaceTargetPropertyDependency,
};
use bray_symbols::{
    AnySymbolId, GenericDeclarationTemplateQuery, GenericOwnerId, GenericParameterSymbolId,
    InterfaceSupportEntityId, RuntimeDefaultProviderInput, StaticStorageDuration,
    SymbolQueryRequest, TypeData, TypeId,
};
use bray_target::TargetPropertyKind;

use super::super::PackageInterfaceExportError;
use super::super::template::SourceTemplateInput;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;
use crate::compilation::source_graph::{
    source_declaration_module_parts, source_symbol_contribution_gate,
};

use super::context::SemanticExporter;
use super::templates::{incomplete, index};

pub(super) fn target_dependencies(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    selected: &BTreeSet<AnySymbolId>,
    export: &mut SemanticExporter<'_>,
) -> Result<Vec<InterfaceTargetPropertyDependency>, PackageInterfaceExportError> {
    let source_graph = compilation
        .product_source_graph()
        .map_err(super::super::invalid_compilation_fact_error)?;

    let module_parts = source_declaration_module_parts(source_graph.declarations());
    let mut dependencies = Vec::new();

    for owner in selected.iter().copied() {
        if let Some(gate) =
            source_symbol_contribution_gate(source_graph, graph, &module_parts, owner)
        {
            for dependency in gate.dependencies() {
                dependencies.push(InterfaceTargetPropertyDependency::new(
                    export.symbol_reference(owner)?,
                    export.symbol_reference(dependency.property().into())?,
                    export.constant_value_id(dependency.value())?,
                ));
            }
        }

        let AnySymbolId::Static(declaration) = owner else {
            continue;
        };

        let template = compilation
            .static_instance_template(declaration)
            .map_err(super::super::invalid_compilation_fact_error)?;

        if template.diagnostics().has_errors()
            || template.value().duration() != StaticStorageDuration::ExactThread
        {
            continue;
        }

        let property = TargetPropertyKind::PlatformNativeThreads;

        let property_symbol = compilation
            .available_compiler_known_symbols()
            .provider()
            .target_property_symbol(property)
            .ok_or_else(|| {
                super::super::export_contract_error(
                    super::super::PackageInterfaceExportContract::MissingCompilerKnownTargetProperty,
                )
            })?;

        let value = compilation
            .target_property_value(property)
            .map_err(super::super::invalid_compilation_fact_error)?;

        dependencies.push(InterfaceTargetPropertyDependency::new(
            export.symbol_reference(owner)?,
            export.symbol_reference(property_symbol.into())?,
            export.constant_value_id(value)?,
        ));
    }

    dependencies.sort_unstable();
    dependencies.dedup();

    Ok(dependencies)
}

pub(super) fn runtime_default_inputs(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    inputs: &[RuntimeDefaultProviderInput],
) -> Result<Vec<SourceTemplateInput>, PackageInterfaceExportError> {
    inputs
        .iter()
        .copied()
        .map(|input| {
            let (kind, symbol) = match input {
                RuntimeDefaultProviderInput::Receiver(receiver) => (
                    InterfaceCheckedTemplateInputKind::Receiver,
                    AnySymbolId::ReceiverParameter(receiver),
                ),
                RuntimeDefaultProviderInput::EarlierParameter(parameter) => {
                    let ordinal = graph
                        .callable_parameters()
                        .iter()
                        .find(|candidate| candidate.id() == parameter)
                        .map(|parameter| parameter.ordinal())
                        .ok_or_else(|| incomplete(parameter.into()))?;

                    (
                        InterfaceCheckedTemplateInputKind::Parameter(
                            bray_symbols::SymbolOrdinal::new(ordinal),
                        ),
                        AnySymbolId::CallableParameter(parameter),
                    )
                }
            };

            Ok(SourceTemplateInput::new(
                kind,
                Some(BoundReferenceTarget::Surface(symbol)),
                runtime_default_input_type(compilation, graph, export, symbol)?,
            ))
        })
        .collect()
}

pub(super) fn runtime_default_input_type(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    symbol: AnySymbolId,
) -> Result<TypeId, PackageInterfaceExportError> {
    let (owner, parameter) = match symbol {
        AnySymbolId::ReceiverParameter(receiver) => {
            let receiver = graph
                .receiver_parameter(receiver)
                .ok_or_else(|| incomplete(symbol))?;

            (receiver.owner(), None)
        }
        AnySymbolId::CallableParameter(parameter) => {
            let parameter = graph
                .callable_parameter(parameter)
                .ok_or_else(|| incomplete(symbol))?;

            (
                parameter.owner(),
                Some((parameter.id(), parameter.ordinal())),
            )
        }
        _ => return Err(incomplete(symbol)),
    };

    let signature = compilation
        .callable_signature_template(owner.into_any())
        .map_err(super::super::fact_query_export_error)?
        .ok_or_else(|| incomplete(symbol))?;

    if signature.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    match parameter {
        Some((parameter, ordinal)) => {
            let ty = signature
                .value()
                .parameter_type_template(parameter, ordinal, export.values)
                .map_err(|error| {
                    super::super::callable_signature_export_error(error)
                })?;

            export.resolve_type_template(symbol, &ty)
        }
        None => signature
            .value()
            .receiver()
            .map(|receiver| receiver.ty())
            .ok_or_else(|| incomplete(symbol)),
    }
}

pub(super) fn callable_template_inputs(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    signature: &bray_symbols::CallableSignatureTemplate,
    generic_parameters: Vec<GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, PackageInterfaceExportError> {
    let mut inputs = Vec::new();

    if let Some(receiver) = signature.receiver() {
        inputs.push(SourceTemplateInput::new(
            InterfaceCheckedTemplateInputKind::Receiver,
            Some(BoundReferenceTarget::Surface(
                AnySymbolId::ReceiverParameter(receiver.parameter()),
            )),
            receiver.ty(),
        ));
    }

    let parameter_types = signature
        .parameter_type_templates(export.values)
        .map_err(|error| {
            super::super::callable_signature_export_error(error)
        })?;

    for (index, (parameter, ty)) in signature
        .parameters()
        .iter()
        .copied()
        .zip(parameter_types.iter())
        .enumerate()
    {
        inputs.push(SourceTemplateInput::new(
            InterfaceCheckedTemplateInputKind::Parameter(bray_symbols::SymbolOrdinal::new(
                u32::try_from(index).map_err(|_| {
                    super::super::capacity_export_error("default_parameter_ordinal", index)
                })?,
            )),
            Some(BoundReferenceTarget::Surface(
                AnySymbolId::CallableParameter(parameter),
            )),
            export.resolve_type_template(parameter.into(), ty)?,
        ));
    }

    inputs.extend(generic_template_inputs(
        compilation,
        export,
        generic_parameters,
    )?);

    Ok(inputs)
}

pub(super) fn generic_template_inputs(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, PackageInterfaceExportError> {
    parameters
        .into_iter()
        .map(|parameter| match parameter {
            GenericParameterSymbolId::Type(parameter) => {
                let symbol = AnySymbolId::GenericTypeParameter(parameter);

                let ty = export
                    .values
                    .intern_type(TypeData::TypeParameter(parameter))
                    .map_err(super::super::semantic_value_export_error)?;

                Ok(SourceTemplateInput::new(
                    InterfaceCheckedTemplateInputKind::GenericType(
                        export.symbol_reference(symbol)?,
                    ),
                    None,
                    ty,
                ))
            }
            GenericParameterSymbolId::Const(parameter) => {
                let symbol = AnySymbolId::GenericConstParameter(parameter);

                Ok(SourceTemplateInput::new(
                    InterfaceCheckedTemplateInputKind::GenericConstant(
                        export.symbol_reference(symbol)?,
                    ),
                    Some(BoundReferenceTarget::Surface(symbol)),
                    symbol_type(compilation, export, symbol)?,
                ))
            }
        })
        .collect()
}

pub(super) fn symbol_type(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    symbol: AnySymbolId,
) -> Result<TypeId, PackageInterfaceExportError> {
    if let Some(ty) = callable_input_type(compilation, export, symbol)? {
        return Ok(ty);
    }

    let ty = compilation
        .symbol_type_template(symbol)
        .map_err(super::super::fact_query_export_error)?
        .ok_or_else(|| incomplete(symbol))?;

    if ty.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    export.resolve_type_template(symbol, ty.value())
}

pub(super) fn callable_input_type(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    symbol: AnySymbolId,
) -> Result<Option<TypeId>, PackageInterfaceExportError> {
    let (owner, parameter_ordinal) = match symbol {
        AnySymbolId::CallableParameter(parameter) => {
            let parameter = export
                .graph
                .callable_parameter(parameter)
                .ok_or_else(|| incomplete(symbol))?;

            (parameter.owner(), Some(parameter.ordinal()))
        }
        AnySymbolId::ReceiverParameter(receiver) => {
            let receiver = export
                .graph
                .receiver_parameter(receiver)
                .ok_or_else(|| incomplete(symbol))?;

            (receiver.owner(), None)
        }
        _ => return Ok(None),
    };

    let signature = compilation
        .callable_signature_template(owner.into_any())
        .map_err(super::super::fact_query_export_error)?
        .ok_or_else(|| incomplete(symbol))?;

    if signature.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let Some(ordinal) = parameter_ordinal else {
        return signature
            .value()
            .receiver()
            .map(|receiver| Some(receiver.ty()))
            .ok_or_else(|| incomplete(symbol));
    };

    let parameter_types = signature
        .value()
        .parameter_type_templates(export.values)
        .map_err(|error| {
            super::super::callable_signature_export_error(error)
        })?;

    let template = parameter_types
        .get(usize::try_from(ordinal).map_err(|_| {
            super::super::capacity_export_error("default_parameter_reference", ordinal)
        })?)
        .ok_or_else(|| incomplete(symbol))?
        .clone();

    export.resolve_type_template(symbol, &template).map(Some)
}

pub(super) fn generic_parameters(
    binder: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
) -> Result<Vec<GenericParameterSymbolId>, PackageInterfaceExportError> {
    let Some(owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(Vec::new());
    };

    let generic = binder
        .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
            owner,
        ))
        .map_err(super::super::binding_query_export_error)?;

    if generic.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    Ok(generic.value().parameters().to_vec())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the helper commits one checked template and its correlated interface records"
)]
pub(super) fn push_declaration_template(
    export: &mut SemanticExporter<'_>,
    owner: AnySymbolId,
    kind: CheckedTemplateKind,
    ordinal: bray_symbols::SymbolOrdinal,
    checked: InterfaceCheckedTemplate,
    checked_templates: &mut Vec<InterfaceCheckedTemplate>,
    declaration_templates: &mut Vec<InterfaceDeclarationTemplate>,
    support_entities: &mut Vec<InterfaceSupportEntity>,
) -> Result<(), PackageInterfaceExportError> {
    let checked_id = InterfaceCheckedTemplateId::new(index(checked_templates.len())?);
    let entity = InterfaceSupportEntityId::new(index(support_entities.len())?);

    checked_templates.push(checked);
    support_entities.push(InterfaceSupportEntity::CheckedTemplate(checked_id));

    declaration_templates.push(InterfaceDeclarationTemplate::new(
        export.symbol_reference(owner)?,
        kind,
        ordinal,
        entity,
    ));

    Ok(())
}
