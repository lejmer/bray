use std::collections::BTreeSet;
use std::sync::Arc;

use bray_source::SourceStore;
use bray_symbols::ImportedInterfaceId;

use super::{Compilation, CompilationLoadError};
use crate::fact::{CompilationFactKey, FactCell};
use crate::request::CompilationRequest;

impl Compilation {
    /// Creates an immutable compilation snapshot from a revised request.
    ///
    /// The current snapshot remains independently usable after the revised
    /// snapshot is created.
    pub fn updated(&self, request: CompilationRequest) -> Result<Self, CompilationLoadError> {
        let mut updated = Self::load(request)?;

        if self.state.package_identity != updated.state.package_identity {
            return Ok(updated);
        }

        let updated_state = Arc::get_mut(&mut updated.state)
            .unwrap_or_else(|| panic!("new compilation state must be uniquely owned"));

        reuse_published_facts(&self.state, updated_state);

        Ok(updated)
    }
}

fn reuse_published_facts(
    previous: &super::facts::CompilationState,
    updated: &mut super::facts::CompilationState,
) {
    updated.sources = shared_sources(&previous.sources, &updated.sources);

    let semantic_values_forked = fork_semantic_values(previous, updated);
    let invalidation_roots = invalidation_roots(previous, updated, semantic_values_forked);

    let worker_budget = updated.options.worker_budget();

    let (runtime, reusable) = previous
        .fact_runtime
        .updated(worker_budget, invalidation_roots);

    updated.fact_runtime = runtime;

    reuse_indexed_cells(
        &previous.source_unit_syntax,
        &mut updated.source_unit_syntax,
        &reusable,
        |index| CompilationFactKey::SourceUnitSyntax(bray_source::SourceId::new(index)),
    );

    reuse_indexed_cells(
        &previous.declaration_chunks,
        &mut updated.declaration_chunks,
        &reusable,
        |index| CompilationFactKey::DeclarationChunk(bray_source::SourceId::new(index)),
    );

    reuse_indexed_cells(
        &previous.loaded_dependency_interfaces,
        &mut updated.loaded_dependency_interfaces,
        &reusable,
        dependency_interface_key,
    );

    reuse_indexed_cells(
        &previous.imported_semantic_graphs,
        &mut updated.imported_semantic_graphs,
        &reusable,
        imported_semantic_graph_key,
    );

    reuse_fixed_cells(previous, updated, &reusable);
    reuse_mapped_cells(previous, updated, &reusable);
}

fn fork_semantic_values(
    previous: &super::facts::CompilationState,
    updated: &mut super::facts::CompilationState,
) -> bool {
    let Some(Ok(previous_store)) = previous.semantic_values.get() else {
        return false;
    };

    updated.semantic_values = FactCell::ready(
        CompilationFactKey::SemanticValueStore,
        previous_store.fork(),
    );

    true
}

fn shared_sources(previous: &SourceStore, updated: &SourceStore) -> SourceStore {
    // Cloning equal snapshots shares their immutable source-text allocation.
    let snapshots = updated
        .iter()
        .map(|snapshot| {
            previous
                .get(snapshot.source_id())
                .filter(|previous| *previous == snapshot)
                .unwrap_or(snapshot)
                .clone()
        })
        .collect();

    SourceStore::from_snapshots(snapshots)
        .unwrap_or_else(|| panic!("loaded source snapshots must remain in source ID order"))
}

