use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundUnitKey;
use bray_package_interface::{
    InterfaceExecutableTemplate, InterfaceNativeBoundary, InterfaceRuntimeRequirement,
    InterfaceSemantics, InterfaceSymbolReference, PackageInterfaceSurface,
    commit_interface_semantic_fragments,
};
use bray_symbols::{AnySymbolId, ExternalSymbolKey, InterfaceSymbolId};

use super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;
use crate::fact::{BatchCompletionError, BatchWork, FactQueryError};

use super::context::SemanticExporter;
use super::defaults::target_dependencies;
use super::fragment::SemanticFragment;
use super::implementation::implementation_semantics;
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

    let fragments = resolve_fragments(
        compilation,
        graph,
        &binder,
        surface,
        selected,
        keys,
        values,
    )?;

    let mut export = SemanticExporter::new(compilation, graph, surface, keys, values);

    let committed_fragments = crate::profile::profile_operation(
        compilation.state.fact_runtime.profile(),
        crate::profile::ProfileOperation::InterfaceCommit,
        || {
            fragments
                .into_iter()
                .map(|fragment| fragment.commit(&mut export))
                .collect::<Result<Vec<_>, _>>()
        },
        crate::profile::result_outcome,
    )?;

    let (implementations, coherence) = implementation_semantics(&mut export, &binder, selected)?;

    let target_dependencies = target_dependencies(compilation, graph, selected, &mut export)?;

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
        .with_implementations(implementations, coherence)
        .with_target_dependencies(target_dependencies, [])
        .with_runtime_requirements(runtime_requirements);

    let semantics = crate::profile::profile_operation(
        compilation.state.fact_runtime.profile(),
        crate::profile::ProfileOperation::InterfaceCommit,
        || commit_interface_semantic_fragments(semantics, committed_fragments),
        crate::profile::result_outcome,
    )
    .map_err(PackageInterfaceExportError::FragmentCommit)?;

    Ok((semantics, executable_templates, native_boundaries))
}

fn resolve_fragments(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    binder: &crate::compilation::binder::CompilationBindingContext<'_>,
    surface: &PackageInterfaceSurface,
    selected: &BTreeSet<AnySymbolId>,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
    values: &bray_symbols::SemanticValueStore,
) -> Result<Vec<SemanticFragment>, PackageInterfaceExportError> {
    let fragments = compilation
        .state
        .fact_runtime
        .complete_batch(
            selected.iter().copied(),
            &compilation.state.cancellation,
            |symbol| {
                crate::profile::profile_operation(
                    compilation.state.fact_runtime.profile(),
                    crate::profile::ProfileOperation::InterfaceFragmentDiscovery,
                    || {
                        SemanticFragment::build(
                            compilation,
                            graph,
                            binder,
                            surface,
                            keys,
                            values,
                            *symbol,
                        )
                        .map(BatchWork::leaf)
                    },
                    crate::profile::result_outcome,
                )
            },
        )
        .map_err(fragment_batch_error)?;

    let mut fragments = fragments
        .into_iter()
        .map(|(_, fragment)| fragment)
        .collect::<Vec<_>>();

    fragments.sort_unstable_by(|left, right| left.identity().cmp(right.identity()));

    if let Some(pair) = fragments
        .windows(2)
        .find(|pair| pair[0].identity() == pair[1].identity())
    {
        let first = pair[0].exact_diagnostic_identity(graph)?;
        let second = pair[1].exact_diagnostic_identity(graph)?;
        let first_span = pair[0].diagnostic_span(graph);
        let second_span = pair[1].diagnostic_span(graph);

        // The error owns the stable identity after the temporary fragment batch is released.
        return Err(PackageInterfaceExportError::ConflictingSemanticFragment {
            first,
            second,
            first_span,
            second_span,
            identity: pair[0].identity().clone(),
        });
    }

    Ok(fragments)
}

fn fragment_batch_error(
    error: BatchCompletionError<AnySymbolId, PackageInterfaceExportError>,
) -> PackageInterfaceExportError {
    match error {
        BatchCompletionError::Cancelled => PackageInterfaceExportError::Cancelled,
        BatchCompletionError::Evaluation { error, .. } => error,
        BatchCompletionError::Scheduler(FactQueryError::Cancelled) => {
            PackageInterfaceExportError::Cancelled
        }
        BatchCompletionError::Scheduler(error) => {
            PackageInterfaceExportError::FragmentCoordination(error)
        }
    }
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
            export_executable_template_family(compilation, graph, owner, root, export)?;

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
                .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?
                .flatten()
        } else {
            None
        };

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
            .map(|template| template.with_platform_service(platform_service))
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

#[cfg(test)]
mod tests {
    use bray_symbols::AnySymbolId;

    use super::fragment_batch_error;
    use crate::compilation::PackageInterfaceExportError;
    use crate::fact::{
        BatchCompletionError, CompilationFactKey, FactCycle, FactQueryError,
    };

    #[test]
    fn scheduler_failures_retain_their_cause() {
        let cycle = FactCycle::new([
            CompilationFactKey::SyntaxTree,
            CompilationFactKey::DeclarationTable,
            CompilationFactKey::SyntaxTree,
        ]);

        let error = fragment_batch_error(BatchCompletionError::<
            AnySymbolId,
            PackageInterfaceExportError,
        >::Scheduler(FactQueryError::Cycle(cycle.clone())));

        assert_eq!(
            error,
            PackageInterfaceExportError::FragmentCoordination(FactQueryError::Cycle(cycle))
        );
    }
}
