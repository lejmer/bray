use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundUnitKey;
use bray_package_interface::{
    InterfaceExecutableTemplate, InterfaceNativeBoundary,
    InterfaceRuntimeRequirement, InterfaceSemantics, InterfaceSymbolReference, PackageInterfaceSurface,
};
use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, InterfaceSymbolId,
};

use super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;

use super::context::SemanticExporter;
use super::declarations::{
    ExportedDeclarations, export_callable_semantics, export_default_semantics,
    export_generic_semantics, export_predicate_semantics, export_type_semantics,
};
use super::defaults::target_dependencies;
use super::implementation::{
    export_constant_semantics, implementation_semantics,
};
use super::templates::ExecutableTemplateExporter;

pub(in crate::compilation::export) fn build_semantics(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    surface: &PackageInterfaceSurface,
    selected: &BTreeSet<AnySymbolId>,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<
    (
        InterfaceSemantics,
        Vec<InterfaceExecutableTemplate>,
        Vec<InterfaceNativeBoundary>,
    ),
    PackageInterfaceExportError,
> {
    let values = compilation
        .semantic_value_store()
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

    let binder = compilation
        .binding_context(&compilation.state.cancellation)
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

    let mut export = SemanticExporter::new(compilation, graph, surface, keys, values);
    let mut declarations = ExportedDeclarations::default();

    for symbol in selected.iter().copied() {
        export_callable_semantics(
            compilation,
            graph,
            &binder,
            symbol,
            &mut export,
            &mut declarations,
        )?;

        export_generic_semantics(compilation, &binder, symbol, &mut export, &mut declarations)?;
        export_constant_semantics(compilation, &binder, symbol, &mut export, &mut declarations)?;
        export_predicate_semantics(&binder, symbol, &export, &mut declarations)?;

        export_default_semantics(
            compilation,
            graph,
            &binder,
            symbol,
            &mut export,
            &mut declarations,
        )?;

        export_type_semantics(compilation, symbol, &mut export, &mut declarations)?;
    }

    let (implementations, coherence) = implementation_semantics(&mut export, &binder, selected)?;

    let target_dependencies = target_dependencies(compilation, graph, selected, &mut export)?;

    declarations.sort_canonical();

    let (executable_templates, runtime_requirements) =
        executable_templates(compilation, graph, selected, &mut export)?;

    let native_boundaries = native_boundaries(compilation, selected, &export)?;

    let semantics = InterfaceSemantics::new()
        .with_applications(
            export.substitutions,
            export.trait_applications,
            export.callable_instances,
            export.implementation_instances,
        )
        .with_values(
            export.dependency_contracts,
            export.types,
            export.constant_values,
            export.constant_terms,
        )
        .with_contracts(declarations.constraints, declarations.callable_contracts)
        .with_declarations(
            declarations.signatures,
            declarations.generic_declarations,
            declarations.parameter_defaults,
            declarations.predicate_definitions,
        )
        .with_declared_types(declarations.declared_types)
        .with_type_representations(declarations.type_representations)
        .with_templates(
            declarations.checked_templates,
            declarations.declaration_templates,
            declarations.support_entities,
        )
        .with_implementations(implementations, coherence)
        .with_target_dependencies(target_dependencies, [])
        .with_runtime_requirements(runtime_requirements);

    Ok((semantics, executable_templates, native_boundaries))
}
fn native_boundaries(
    compilation: &Compilation,
    selected: &BTreeSet<AnySymbolId>,
    export: &SemanticExporter<'_>,
) -> Result<Vec<InterfaceNativeBoundary>, PackageInterfaceExportError> {
    let mut boundaries = Vec::new();

    for symbol in selected.iter().copied() {
        let (direction, kind, native_symbol) = match symbol {
            AnySymbolId::Function(function) => {
                let contract = compilation
                    .foreign_callable_contract(function)
                    .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

                if contract.diagnostics().has_errors() {
                    return Err(PackageInterfaceExportError::InvalidCompilation);
                }

                let Some(contract) = contract.value() else {
                    continue;
                };

                (
                    contract.direction(),
                    bray_package_interface::InterfaceNativeBoundaryKind::Callable,
                    contract.symbol().clone(),
                )
            }
            AnySymbolId::Static(declaration) => {
                let contract = compilation
                    .foreign_static_contract(declaration)
                    .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

                if contract.diagnostics().has_errors() {
                    return Err(PackageInterfaceExportError::InvalidCompilation);
                }

                let Some(contract) = contract.value() else {
                    continue;
                };

                let template = compilation
                    .static_instance_template(declaration)
                    .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

                (
                    contract.direction(),
                    bray_package_interface::InterfaceNativeBoundaryKind::Static {
                        duration: template.value().duration(),
                        mutable: contract.is_mutable(),
                    },
                    contract.symbol().clone(),
                )
            }
            _ => continue,
        };

        let InterfaceSymbolReference::Local(owner) = export.symbol_reference(symbol)? else {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        };

        boundaries.push(InterfaceNativeBoundary::new(
            owner,
            direction,
            kind,
            native_symbol,
        ));
    }

    Ok(boundaries)
}

fn executable_templates(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    selected: &BTreeSet<AnySymbolId>,
    export: &mut SemanticExporter<'_>,
) -> Result<
    (
        Vec<InterfaceExecutableTemplate>,
        Vec<InterfaceRuntimeRequirement>,
    ),
    PackageInterfaceExportError,
> {
    let mut templates = Vec::new();
    let mut runtime_requirements = Vec::new();

    for symbol in selected.iter().copied() {
        let Some(root) = executable_template_unit(compilation, graph, symbol)? else {
            continue;
        };

        let InterfaceSymbolReference::Local(owner) = export.symbol_reference(symbol)? else {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        };

        let (family_templates, family_requirement) =
            export_executable_template_family(compilation, owner, root, export)?;

        templates.extend(family_templates);

        if let Some(requirement) = family_requirement {
            runtime_requirements.push(requirement);
        }
    }

    runtime_requirements.sort_unstable();

    Ok((templates, runtime_requirements))
}

fn export_executable_template_family(
    compilation: &Compilation,
    owner: InterfaceSymbolId,
    root: BoundUnitKey,
    export: &mut SemanticExporter<'_>,
) -> Result<
    (
        Vec<InterfaceExecutableTemplate>,
        Option<InterfaceRuntimeRequirement>,
    ),
    PackageInterfaceExportError,
> {
    let family = executable_template_family(compilation, root)?;

    let family_size =
        u32::try_from(family.len()).map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

    let identities = family
        .iter()
        .enumerate()
        .map(|(index, key)| {
            u32::try_from(index)
                .map(bray_ir::MirExecutableTemplateId::new)
                // The address map owns stable source keys independently of the traversal list.
                .map(|identity| (key.clone(), identity))
                .map_err(|_| PackageInterfaceExportError::InvalidCompilation)
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let selected_target = compilation.selected_target().target();

    let codegen_target = selected_target
        .codegen_target()
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

    let mut templates = Vec::with_capacity(family.len());
    let mut family_requirements = Vec::new();
    let mut frames = BTreeSet::new();

    for key in family {
        let identity = identities
            .get(&key)
            .copied()
            .ok_or(PackageInterfaceExportError::InvalidCompilation)?;

        let lowered = compilation
            .lowered_unit(key)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        if lowered.diagnostics().has_errors() {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let mir = lowered
            .value()
            .as_ref()
            .and_then(bray_lowering::LoweredUnit::mir)
            .ok_or(PackageInterfaceExportError::InvalidCompilation)?;

        let capabilities = crate::compilation::product::demanded_runtime_capabilities(mir);

        if !capabilities.is_empty() || mir.frame_descriptor().is_some() {
            let frame = mir.frame_descriptor();

            let requirements = bray_runtime_interface::RuntimeRequirements::new(
                None,
                frame.map_or(
                    selected_target.runtime_abi(),
                    bray_ir::MirFrameDescriptor::abi_version,
                ),
                frame.map(bray_ir::MirFrameDescriptor::frame_abi),
                codegen_target.identity().clone(),
                codegen_target.panic_abi().clone(),
                [],
                capabilities,
                [],
            );

            family_requirements.push(requirements);

            if let Some(frame) = frame {
                frames.insert(frame.frame());
            }
        }

        let mut context = ExecutableTemplateExporter::new(export, &identities);

        let payload = bray_package_interface::encode_executable_template(mir, &mut context)
            .map_err(|error| match error {
                bray_package_interface::ExecutableTemplateEncodeError::Semantic(error) => error,
                bray_package_interface::ExecutableTemplateEncodeError::InvalidUnitKind => {
                    PackageInterfaceExportError::InvalidCompilation
                }
            })?;

        let template = InterfaceExecutableTemplate::new(owner, identity, family_size, payload)
            .ok_or(PackageInterfaceExportError::InvalidCompilation)?;

        templates.push(template);
    }

    let runtime_requirement =
        bray_runtime_interface::RuntimeRequirements::try_merge(family_requirements)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?
            .map(|requirements| {
                InterfaceRuntimeRequirement::new(
                    InterfaceSymbolReference::Local(owner),
                    frames,
                    requirements,
                )
            });

    Ok((templates, runtime_requirement))
}

fn executable_template_family(
    compilation: &Compilation,
    root: BoundUnitKey,
) -> Result<Vec<BoundUnitKey>, PackageInterfaceExportError> {
    compilation
        .bound_unit_family_with_cancellation(root, &compilation.state.cancellation)
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?
        .into_iter()
        .map(|bound| {
            if bound.diagnostics().has_errors() {
                return Err(PackageInterfaceExportError::InvalidCompilation);
            }

            // Export traversal retains each stable key beyond the immutable bound-semantics borrow.
            Ok(bound.value().key().clone())
        })
        .collect()
}

fn executable_template_unit(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
) -> Result<Option<BoundUnitKey>, PackageInterfaceExportError> {
    if let AnySymbolId::Static(static_declaration) = owner {
        return compilation
            .static_initializer_key(static_declaration)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation);
    }

    if let Some(definition) = bray_symbols::CallableDefinitionId::try_new(owner) {
        return compilation
            .callable_body_key(definition)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation);
    }

    if graph.runtime_default_subject(owner).is_none() {
        return Ok(None);
    }

    let owner = graph
        .symbol_key(owner)
        .ok_or(PackageInterfaceExportError::InvalidCompilation)?;

    compilation
        .declared_unit_keys()
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)
        .map(|units| {
            units.into_iter().find(|unit| {
                unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault
                    && unit.declared_owner() == owner
            })
        })
}