fn invalidation_roots(
    previous: &super::facts::CompilationState,
    updated: &super::facts::CompilationState,
    semantic_values_forked: bool,
) -> BTreeSet<CompilationFactKey> {
    let mut roots = BTreeSet::new();

    if !semantic_values_forked {
        roots.insert(CompilationFactKey::SemanticValueStore);
    }

    let sources_changed = previous.sources != updated.sources;

    if sources_changed {
        roots.extend([
            CompilationFactKey::SyntaxTree,
            CompilationFactKey::DeclarationTable,
        ]);

        for source in previous.sources.iter() {
            if updated.sources.get(source.source_id()) != Some(source) {
                roots.insert(CompilationFactKey::SourceUnitSyntax(source.source_id()));
                roots.insert(CompilationFactKey::DeclarationChunk(source.source_id()));
            }
        }
    }

    if previous.source_diagnostics != updated.source_diagnostics {
        roots.insert(CompilationFactKey::CheckDiagnostics);
    }

    if previous.options.selected_target() != updated.options.selected_target() {
        roots.insert(CompilationFactKey::SelectedTarget);

        roots.extend(
            previous
                .target_validity
                .keys()
                .into_iter()
                .map(CompilationFactKey::TargetValidity),
        );

        roots.extend(
            previous
                .constant_instances
                .keys()
                .into_iter()
                .map(CompilationFactKey::ConstantInstance),
        );

        roots.extend(
            previous
                .constant_calls
                .keys()
                .into_iter()
                .map(CompilationFactKey::ConstantCall),
        );
    }

    if previous.options.product_kind() != updated.options.product_kind() {
        roots.insert(CompilationFactKey::ProductSourceGraph);
    }

    if previous.options.native_link_inputs() != updated.options.native_link_inputs() {
        roots.insert(CompilationFactKey::ForeignCallableValidation);

        roots.extend(
            previous
                .foreign_callable_contracts
                .keys()
                .into_iter()
                .map(CompilationFactKey::ForeignCallableContract),
        );
    }

    if previous.dependency_interfaces != updated.dependency_interfaces {
        roots.extend([
            CompilationFactKey::ImportedSymbolSkeleton,
            CompilationFactKey::ImportedDiagnostics,
        ]);

        for (index, dependency) in previous.dependency_interfaces.iter().enumerate() {
            if updated.dependency_interfaces.get(index) != Some(dependency) {
                let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                    continue;
                };

                roots.insert(CompilationFactKey::DependencyInterface(interface));
                roots.insert(CompilationFactKey::ImportedSemanticGraph(interface));
            }
        }

        roots.extend(
            previous
                .imported_semantic_facts
                .keys()
                .into_iter()
                .map(CompilationFactKey::ImportedSemanticFact),
        );
    }

    if previous.package_interface_export != updated.package_interface_export {
        roots.insert(CompilationFactKey::PackageInterfaceExportBundle);
    }

    roots
}

fn reuse_fixed_cells(
    previous: &super::facts::CompilationState,
    updated: &mut super::facts::CompilationState,
    reusable: &BTreeSet<CompilationFactKey>,
) {
    macro_rules! reuse {
        ($field:ident, $key:expr) => {
            updated.$field = previous.$field.updated(&$key, reusable);
        };
    }

    reuse!(syntax_tree_result, CompilationFactKey::SyntaxTree);

    reuse!(
        declaration_table_result,
        CompilationFactKey::DeclarationTable
    );

    reuse!(product_source_graph, CompilationFactKey::ProductSourceGraph);

    reuse!(
        compiler_known_symbols,
        CompilationFactKey::CompilerKnownSymbols
    );

    reuse!(selected_target, CompilationFactKey::SelectedTarget);

    reuse!(
        bound_unit_identities,
        CompilationFactKey::BoundUnitIdentities
    );

    reuse!(
        discovery_symbol_graph,
        CompilationFactKey::DiscoverySymbolGraph
    );

    reuse!(symbol_graph, CompilationFactKey::SymbolGraph);

    reuse!(
        imported_symbol_skeleton,
        CompilationFactKey::ImportedSymbolSkeleton
    );

    reuse!(
        imported_diagnostics,
        CompilationFactKey::ImportedDiagnostics
    );

    reuse!(
        implementation_index,
        CompilationFactKey::ImplementationHeaderIndex
    );

    reuse!(
        implementation_coherence,
        CompilationFactKey::ImplementationCoherence
    );

    reuse!(
        callable_overload_validation,
        CompilationFactKey::CallableOverloadValidation
    );

    reuse!(
        foreign_callable_validation,
        CompilationFactKey::ForeignCallableValidation
    );

    reuse!(
        type_associated_implementation_index,
        CompilationFactKey::TypeAssociatedImplementationIndex
    );

    reuse!(
        semantic_diagnostics,
        CompilationFactKey::SemanticDiagnostics
    );

    reuse!(
        constant_template_keys,
        CompilationFactKey::ConstantTemplateKeys
    );

    reuse!(callable_body_keys, CompilationFactKey::CallableBodyKeys);

    reuse!(
        predicate_definition_keys,
        CompilationFactKey::PredicateDefinitionKeys
    );

    reuse!(check_diagnostics, CompilationFactKey::CheckDiagnostics);

    reuse!(
        package_interface_export_bundle,
        CompilationFactKey::PackageInterfaceExportBundle
    );
}

