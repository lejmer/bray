use std::collections::BTreeSet;
use std::sync::Arc;

use bray_source::SourceInput;
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
        let mut updated = Self::load_inner(request, self.state.codegen.clone())?;

        if self.state.package_identity != updated.state.package_identity {
            return Ok(updated);
        }

        let updated_state = Arc::get_mut(&mut updated.state)
            .unwrap_or_else(|| panic!("new compilation state must be uniquely owned"));

        reuse_resolved_queries(&self.state, updated_state);

        Ok(updated)
    }

    /// Creates a revised snapshot with replacement source inputs.
    ///
    /// Package, target, dependency-interface, and export inputs remain unchanged.
    /// The current snapshot remains independently usable.
    pub fn updated_sources(&self, sources: Vec<SourceInput>) -> Result<Self, CompilationLoadError> {
        let mut request = CompilationRequest::with_options(
            self.state.package_identity.clone(),
            sources,
            self.state.options.clone(),
        )
        .with_dependency_interfaces(
            self.state
                .dependency_interfaces
                .iter()
                .filter(|input| !input.is_standard_library())
                .cloned(),
        );

        if self.state.package_source_authority.is_standard_library() {
            request = request.with_standard_library_source_authority();
        }

        // Revised snapshots retain explicit immutable roots, not resolver cache state.
        if let Some(resolver) = self.state.standard_library.as_ref() {
            request = request.with_standard_library_root(resolver.root().clone());
        }

        if let Some(resolver) = self.state.standard_library_providers.as_ref() {
            request = request.with_standard_library_provider_root(resolver.root().clone());
        }

        if let Some(export) = self.state.package_interface_export.clone() {
            request = request.with_package_interface_export(export);
        }

        if let Some(profile) = self.state.fact_runtime.profile_configuration() {
            request = request.with_profile(profile);
        }

        if let Some(product) = &self.state.profile_product {
            // Product identities share immutable canonical strings across snapshots.
            request = request.with_profile_product(product.clone());
        }

        self.updated(request)
    }
}

