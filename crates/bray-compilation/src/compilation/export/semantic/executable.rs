use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundUnitKey;
use bray_package_interface::{
    InterfaceExecutableTemplate, InterfaceRuntimeRequirement, InterfaceSymbolReference,
};

use bray_symbols::{AnySymbolId, CallableDefinitionId, InterfaceSymbolId, SymbolGraph};

use super::super::PackageInterfaceExportError;
use super::build::{exported_declaration_identity, semantic_batch_error};
use super::context::SemanticExporter;
use super::templates::ExecutableTemplateExporter;
use crate::compilation::Compilation;
use crate::fact::BatchWork;

pub(super) fn prepare_executable_outputs(
    compilation: &Compilation,
    graph: &SymbolGraph,
    selected: &BTreeSet<AnySymbolId>,
    export: &SemanticExporter<'_>,
) -> Result<(), PackageInterfaceExportError> {
    let roots = selected
        .iter()
        .copied()
        .map(|symbol| {
            let declaration = exported_declaration_identity(export, symbol)?;

            executable_template_unit(compilation, graph, symbol, &declaration)
        })
        .collect::<Result<Vec<_>, _>>()?;

    compilation
        .state
        .fact_runtime
        .complete_batch(
            roots.into_iter().flatten(),
            &compilation.state.cancellation,
            |key| {
                let output = compilation
                    .source_output_unit(key.clone(), &compilation.state.cancellation)
                    .map_err(super::super::invalid_compilation_fact_error)?;

                if output.has_errors() {
                    // rust-style: allow(context-erasing-failure-conversion, reason = "completed source output retains exact diagnostic causes")
                    return Err(PackageInterfaceExportError::InvalidCompilation);
                }

                // The worklist owns only shared unit keys after each source projection completes.
                let nested: Vec<_> = output
                    .nested
                    .iter()
                    .map(|(_, _, _, key)| key.clone())
                    .collect();

                Ok::<_, PackageInterfaceExportError>(BatchWork::new((), nested))
            },
        )
        .map_err(semantic_batch_error)?;

    Ok(())
}

pub(super) fn executable_templates(
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
        let declaration = exported_declaration_identity(export, symbol)?;

        let Some(root) = executable_template_unit(compilation, graph, symbol, &declaration)? else {
            continue;
        };

        let InterfaceSymbolReference::Local(owner) = export.symbol_reference(symbol)? else {
            return Err(super::super::export_contract_error(
                super::super::PackageInterfaceExportContract::NonLocalExecutableTemplate,
            ));
        };

        let (family_templates, family_requirement) = export_executable_template_family(
            compilation,
            graph,
            owner,
            &declaration,
            root,
            export,
        )?;

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
    graph: &bray_symbols::SymbolGraph,
    owner: InterfaceSymbolId,
    declaration: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
    root: BoundUnitKey,
    export: &mut SemanticExporter<'_>,
) -> Result<
    (
        Vec<InterfaceExecutableTemplate>,
        Option<InterfaceRuntimeRequirement>,
    ),
    PackageInterfaceExportError,