fn reuse_mapped_cells(
    previous: &super::facts::CompilationState,
    updated: &mut super::facts::CompilationState,
    reusable: &BTreeSet<CompilationFactKey>,
) {
    macro_rules! reuse {
        ($field:ident, $key:expr) => {
            updated.$field = previous.$field.updated(reusable, $key);
        };
    }

    // Cache maps and runtime keys own stable identities across snapshot lifetimes.
    reuse!(target_validity, |key| {
        CompilationFactKey::TargetValidity(key.clone())
    });

    reuse!(module_contribution_gates, |key| {
        CompilationFactKey::ModuleContributionGate(*key)
    });

    reuse!(callable_type_directives, |key| {
        CompilationFactKey::CallableTypeDirectives(*key)
    });

    reuse!(foreign_callable_contracts, |key| {
        CompilationFactKey::ForeignCallableContract(*key)
    });

    reuse!(imported_semantic_facts, |key| {
        CompilationFactKey::ImportedSemanticFact(*key)
    });

    reuse!(implementation_participation, |key| {
        CompilationFactKey::ImplementationParticipation(key.clone())
    });

    reuse!(type_associated_surfaces, |key| {
        CompilationFactKey::TypeAssociatedSurface(*key)
    });

    reuse!(declared_type_representations, |key| {
        CompilationFactKey::DeclaredTypeRepresentation(*key)
    });

    reuse!(implementation_candidate_sets, |key| {
        CompilationFactKey::ImplementationCandidateSet(*key)
    });

    reuse!(trait_implementation_conformance, |key| {
        CompilationFactKey::TraitImplementationConformance(*key)
    });

    reuse!(generic_constraint_satisfaction, |key| {
        CompilationFactKey::GenericConstraintSatisfaction(*key)
    });

    reuse!(implementation_selections, |key| {
        CompilationFactKey::ImplementationSelection(*key)
    });

    reuse!(iteration_sources, |key| {
        CompilationFactKey::IterationSource(key.clone())
    });

    reuse!(constant_instances, |key| {
        CompilationFactKey::ConstantInstance(key.clone())
    });

    reuse!(constant_calls, |key| {
        CompilationFactKey::ConstantCall(key.clone())
    });

    updated.symbol_facts = previous.symbol_facts.updated(reusable);
    updated.discovery_symbol_facts = previous.discovery_symbol_facts.updated(reusable);

    reuse!(bound_units, |key| CompilationFactKey::BoundUnit(
        key.clone()
    ));

    reuse!(declared_value_type_templates, |key| {
        CompilationFactKey::DeclaredValueTypeTemplates(key.clone())
    });

    reuse!(checked_control_flow, |key| {
        CompilationFactKey::CheckedControlFlow(key.clone())
    });

    reuse!(provisional_expression_semantics, |key| {
        CompilationFactKey::ProvisionalExpressionSemantics(key.clone())
    });

    reuse!(expression_semantics, |key| {
        CompilationFactKey::ExpressionSemantics(key.clone())
    });

    reuse!(checked_expression_types, |key| {
        CompilationFactKey::CheckedExpressionTypes(key.clone())
    });

    reuse!(checked_patterns, |key| {
        CompilationFactKey::CheckedPatterns(key.clone())
    });

    reuse!(checked_semantic_selections, |key| {
        CompilationFactKey::CheckedSemanticSelections(key.clone())
    });

    reuse!(storage_plans, |key| {
        CompilationFactKey::StoragePlan(key.clone())
    });

    reuse!(liveness, |key| CompilationFactKey::Liveness(key.clone()));

    reuse!(refinement_facts, |key| {
        CompilationFactKey::RefinementFacts(key.clone())
    });

    reuse!(storage_flow_facts, |key| {
        CompilationFactKey::StorageFlowFacts(key.clone())
    });

    reuse!(dependency_contracts, |key| {
        CompilationFactKey::DependencyContracts(key.clone())
    });

    reuse!(async_facts, |key| {
        CompilationFactKey::AsyncFacts(key.clone())
    });

    reuse!(body_behavior_contributions, |key| {
        CompilationFactKey::BodyBehaviorContributions(key.clone())
    });

    reuse!(checked_body_behaviors, |key| {
        CompilationFactKey::CheckedBodyBehavior(key.clone())
    });

    reuse!(symbolic_constant_terms, |key| {
        CompilationFactKey::SymbolicConstantTerm(key.clone())
    });
}

