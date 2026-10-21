use std::collections::BTreeSet;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{CheckedTemplateKind};
use bray_package_interface::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateId, InterfaceDeclarationTemplate,
    InterfaceSupportEntity, InterfaceTargetPropertyDependency,
};
use bray_symbols::{
    AnySymbolId, GenericDeclarationTemplateQuery, GenericOwnerId, GenericParameterSymbolId,
    InterfaceSupportEntityId, StaticStorageDuration,
    SymbolQueryRequest,
};
use bray_target::TargetPropertyKind;

use super::super::PackageInterfaceExportError;
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