> {
    let family = executable_template_family(compilation, root, declaration)?;

    let family_size = u32::try_from(family.len()).map_err(|_| {
        PackageInterfaceExportError::InvalidCompilationCause(
            super::super::PackageInterfaceInvalidCompilationCause::Capacity {
                field: "executable_template_family_size",
                actual: family.len().to_string(),
            },
        )
    })?;

    let identities = family
        .iter()
        .enumerate()
        .map(|(index, key)| {
            u32::try_from(index)
                .map(bray_ir::MirExecutableTemplateId::new)
                // The address map owns stable source keys independently of the traversal list.
                .map(|identity| (key.clone(), identity))
                .map_err(|_| {
                    PackageInterfaceExportError::InvalidCompilationCause(
                        super::super::PackageInterfaceInvalidCompilationCause::Capacity {
                            field: "executable_template_identity",
                            actual: index.to_string(),
                        },
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let selected_target = compilation.selected_target().target();

    let codegen_target = selected_target.codegen_target().map_err(|cause| {
        PackageInterfaceExportError::InvalidCompilationCause(
            super::super::PackageInterfaceInvalidCompilationCause::CodegenTarget(cause),
        )
    })?;

    let mut templates = Vec::with_capacity(family.len());
    let mut family_requirements = Vec::new();
    let mut frames = BTreeSet::new();
    let interface = export.surface.identity();

    let source_namespace = bray_symbols::ProductIdentity::try_new(
        interface.package().clone(),
        interface.product().as_str(),
    )
    .expect("validated interface product identity must be nonempty")
    .source_namespace();

    for key in family {
        let identity = identities
            .get(&key)
            .copied()
            .ok_or_else(|| invalid_executable_template("missing_family_identity"))?;

        let platform_service = if identity == bray_ir::MirExecutableTemplateId::ROOT {
            graph
                .symbol_for_key(key.declared_owner())
                .and_then(|symbol| match symbol {
                    AnySymbolId::Function(function) => Some(function),
                    _ => None,
                })
                .map(|function| {
                    crate::compilation::foreign::platform::platform_service_role(
                        compilation,
                        function,
                    )
                })
                .transpose()
                .map_err(|cause| {
                    super::super::executable_template_evaluation_export_error(
                        declaration.clone(),
                        cause,
                    )
                })?
                .flatten()
        } else {
            None
        };

        let lowered = compilation.lowered_unit(key).map_err(|cause| {
            super::super::executable_template_evaluation_export_error(declaration.clone(), cause)
        })?;

        if lowered.diagnostics().has_errors() {
            // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let mir = lowered
            .value()
            .as_ref()
            .and_then(bray_lowering::LoweredUnit::mir)
            .ok_or_else(|| invalid_executable_template("missing_lowered_mir"))?;

        if let Some(frame) = mir.frame_descriptor() {
            let requirements = bray_runtime_interface::RuntimeRequirements::new(
                None,
                frame.abi_version(),
                Some(frame.frame_abi()),
                codegen_target.identity().clone(),
                codegen_target.panic_abi().clone(),
                [],
                [],
                [],
            );

            family_requirements.push(requirements);
            frames.insert(frame.frame());
        }

        let mut context = ExecutableTemplateExporter::new(export, &identities);

        let payload =
            bray_package_interface::encode_executable_template(mir, source_namespace, &mut context)
                .map_err(|error| match error {
                    bray_package_interface::ExecutableTemplateEncodeError::Semantic(error) => error,
                    bray_package_interface::ExecutableTemplateEncodeError::InvalidUnitKind => {
                        invalid_executable_template("invalid_mir_unit_kind")
                    }
                })?;

        let template = InterfaceExecutableTemplate::new(owner, identity, family_size, payload)
            .map(|template| template.with_platform_service(platform_service))
            .ok_or_else(|| invalid_executable_template("invalid_template_identity"))?;

        templates.push(template);
    }

    let runtime_requirement =
        bray_runtime_interface::RuntimeRequirements::try_merge(family_requirements)
            .map_err(|cause| {
                PackageInterfaceExportError::InvalidCompilationCause(
                    super::super::PackageInterfaceInvalidCompilationCause::RuntimeRequirements(
                        cause,
                    ),
                )
            })?
            .map(|requirements| {
                InterfaceRuntimeRequirement::new(
                    InterfaceSymbolReference::Local(owner),
                    frames,
                    requirements,
                )
            });

    Ok((templates, runtime_requirement))
}

fn invalid_executable_template(reason: &'static str) -> PackageInterfaceExportError {
    PackageInterfaceExportError::InvalidCompilationCause(
        super::super::PackageInterfaceInvalidCompilationCause::ExecutableTemplate { reason },
    )
}

fn executable_template_family(
    compilation: &Compilation,
    root: BoundUnitKey,
    declaration: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
) -> Result<Vec<BoundUnitKey>, PackageInterfaceExportError> {
    let mut family = Vec::new();
    let mut pending = vec![root];

    while let Some(key) = pending.pop() {
        let output = compilation
            .source_output_unit(key.clone(), &compilation.state.cancellation)
            .map_err(|cause| {
                super::super::executable_template_evaluation_export_error(
                    declaration.clone(),
                    cause,
                )
            })?;

        if output.has_errors() {
            // rust-style: allow(context-erasing-failure-conversion, reason = "completed source output retains exact diagnostic causes")
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        // Preserve the bound family's deterministic preorder without rebinding released units.
        pending.extend(output.nested.iter().rev().map(|(_, _, _, key)| key.clone()));
        family.push(key);
    }

    Ok(family)
}

fn executable_template_unit(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    owner: AnySymbolId,
    declaration: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
) -> Result<Option<BoundUnitKey>, PackageInterfaceExportError> {
    if let AnySymbolId::Static(static_declaration) = owner {
        return compilation
            .static_initializer_key(static_declaration)
            .map_err(|cause| {
                super::super::executable_template_evaluation_export_error(
                    declaration.clone(),
                    cause,
                )
            });
    }

    if let Some(definition) = CallableDefinitionId::try_new(owner) {
        return compilation.callable_body_key(definition).map_err(|cause| {
            super::super::executable_template_evaluation_export_error(declaration.clone(), cause)
        });
    }

    if graph.runtime_default_subject(owner).is_none() {
        return Ok(None);
    }

    graph.symbol_key(owner).ok_or_else(|| {
        super::super::export_contract_error(
            super::super::PackageInterfaceExportContract::MissingRuntimeDefaultOwnerKey,
        )
    })?;

    compilation
        .declared_unit_key(owner, bray_bound_tree::BoundUnitKind::RuntimeDefault)
        .map_err(|cause| {
            super::super::executable_template_evaluation_export_error(declaration.clone(), cause)
        })
}
