use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_package_interface::{
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBoundary,
    InterfaceRuntimeRequirement, InterfaceSemantics, InterfaceSymbolReference,
    PackageInterfaceSurface, commit_interface_semantic_fragments,
};
use bray_symbols::{
    AnySymbolId, CallableConstness, CallableDefinitionId, CallableInstanceData,
    CallableSignatureQuery, ExternalSymbolKey, GenericOwnerId, InterfaceSymbolId,
    SymbolQueryRequest, diagnostic_external_symbol_identity,
};

use super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;
use crate::fact::{BatchCompletionError, BatchWork, FactQueryError};

use super::context::{CheckedConstantExpression, SemanticExporter};
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
        Vec<InterfaceConstantCallableBody>,
        Vec<InterfaceExecutableTemplate>,
        Vec<InterfaceNativeBoundary>,
    ),
    PackageInterfaceExportError,
> {
    let values = compilation
        .semantic_value_store()
        .map_err(super::super::invalid_compilation_fact_error)?;

    let binder = compilation
        .binding_context(&compilation.state.cancellation)
        .map_err(super::super::invalid_compilation_fact_error)?;

    let fragments =
        resolve_fragments(compilation, graph, &binder, surface, selected, keys, values)?;

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

    let constant_callable_bodies = crate::profile::profile_operation(
        compilation.state.fact_runtime.profile(),
        crate::profile::ProfileOperation::InterfaceConstantBodies,
        || constant_callable_bodies(compilation, &binder, selected, &mut export),
        crate::profile::result_outcome,
    )?;

    let (executable_templates, runtime_requirements) = crate::profile::profile_operation(
        compilation.state.fact_runtime.profile(),
        crate::profile::ProfileOperation::InterfaceExecutableTemplates,
        || executable_templates(compilation, graph, selected, &mut export),
        crate::profile::result_outcome,
    )?;

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

    Ok((
        semantics,
        constant_callable_bodies,
        executable_templates,
        native_boundaries,
    ))
}

fn constant_callable_bodies(
    compilation: &Compilation,
    binder: &crate::compilation::binder::CompilationBindingContext<'_>,
    selected: &BTreeSet<AnySymbolId>,
    export: &mut SemanticExporter<'_>,
) -> Result<Vec<InterfaceConstantCallableBody>, PackageInterfaceExportError> {
    let values = export.values;
    let mut bodies = Vec::new();

    for symbol in selected.iter().copied() {
        let Some(definition) = CallableDefinitionId::try_new(symbol) else {
            continue;
        };

        if compilation
            .callable_body_key(definition)
            .map_err(super::super::invalid_compilation_fact_error)?
            .is_none()
        {
            continue;
        }

        let signature = binder
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(super::super::invalid_compilation_binding_error)?;

        if signature.diagnostics().has_errors()
            || signature
                .value()
                .constness(values)
                .map_err(|error| super::super::callable_signature_export_error(error))?
                != CallableConstness::Constant
        {
            continue;
        }

        let arguments = callable_argument_ordinals(signature.value())?;

        let parameters =
            crate::compilation::binder::visible_generic_parameters(export.graph, symbol);

        let owner = GenericOwnerId::try_new(symbol).ok_or_else(|| {
            super::super::export_contract_error(
                super::super::PackageInterfaceExportContract::MissingGenericOwner,
            )
        })?;

        let substitution =
            crate::compilation::substitution::identity_substitution(values, owner, &parameters)
                .map_err(super::super::invalid_compilation_fact_error)?;

        let result_type = export.resolve_type_template(symbol, signature.value().result())?;

        let result_type = if let Some(context @ bray_symbols::SelfTypeContext::NamedType(_)) =
            export.graph.contextual_self_scope(symbol)
        {
            let replacement =
                crate::compilation::substitution::contextual_self_type(binder, context)
                    .map_err(super::super::invalid_compilation_fact_error)?;

            values
                .substitute_contextual_self(result_type, context, replacement)
                .map_err(super::super::semantic_value_export_error)?
        } else {
            result_type
        };

        let result_type = values
            .substitute_type(result_type, substitution)
            .map_err(super::super::semantic_value_export_error)?;

        let declaration = exported_declaration_identity(export, symbol)?;

        let evaluated = compilation
            .symbolic_constant_callable_body(
                CallableInstanceData::new(definition, substitution),
                result_type,
                &arguments,
                &compilation.state.cancellation,
            )
            .map_err(|cause| {
                super::super::constant_callable_evaluation_export_error(declaration.clone(), cause)
            })?;

        if evaluated.diagnostics().has_errors() {
            continue;
        }

        let dependency = values
            .empty_dependency_contract_template()
            .map_err(super::super::semantic_value_export_error)?;

        let template = export.checked_constant_template(
            CheckedTemplateKind::ConstantCallableBody,
            CheckedConstantExpression {
                term: *evaluated.value(),
                ty: result_type,
            },
            dependency,
        )?;

        let InterfaceSymbolReference::Local(owner) = export.symbol_reference(symbol)? else {
            return Err(super::super::export_contract_error(
                super::super::PackageInterfaceExportContract::NonLocalConstantCallable,
            ));
        };

        bodies.push(InterfaceConstantCallableBody::new(owner, template));
    }

    Ok(bodies)
}

