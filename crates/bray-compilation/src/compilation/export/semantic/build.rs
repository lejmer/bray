use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::CheckedTemplateKind;
use bray_package_interface::{
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBoundary,
    InterfaceSemantics, InterfaceSymbolReference, PackageInterfaceSurface,
    commit_interface_semantic_fragments,
};
use bray_symbols::{
    AnySymbolId, CallableConstness, CallableDefinitionId, CallableInstanceData,
    CallableSignatureQuery, ExternalSymbolKey, GenericOwnerId, SymbolQueryRequest,
    diagnostic_external_symbol_identity,
};

use super::super::PackageInterfaceExportError;
use crate::compilation::Compilation;
use crate::fact::{BatchCompletionError, BatchWork, FactQueryError};

use super::context::{CheckedConstantExpression, SemanticExporter};
use super::defaults::target_dependencies;
use super::executable::{executable_templates, prepare_executable_outputs};
use super::fragment::SemanticFragment;
use super::implementation::implementation_semantics;

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

    let mut export = SemanticExporter::new(compilation, graph, surface, keys, values);

    prepare_executable_outputs(compilation, graph, selected, &export)?;

    let mut fragments =
        resolve_fragments(compilation, graph, &binder, surface, selected, keys, values)?;

    let resolved_contracts = fragments
        .iter_mut()
        .filter_map(SemanticFragment::take_callable_contract)
        .collect::<BTreeMap<_, _>>();

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

    let constant_callable_bodies =
        constant_callable_bodies(compilation, &binder, selected, &mut export)?;

    let (executable_templates, runtime_requirements) =
        executable_templates(compilation, graph, selected, &mut export)?;

    let native_boundaries = native_boundaries(compilation, selected, &export)?;

    let callable_contracts = super::execution::callable_contracts(
        compilation,
        &resolved_contracts,
        selected,
        &mut export,
    )?;

    let semantics = InterfaceSemantics::new()
        .with_contracts([], callable_contracts)
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
            crate::compilation::binder::self_type_context(export.graph, symbol)
        {
            let replacement =
                crate::compilation::substitution::contextual_self_type(binder, context)
                    .map_err(super::super::invalid_compilation_fact_error)?;

            values
                .substitute_contextual_self(result_type, context, replacement)
                .unwrap_or_else(|error| {
                    panic!("semantic_value_export_error in constant_callable_bodies: {error:?}")
                })
        } else {
            result_type
        };

        let result_type = values
            .substitute_type(result_type, substitution)
            .unwrap_or_else(|error| {
                panic!("semantic_value_export_error in constant_callable_bodies: {error:?}")
            });

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
            .unwrap_or_else(|error| {
                panic!("semantic_value_export_error in constant_callable_bodies: {error:?}")
            });

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

pub(super) fn exported_declaration_identity(
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
        .map_err(semantic_batch_error)?;

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

pub(super) fn semantic_batch_error<K>(
    error: BatchCompletionError<K, PackageInterfaceExportError>,
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

#[cfg(test)]
mod tests {
    use bray_symbols::AnySymbolId;

    use super::semantic_batch_error;
    use crate::compilation::PackageInterfaceExportError;
    use crate::fact::{BatchCompletionError, CompilationFactKey, FactCycle, FactQueryError};

    #[test]
    fn scheduler_failures_retain_their_cause() {
        let cycle = FactCycle::new([
            CompilationFactKey::SyntaxTree,
            CompilationFactKey::DeclarationTable,
            CompilationFactKey::SyntaxTree,
        ]);

        let error = semantic_batch_error(BatchCompletionError::<
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