fn reuse_indexed_cells<T>(
    previous: &[FactCell<T>],
    updated: &mut [FactCell<T>],
    reusable: &BTreeSet<CompilationFactKey>,
    key: impl Fn(u32) -> CompilationFactKey,
) {
    for (index, updated_cell) in updated.iter_mut().enumerate() {
        let Some(previous_cell) = previous.get(index) else {
            continue;
        };

        let Ok(index) = u32::try_from(index) else {
            continue;
        };

        let key = key(index);

        *updated_cell = previous_cell.updated(&key, reusable);
    }
}

fn dependency_interface_key(index: u32) -> CompilationFactKey {
    let Some(interface) = ImportedInterfaceId::try_from_index(index as usize) else {
        unreachable!("loaded dependency interface index must fit its compact identity");
    };

    CompilationFactKey::DependencyInterface(interface)
}

fn imported_semantic_graph_key(index: u32) -> CompilationFactKey {
    let Some(interface) = ImportedInterfaceId::try_from_index(index as usize) else {
        unreachable!("imported semantic graph index must fit its compact identity");
    };

    CompilationFactKey::ImportedSemanticGraph(interface)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_source::{SourceId, SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{ProductKind, TypeData};
    use bray_syntax::SyntaxText;

    use super::Compilation;
    use crate::request::{CompilationOptions, CompilationRequest};
    use crate::test_support::{package_identity, source_callable_body_key};
    use crate::{SelectedTarget, WorkerBudget};

    #[test]
    fn variable_width_source_edits_reuse_only_unaffected_source_facts() {
        let previous = compilation([
            source(10, 0, "module app.changed;\n\nconst value: i32 = 1;\n"),
            source(11, 0, "module app.stable;\n\nfunc stable()\n{\n}\n"),
        ]);

        previous.source_unit_syntax(SourceId::new(0));
        previous.source_unit_syntax(SourceId::new(1));
        previous.declaration_chunk(SourceId::new(0));
        previous.declaration_chunk(SourceId::new(1));

        let stable_unit = source_callable_body_key(&previous);

        let _ = previous.bound_unit(stable_unit.clone());
        let _ = previous.control_flow(stable_unit.clone());

        let updated = previous
            .updated(request(
                [
                    source(
                        10,
                        1,
                        "module app.changed;\n\nconst value: i32 = 2_000_000;\n",
                    ),
                    source(11, 0, "module app.stable;\n\nfunc stable()\n{\n}\n"),
                ],
                options(ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            !previous.state.source_unit_syntax[0]
                .shares_storage_with(&updated.state.source_unit_syntax[0])
        );

        assert!(
            previous.state.source_unit_syntax[1]
                .shares_storage_with(&updated.state.source_unit_syntax[1])
        );

        let previous_source = previous
            .source(SourceId::new(1))
            .unwrap_or_else(|| panic!("stable source must exist"))
            .shared_text();

        let updated_source = updated
            .source(SourceId::new(1))
            .unwrap_or_else(|| panic!("stable source must exist"))
            .shared_text();

        assert!(Arc::ptr_eq(&previous_source, &updated_source));

        assert!(
            !previous.state.declaration_chunks[0]
                .shares_storage_with(&updated.state.declaration_chunks[0])
        );

        assert!(
            previous.state.declaration_chunks[1]
                .shares_storage_with(&updated.state.declaration_chunks[1])
        );

        assert!(
            !previous
                .state
                .bound_units
                .shares_cell_with(&updated.state.bound_units, &stable_unit)
        );

        assert!(
            !previous
                .state
                .checked_control_flow
                .shares_cell_with(&updated.state.checked_control_flow, &stable_unit)
        );
    }

    #[test]
    fn updated_snapshots_do_not_eagerly_demand_semantic_facts() {
        let previous = compilation([
            source(10, 0, "module app.changed;\n\nconst value: i32 = 1;\n"),
            source(11, 0, "module app.stable;\n\nfunc stable()\n{\n}\n"),
        ]);

        let _ = previous
            .symbol_graph()
            .unwrap_or_else(|error| panic!("previous symbol graph must build: {error:?}"));

        let updated = previous
            .updated(request(
                [
                    source(
                        10,
                        1,
                        "module app.changed;\n\nconst value: i32 = 1;\nconst other: i32 = 2;\n",
                    ),
                    source(11, 0, "module app.stable;\n\nfunc stable()\n{\n}\n"),
                ],
                options(ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(updated.state.symbol_graph.get().is_none());
        assert!(updated.state.declaration_table_result.get().is_none());
    }

    #[test]
    fn semantic_value_stores_are_snapshot_local() {
        let previous = compilation([source(10, 0, "module app;\n")]);

        let previous_store = previous
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("previous semantic store must build: {error:?}"));

        let inherited = previous_store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("previous semantic type must intern: {error:?}"));

        let updated = previous
            .updated(request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            !previous
                .state
                .semantic_values
                .shares_storage_with(&updated.state.semantic_values)
        );

        let updated_store = updated
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("updated semantic store must build: {error:?}"));

        assert_ne!(previous_store.id(), updated_store.id());

        assert_eq!(
            updated_store.type_data(inherited).as_deref(),
            Ok(&TypeData::Error)
        );
    }

    #[test]
    fn worker_budget_changes_reuse_semantic_facts_with_forked_values() {
        let source_text = "module app;\n\nfunc stable()\n{\n}\n";
        let previous = compilation([source(10, 0, source_text)]);
        let stable_unit = source_callable_body_key(&previous);

        let _ = previous.bound_unit(stable_unit.clone());

        let parallel = WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("parallel worker budget must build: {error:?}"));

        let updated = previous
            .updated(request(
                [source(10, 0, source_text)],
                CompilationOptions::new(parallel, ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            previous
                .state
                .bound_units
                .shares_cell_with(&updated.state.bound_units, &stable_unit)
        );

        assert!(
            !previous
                .state
                .semantic_values
                .shares_storage_with(&updated.state.semantic_values)
        );
    }

    #[test]
    fn target_changes_invalidate_target_facts_without_invalidating_syntax() {
        let previous = compilation([source(10, 0, "module app;\n")]);

        previous.source_unit_syntax(SourceId::new(0));
        previous.selected_target();

        let baseline = SelectedTarget::baseline();

        let revised_target =
            SelectedTarget::new(baseline.profile().clone(), RuntimeAbiVersion::new(1, 1));

        let updated = previous
            .updated(request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Library, revised_target),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            previous.state.source_unit_syntax[0]
                .shares_storage_with(&updated.state.source_unit_syntax[0])
        );

        assert!(
            !previous
                .state
                .selected_target
                .shares_storage_with(&updated.state.selected_target)
        );

        assert_eq!(
            previous.selected_target().target().runtime_abi(),
            RuntimeAbiVersion::new(1, 0)
        );

        assert_eq!(
            updated.selected_target().target().runtime_abi(),
            RuntimeAbiVersion::new(1, 1)
        );
    }

    #[test]
    fn product_changes_invalidate_product_facts_without_invalidating_other_inputs() {
        let previous = compilation([source(10, 0, "module app;\n")]);

        previous.source_unit_syntax(SourceId::new(0));
        previous.selected_target();
        let _ = previous.product_source_graph();

        let updated = previous
            .updated(request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Executable, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            previous.state.source_unit_syntax[0]
                .shares_storage_with(&updated.state.source_unit_syntax[0])
        );

        assert!(
            previous
                .state
                .selected_target
                .shares_storage_with(&updated.state.selected_target)
        );

        assert!(
            !previous
                .state
                .product_source_graph
                .shares_storage_with(&updated.state.product_source_graph)
        );
    }

    #[test]
    fn old_and_updated_snapshots_answer_requests_concurrently() {
        let previous = compilation([source(10, 0, "module old;\n")]);

        let updated = previous
            .updated(request(
                [source(10, 1, "module new;\n")],
                options(ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        let texts = std::thread::scope(|scope| {
            let previous = scope.spawn(|| {
                previous
                    .source_unit_syntax(SourceId::new(0))
                    .map(|result| result.source_unit().full_text())
            });

            let updated = scope.spawn(|| {
                updated
                    .source_unit_syntax(SourceId::new(0))
                    .map(|result| result.source_unit().full_text())
            });

            [previous, updated].map(|request| {
                request
                    .join()
                    .unwrap_or_else(|_| panic!("snapshot request must not panic"))
            })
        });

        assert_eq!(
            texts,
            [
                Some(String::from("module old;\n")),
                Some(String::from("module new;\n"))
            ]
        );
    }

    #[test]
    fn repeated_updates_do_not_retain_obsolete_compilation_states() {
        let previous = compilation([source(10, 0, "module app;\n")]);

        previous.source_unit_syntax(SourceId::new(0));

        let updated = previous
            .updated(request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert_eq!(
            updated.state.source_unit_syntax[0].storage_reference_count(),
            2
        );

        drop(previous);

        assert_eq!(
            updated.state.source_unit_syntax[0].storage_reference_count(),
            1
        );

        let newest = updated
            .updated(request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Library, SelectedTarget::baseline()),
            ))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        drop(updated);

        assert_eq!(
            newest.state.source_unit_syntax[0].storage_reference_count(),
            1
        );
    }

    fn compilation<const N: usize>(sources: [SourceInput; N]) -> Compilation {
        Compilation::load(request(
            sources,
            options(ProductKind::Library, SelectedTarget::baseline()),
        ))
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn request(
        sources: impl IntoIterator<Item = SourceInput>,
        options: CompilationOptions,
    ) -> CompilationRequest {
        CompilationRequest::with_options(package_identity(), sources.into_iter().collect(), options)
    }

    fn options(product: ProductKind, target: SelectedTarget) -> CompilationOptions {
        CompilationOptions::new(WorkerBudget::serial(), product, target)
    }

    fn source(identity: u32, version: u64, text: &str) -> SourceInput {
        SourceInput::virtual_text(
            SourceIdentity::new(identity),
            format!("source-{identity}"),
            SourceVersion::new(version),
            text.to_owned(),
        )
    }
}