fn callable_argument_ordinals(
    signature: &bray_symbols::CallableSignatureTemplate,
) -> Result<BTreeMap<AnySymbolId, bray_symbols::SymbolOrdinal>, PackageInterfaceExportError> {
    let parameters = signature
        .receiver()
        .map(|receiver| AnySymbolId::from(receiver.parameter()))
        .into_iter()
        .chain(
            signature
                .parameters()
                .iter()
                .map(|parameter| AnySymbolId::from(*parameter)),
        );

    parameters
        .enumerate()
        .map(|(index, parameter)| {
            let ordinal = u32::try_from(index)
                .map(bray_symbols::SymbolOrdinal::new)
                .map_err(|_| {
                    PackageInterfaceExportError::InvalidCompilationCause(
                        super::super::PackageInterfaceInvalidCompilationCause::Capacity {
                            field: "callable_parameter_ordinal",
                            actual: index.to_string(),
                        },
                    )
                })?;

            Ok((parameter, ordinal))
        })
        .collect()
}

fn exported_declaration_identity(
    export: &SemanticExporter<'_>,
    symbol: AnySymbolId,
) -> Result<bray_diagnostics::DiagnosticInterfaceSymbolIdentity, PackageInterfaceExportError> {
    export
        .keys
        .get(&symbol)
        .map(diagnostic_external_symbol_identity)
        .ok_or_else(|| {
            super::super::export_contract_error(
                super::super::PackageInterfaceExportContract::MissingExportedDeclaration,
            )
        })
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
            super::super::fragment_coordination_export_error(error)
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
                    .map_err(super::super::invalid_compilation_fact_error)?;

                if contract.diagnostics().has_errors() {
                    // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
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
                    .map_err(super::super::invalid_compilation_fact_error)?;

                if contract.diagnostics().has_errors() {
                    // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
                    return Err(PackageInterfaceExportError::InvalidCompilation);
                }

                let Some(contract) = contract.value() else {
                    continue;
                };

                let template = compilation
                    .static_instance_template(declaration)
                    .map_err(super::super::invalid_compilation_fact_error)?;

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
            return Err(super::super::export_contract_error(
                super::super::PackageInterfaceExportContract::NonLocalNativeBoundary,
            ));
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

        let payload = crate::profile::profile_operation(
            compilation.state.fact_runtime.profile(),
            crate::profile::ProfileOperation::InterfaceTemplateEncoding,
            || bray_package_interface::encode_executable_template(mir, &mut context),
            crate::profile::result_outcome,
        )
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
    compilation
        .bound_unit_family_with_cancellation(root, &compilation.state.cancellation)
        .map_err(|cause| {
            super::super::executable_template_evaluation_export_error(declaration.clone(), cause)
        })?
        .into_iter()
        .map(|bound| {
            if bound.diagnostics().has_errors() {
                // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
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

    if let Some(definition) = bray_symbols::CallableDefinitionId::try_new(owner) {
        return compilation.callable_body_key(definition).map_err(|cause| {
            super::super::executable_template_evaluation_export_error(declaration.clone(), cause)
        });
    }

    if graph.runtime_default_subject(owner).is_none() {
        return Ok(None);
    }

    compilation
        .declared_unit_key(owner, bray_bound_tree::BoundUnitKind::RuntimeDefault)
        .map_err(|cause| {
            super::super::executable_template_evaluation_export_error(declaration.clone(), cause)
        })
}

#[cfg(test)]
mod tests {
    use bray_symbols::AnySymbolId;

    use super::fragment_batch_error;
    use crate::compilation::PackageInterfaceExportError;
    use crate::fact::{BatchCompletionError, CompilationFactKey, FactCycle, FactQueryError};

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
        >::Scheduler(FactQueryError::Cycle(
            cycle.clone(),
        )));

        assert_eq!(
            error,
            PackageInterfaceExportError::FragmentCoordination(FactQueryError::Cycle(cycle))
        );
    }
}