fn reuse_resolved_queries(
    previous: &super::state::CompilationState,
    updated: &mut super::state::CompilationState,
) {
    updated.sources = shared_sources(&previous.sources, &updated.sources);

    fork_semantic_values(previous, updated);

    let worker_budget = updated.options.worker_budget();
    let profile = updated.fact_runtime.profile_session();
    let inputs = updated.fact_runtime.input_snapshot();

    let (runtime, reusable) = previous
        .fact_runtime
        .updated(worker_budget, inputs, profile);

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
        &previous.source_reference_indexes,
        &mut updated.source_reference_indexes,
        &reusable,
        |index| CompilationFactKey::SourceReferenceIndex(bray_source::SourceId::new(index)),
    );

    reuse_indexed_cells(
        &previous.loaded_dependency_interfaces,
        &mut updated.loaded_dependency_interfaces,
        &reusable,
        dependency_interface_key,
    );

    reuse_indexed_cells(
        &previous.loaded_dependency_implementations,
        &mut updated.loaded_dependency_implementations,
        &reusable,
        dependency_implementation_key,
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
    previous: &super::state::CompilationState,
    updated: &mut super::state::CompilationState,
) {
    let Some(Ok(previous_store)) = previous.semantic_values.get() else {
        return;
    };

    updated.semantic_values = std::sync::OnceLock::from(previous_store.fork());
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

fn reuse_fixed_cells(
    previous: &super::state::CompilationState,
    updated: &mut super::state::CompilationState,
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

    reuse!(product_semantics, CompilationFactKey::ProductSemantics);

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

    reuse!(declared_units, CompilationFactKey::DeclaredUnits);

    reuse!(check_diagnostics, CompilationFactKey::CheckDiagnostics);

    reuse!(
        package_interface_export_bundle,
        CompilationFactKey::PackageInterfaceExportBundle
    );
}

fn reuse_mapped_cells(
    previous: &super::state::CompilationState,
    updated: &mut super::state::CompilationState,
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

    reuse!(test_discoveries, |key| {
        CompilationFactKey::TestDiscovery(key.clone())
    });

    reuse!(foreign_callable_contracts, |key| {
        CompilationFactKey::ForeignCallableContract(*key)
    });

    reuse!(foreign_static_contracts, |key| {
        CompilationFactKey::ForeignStaticContract(*key)
    });

    reuse!(imported_semantics, |key| {
        CompilationFactKey::ImportedSemanticRecord(*key)
    });

    reuse!(imported_constant_callable_bodies, |key| {
        CompilationFactKey::ImportedConstantCallableBody(*key)
    });

    reuse!(imported_executable_templates, |key| {
        CompilationFactKey::ImportedExecutableTemplate(*key)
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

    reuse!(native_products, |key| {
        CompilationFactKey::NativeProduct(key.clone())
    });

    reuse!(constant_instances, |key| {
        CompilationFactKey::ConstantInstance(key.clone())
    });

    reuse!(constant_calls, |key| {
        CompilationFactKey::ConstantCall(key.clone())
    });

    reuse!(codegen_artifacts, |key| {
        CompilationFactKey::CodegenArtifact(key.clone())
    });

    updated.symbol_semantics = previous.symbol_semantics.updated(reusable);
    updated.discovery_symbol_semantics = previous.discovery_symbol_semantics.updated(reusable);

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

    reuse!(checked_patterns, |key| {
        CompilationFactKey::CheckedPatterns(key.clone())
    });

    reuse!(storage_plans, |key| {
        CompilationFactKey::StoragePlan(key.clone())
    });

    reuse!(memory_operations, |key| {
        CompilationFactKey::MemoryOperations(key.clone())
    });

    reuse!(execution_candidates, |key| {
        CompilationFactKey::ExecutionCandidates(key.clone())
    });

    reuse!(certified_execution, |key| {
        CompilationFactKey::CertifiedExecution(key.clone())
    });

    reuse!(body_semantics, |key| {
        CompilationFactKey::BodySemantics(key.clone())
    });

    reuse!(checked_body_behaviors, |key| {
        CompilationFactKey::CheckedBodyBehavior(key.clone())
    });

    reuse!(lowered_units, |key| {
        CompilationFactKey::LoweredUnit(key.clone())
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

fn dependency_implementation_key(index: u32) -> CompilationFactKey {
    let Some(interface) = ImportedInterfaceId::try_from_index(index as usize) else {
        unreachable!("loaded dependency implementation index must fit its compact identity");
    };

    CompilationFactKey::DependencyImplementation(interface)
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
    use bray_symbols::{NamedTypeSymbolId, ProductKind, SymbolOrigin, TypeData};
    use bray_syntax::SyntaxText;

    use super::Compilation;
    use crate::request::{CompilationOptions, CompilationRequest};
    use crate::test_support::{package_identity, source_callable_body_key};
    use crate::{SelectedTarget, SemanticAnalysisLimits, WorkerBudget};

    #[test]
    fn variable_width_source_edits_reuse_only_unaffected_source_results() {
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
        let _ = previous.dependency_contracts(stable_unit.clone());
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
    fn updated_snapshots_do_not_eagerly_demand_semantics() {
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
    fn updated_source_snapshots_preserve_the_configured_standard_library_root() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("fixture directory must exist: {error}"));

        let root = bray_standard_library::StandardLibraryRoot::try_new(directory.path())
            .unwrap_or_else(|| panic!("temporary root must be absolute"));

        let previous = Compilation::load(
            request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Library, SelectedTarget::baseline()),
            )
            .with_standard_library_root(root.clone()),
        )
        .unwrap_or_else(|error| {
            panic!("compilation must load without reading the root: {error:?}")
        });

        let updated = previous
            .updated_sources(vec![source(10, 1, "module app;\n")])
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert_eq!(
            updated
                .state
                .standard_library
                .as_ref()
                .map(|resolver| resolver.root()),
            Some(&root)
        );

        assert_eq!(
            updated
                .state
                .dependency_interfaces
                .iter()
                .filter(|input| input.is_standard_library())
                .count(),
            1
        );
    }

    #[test]
    fn updated_source_snapshots_preserve_provider_selection_without_importing_std() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("fixture directory must exist: {error}"));

        let root = bray_standard_library::StandardLibraryRoot::try_new(directory.path())
            .unwrap_or_else(|| panic!("temporary root must be absolute"));

        let previous = Compilation::load(
            request(
                [source(10, 0, "module app;\n")],
                options(ProductKind::Executable, SelectedTarget::baseline()),
            )
            .with_standard_library_provider_root(root.clone()),
        )
        .unwrap_or_else(|error| panic!("provider-only compilation must load: {error:?}"));

        let updated = previous
            .updated_sources(vec![source(10, 1, "module app;\n")])
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert_eq!(
            updated
                .state
                .standard_library_providers
                .as_ref()
                .map(|resolver| resolver.root()),
            Some(&root)
        );

        assert!(updated.state.standard_library.is_none());

        assert!(
            updated
                .state
                .dependency_interfaces
                .iter()
                .all(|input| !input.is_standard_library())
        );
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
    fn worker_budget_changes_reuse_semantics_with_forked_values() {
        let source_text = "module app;\n\nfunc stable()\n{\n}\n";
        let previous = compilation([source(10, 0, source_text)]);
        let stable_unit = source_callable_body_key(&previous);

        previous
            .liveness(stable_unit.clone())
            .unwrap_or_else(|error| panic!("previous body semantics must build: {error:?}"));

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
            previous
                .state
                .body_semantics
                .shares_cell_with(&updated.state.body_semantics, &stable_unit)
        );

        assert_ne!(
            previous
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("previous semantic store must exist: {error:?}"))
                .id(),
            updated
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("updated semantic store must exist: {error:?}"))
                .id()
        );
    }

    #[test]
    fn semantic_limit_fields_invalidate_only_their_dependent_results() {
        let source_text = concat!(
            "module app;\n",
            "\n",
            "struct Value\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
        );

        let previous = compilation([source(10, 0, source_text)]);

        let symbols = previous
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one structure");
        };

        let subject = NamedTypeSymbolId::from(structure.id());

        let _ = previous.declared_type_representation(subject);

        previous
            .implementation_coherence_diagnostics(&previous.state.cancellation)
            .unwrap_or_else(|error| panic!("coherence validation must complete: {error:?}"));

        previous
            .callable_overload_diagnostics(&previous.state.cancellation)
            .unwrap_or_else(|error| panic!("overload validation must complete: {error:?}"));

        let default_limits = SemanticAnalysisLimits::default();

        let recursion_options = options(ProductKind::Library, SelectedTarget::baseline())
            .with_semantic_analysis_limits(SemanticAnalysisLimits::new(
                32,
                default_limits.pairwise_comparisons(),
            ));

        let recursion_updated = previous
            .updated(request([source(10, 0, source_text)], recursion_options))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            !previous
                .state
                .declared_type_representations
                .shares_cell_with(
                    &recursion_updated.state.declared_type_representations,
                    &subject
                )
        );

        assert!(
            previous
                .state
                .implementation_coherence
                .shares_storage_with(&recursion_updated.state.implementation_coherence)
        );

        assert!(
            previous
                .state
                .callable_overload_validation
                .shares_storage_with(&recursion_updated.state.callable_overload_validation)
        );

        let comparison_options = options(ProductKind::Library, SelectedTarget::baseline())
            .with_semantic_analysis_limits(SemanticAnalysisLimits::new(
                default_limits.recursion_depth(),
                64,
            ));

        let comparison_updated = previous
            .updated(request([source(10, 0, source_text)], comparison_options))
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        assert!(
            previous
                .state
                .declared_type_representations
                .shares_cell_with(
                    &comparison_updated.state.declared_type_representations,
                    &subject
                )
        );

        assert!(
            !previous
                .state
                .implementation_coherence
                .shares_storage_with(&comparison_updated.state.implementation_coherence)
        );

        assert!(
            !previous
                .state
                .callable_overload_validation
                .shares_storage_with(&comparison_updated.state.callable_overload_validation)
        );

        assert!(
            previous.state.source_unit_syntax[0]
                .shares_storage_with(&recursion_updated.state.source_unit_syntax[0])
        );

        assert!(
            previous.state.source_unit_syntax[0]
                .shares_storage_with(&comparison_updated.state.source_unit_syntax[0])
        );
    }

    #[test]
    fn target_changes_invalidate_target_properties_without_invalidating_syntax() {
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
    fn product_changes_invalidate_product_semantics_without_invalidating_other_inputs() {
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

        assert_eq!(
            previous.state.selected_target,
            updated.state.selected_target
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
