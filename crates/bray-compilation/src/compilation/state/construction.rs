use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

use bray_binder::BinderDependency;
use bray_bound_tree::BoundUnitKey;
use bray_codegen::CodegenConfiguration;
use bray_declarations::{
    DeclarationChunkResult, DeclarationTable, DeclarationTableResult,
    discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_parser::{SourceUnitSyntaxResult, SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceId, SourceInput, SourceLoadError, SourceSnapshot, SourceStore};
use bray_symbols::{PackageIdentity, ProductIdentity, SemanticValueStore, SymbolGraph};
use bray_syntax::SyntaxTree;

use crate::fact::{
    BoundUnitIdentityMap, CancellationToken, CompilationFactKey, FactCell, FactCellMap,
    FactQueryError, FactRuntime, PublishedUnitResult, UnitQueryCache,
};
use crate::request::{CompilationRequest, DependencyInterfaceInput};

use crate::compilation::binder::CompilationSymbolSemantics;
use crate::compilation::load::{
    CompilationLoadError, SourceInputDiagnosticContext, duplicate_source_input_diagnostic,
    missing_source_input_diagnostic, next_diagnostic_id, package_source_authority_diagnostic,
    source_load_diagnostic,
};

use super::{Compilation, CompilationState};

impl Compilation {
    /// Loads source inputs into durable compilation state without requesting derived results.
    pub fn load(request: CompilationRequest) -> Result<Self, CompilationLoadError> {
        Self::load_inner(request, None)
    }

    /// Loads source inputs with the code generation backend selected for this composition.
    pub fn load_with_codegen(
        request: CompilationRequest,
        codegen: CodegenConfiguration,
    ) -> Result<Self, CompilationLoadError> {
        Self::load_inner(request, Some(codegen))
    }

    pub(in crate::compilation) fn load_inner(
        request: CompilationRequest,
        codegen: Option<CodegenConfiguration>,
    ) -> Result<Self, CompilationLoadError> {
        let (
            package_identity,
            package_source_authority,
            standard_library_root,
            standard_library_provider_root,
            options,
            source_inputs,
            mut dependency_interfaces,
            platform_services,
            runtime_roles,
            package_interface_export,
            profile,
            profile_product,
        ) = request.into_parts();

        let worker_budget = options.worker_budget();

        let profile = profile.map(|configuration| {
            let product = profile_product
                .as_ref()
                .map(ProductIdentity::name)
                .or_else(|| {
                    package_interface_export
                        .as_ref()
                        .map(|export| export.identity().product().as_str())
                })
                .unwrap_or_else(|| match options.product_kind() {
                    bray_symbols::ProductKind::Library => "library",
                    bray_symbols::ProductKind::Executable => "executable",
                    bray_symbols::ProductKind::Test => "test",
                });

            let context = crate::CompilationProfileContext {
                package: package_identity.as_str().to_owned(),
                product: product.to_owned(),
                target: options
                    .selected_target()
                    .profile()
                    .identity()
                    .as_str()
                    .to_owned(),
            };

            (configuration, context)
        });

        let mut fact_runtime = FactRuntime::with_profile(worker_budget, profile);
        let profile_session = fact_runtime.profile_session();

        let load_span = profile_session
            .as_deref()
            .map(|profile| profile.start(crate::profile::ProfileOperation::CompilationLoad, None));

        let standard_library =
            standard_library_root.map(bray_standard_library::StandardLibraryResolver::new);

        let standard_library_providers =
            standard_library_provider_root.map(bray_standard_library::StandardLibraryResolver::new);

        if let Some(resolver) = standard_library.as_ref() {
            // The synthetic dependency and native selection share one immutable resolver cache.
            // SelectedTarget is small immutable request data retained by both compilation queries.
            dependency_interfaces.push(DependencyInterfaceInput::for_standard_library(
                resolver.clone(),
                options.selected_target().clone(),
            ));
        }

        dependency_interfaces.sort_by(|left, right| {
            (left.package(), left.product()).cmp(&(right.package(), right.product()))
        });

        let (sources, diagnostics) =
            load_source_inputs(&package_identity, package_source_authority, source_inputs)?;

        let source_count = sources.len();
        let dependency_count = dependency_interfaces.len();

        fact_runtime.set_inputs(crate::compilation::input::compilation_inputs(
            &package_identity,
            package_source_authority,
            standard_library.as_ref(),
            standard_library_providers.as_ref(),
            &options,
            &sources,
            &diagnostics,
            &dependency_interfaces,
            &platform_services,
            &runtime_roles,
            package_interface_export.as_ref(),
            codegen.as_ref(),
        ));

        if let Some(profile) = fact_runtime.profile() {
            profile.record_metric(
                crate::profile::ProfileMetricKind::SourceUnits,
                u64::try_from(source_count).unwrap_or(u64::MAX),
            );

            let source_bytes = sources.iter().fold(0_u64, |total, source| {
                total.saturating_add(u64::try_from(source.text().len()).unwrap_or(u64::MAX))
            });

            profile.record_metric(crate::profile::ProfileMetricKind::SourceBytes, source_bytes);
        }

        let compiler_known_symbols = super::inputs::shared_catalog();
        let selected_target = options.selected_target().clone();

        // The filtered view and compilation state retain the shared generated catalog.
        let available_compiler_known_symbols = Arc::clone(&compiler_known_symbols)
            .available_symbols(|rule| selected_target.supports(rule));

        let selected_target =
            crate::SelectedTargetContext::new(selected_target, available_compiler_known_symbols);

        if let Some(span) = load_span {
            span.finish(crate::CompilationProfileOutcome::Completed);
        }

        Ok(Self {
            state: Arc::new(CompilationState {
                package_identity,
                package_source_authority,
                standard_library,
                standard_library_providers,
                options,
                sources,
                source_diagnostics: diagnostics,
                package_interface_export,
                profile_product,
                dependency_interfaces: dependency_interfaces.into_boxed_slice(),
                platform_services: platform_services.into_boxed_slice(),
                runtime_roles: runtime_roles.into_boxed_slice(),
                fact_runtime,
                cancellation: CancellationToken::new(),
                source_unit_syntax: empty_query_caches(source_count),
                syntax_tree_result: FactCell::new(),
                declaration_chunks: empty_query_caches(source_count),
                source_reference_indexes: empty_query_caches(source_count),
                declaration_table_result: FactCell::new(),
                product_source_graph: FactCell::new(),
                product_semantics: FactCell::new(),
                test_discoveries: FactCellMap::new(),
                compiler_known_symbols,
                selected_target,
                target_validity: FactCellMap::new(),
                module_contribution_gates: FactCellMap::new(),
                callable_type_directives: FactCellMap::new(),
                bound_unit_identities: OnceLock::new(),
                discovery_symbol_graph: FactCell::new(),
                symbol_graph: FactCell::new(),
                semantic_values: OnceLock::new(),
                loaded_dependency_interfaces: empty_query_caches(dependency_count),
                loaded_dependency_implementations: empty_query_caches(dependency_count),
                imported_symbol_skeleton: FactCell::new(),
                imported_semantic_graphs: empty_query_caches(dependency_count),
                imported_semantics: FactCellMap::new(),
                imported_constant_callable_bodies: FactCellMap::new(),
                imported_executable_templates: FactCellMap::new(),
                imported_diagnostics: FactCell::new(),
                implementation_participation: FactCellMap::new(),
                implementation_coherence: FactCell::new(),
                callable_overload_validation: FactCell::new(),
                foreign_callable_contracts: FactCellMap::new(),
                foreign_static_contracts: FactCellMap::new(),
                foreign_callable_validation: FactCell::new(),
                type_associated_surfaces: FactCellMap::new(),
                declared_type_representations: FactCellMap::new(),
                codegen_lifecycle_needs: Mutex::new(BTreeMap::new()),
                type_associated_implementation_index: FactCell::new(),
                implementation_index: FactCell::new(),
                implementation_candidate_sets: FactCellMap::new(),
                trait_implementation_conformance: FactCellMap::new(),
                generic_constraint_satisfaction: FactCellMap::new(),
                implementation_selections: FactCellMap::new(),
                iteration_sources: FactCellMap::new(),
                operation_selections: FactCellMap::new(),
                semantic_diagnostics: FactCell::new(),
                symbol_semantics: CompilationSymbolSemantics::new(),
                discovery_symbol_semantics: CompilationSymbolSemantics::new(),
                bound_units: UnitQueryCache::new(),
                declared_value_type_templates: UnitQueryCache::new(),
                checked_control_flow: UnitQueryCache::new(),
                provisional_expression_semantics: UnitQueryCache::new(),
                expression_semantics: UnitQueryCache::new(),
                checked_patterns: UnitQueryCache::new(),
                storage_plans: UnitQueryCache::new(),
                memory_operations: UnitQueryCache::new(),
                execution_candidates: UnitQueryCache::new(),
                certified_execution: UnitQueryCache::new(),
                body_semantics: UnitQueryCache::new(),
                checked_body_behaviors: UnitQueryCache::new(),
                lowered_units: UnitQueryCache::new(),
                codegen,
                codegen_artifacts: FactCellMap::new(),
                native_products: FactCellMap::new(),
                declared_units: FactCell::new(),
                symbolic_constant_terms: UnitQueryCache::new(),
                embedded_constant_expectations: Mutex::new(BTreeMap::new()),
                constant_instances: FactCellMap::new(),
                constant_calls: FactCellMap::new(),
                check_diagnostics: FactCell::new(),
                package_interface_export_bundle: FactCell::new(),
            }),
        })
    }

    /// Loads one source package with default options without requesting derived results.
    pub fn load_sources(
        package_identity: PackageIdentity,
        sources: Vec<SourceInput>,
    ) -> Result<Self, CompilationLoadError> {
        Self::load(CompilationRequest::new(package_identity, sources))
    }

    /// Returns the syntax result for one source unit.
    pub fn source_unit_syntax(&self, source_id: SourceId) -> Option<&SourceUnitSyntaxResult> {
        self.state.sources.get(source_id)?;

        let cache = self.state.source_unit_syntax.get(source_id.to_index()?)?;

        Some(self.evaluate_query(
            CompilationFactKey::SourceUnitSyntax(source_id),
            cache,
            || {
                let snapshot = self
                    .source(source_id)
                    .unwrap_or_else(|| panic!("source query cache should match source store"));

                let result = parse_source_unit(snapshot);

                if let Some(profile) = self.state.fact_runtime.profile() {
                    let mut tokens = 0_u64;

                    bray_syntax::walk_source_unit(result.source_unit(), |event| {
                        if matches!(event, bray_syntax::SyntaxWalkEvent::Token(_)) {
                            tokens = tokens.saturating_add(1);
                        }

                        bray_syntax::SyntaxWalkControl::Continue
                    });

                    profile.record_metric(crate::profile::ProfileMetricKind::SyntaxTokens, tokens);
                }

                result
            },
        ))
    }

    /// Returns the syntax tree result for all loaded source units.
    pub fn syntax_tree_result(&self) -> &SyntaxTreeResult {
        self.evaluate_frozen_query(
            CompilationFactKey::SyntaxTree,
            &self.state.syntax_tree_result,
            || {
                let source_ids = self
                    .sources()
                    .iter()
                    .map(SourceSnapshot::source_id)
                    .collect::<Vec<_>>();

                let source_units = self.map_queries(source_ids.len(), |index| {
                    // Source stores and query caches share the loaded source count.
                    // The whole-source syntax result owns the composed source units.
                    match self.source_unit_syntax(source_ids[index]) {
                        Some(result) => result.clone(),
                        None => panic!("source unit query cache should match source store"),
                    }
                });

                SyntaxTreeResult::from_source_unit_results(source_units)
            },
        )
    }

    /// Returns the syntax tree for all loaded source units.
    pub fn syntax_tree(&self) -> &SyntaxTree {
        self.syntax_tree_result().syntax_tree()
    }

    /// Returns the declaration-discovery result for one source unit.
    pub fn declaration_chunk(&self, source_id: SourceId) -> Option<&DeclarationChunkResult> {
        let cache = self.state.declaration_chunks.get(source_id.to_index()?)?;

        Some(self.evaluate_query(
            CompilationFactKey::DeclarationChunk(source_id),
            cache,
            || {
                let Some(syntax) = self.source_unit_syntax(source_id) else {
                    panic!("declaration chunk cache should match source syntax cache");
                };

                discover_source_unit_declarations(syntax.source_unit())
            },
        ))
    }

    /// Returns the merged declaration-discovery result for this compilation.
    pub fn declaration_table_result(&self) -> &DeclarationTableResult {
        self.evaluate_query(
            CompilationFactKey::DeclarationTable,
            &self.state.declaration_table_result,
            || {
                let source_ids = self
                    .sources()
                    .iter()
                    .map(SourceSnapshot::source_id)
                    .collect::<Vec<_>>();

                let chunks = self.map_queries(source_ids.len(), |index| {
                    match self.declaration_chunk(source_ids[index]) {
                        // The merge owns its input chunks after scheduled work completes.
                        Some(chunk) => chunk.clone(),
                        None => panic!("declaration chunk cache should match source store"),
                    }
                });

                let result = merge_declaration_chunks(chunks.iter());

                if let Some(profile) = self.state.fact_runtime.profile() {
                    profile.record_metric(
                        crate::profile::ProfileMetricKind::Declarations,
                        u64::try_from(result.table().declarations().len()).unwrap_or(u64::MAX),
                    );
                }

                result
            },
        )
    }

    pub(in crate::compilation) fn map_queries<T>(
        &self,
        len: usize,
        operation: impl Fn(usize) -> T + Send + Sync,
    ) -> Vec<T>
    where
        T: Send,
    {
        crate::compilation::boundary::expect_uncancelled_query(
            "map_queries",
            self.state.fact_runtime.map_indexed(len, operation),
        )
    }

    /// Returns the merged declaration table for this compilation.
    pub fn declaration_table(&self) -> &DeclarationTable {
        self.declaration_table_result().table()
    }

    /// Returns diagnostics produced by declaration discovery and merge.
    pub fn declaration_diagnostics(&self) -> &DiagnosticBag {
        self.declaration_table_result().diagnostics()
    }

    /// Returns the compilation-wide symbol graph.
    pub fn symbol_graph(&self) -> Result<&SymbolGraph, FactQueryError> {
        self.evaluate_frozen_query(
            CompilationFactKey::SymbolGraph,
            &self.state.symbol_graph,
            || {
                let provider = Arc::clone(self.compiler_known_provider());

                // The graph owns the Arc-backed package identity after compilation retains its input.
                SymbolGraph::build_source_with_provider(
                    self.package_identity().clone(),
                    self.product_source_graph()?.declarations(),
                    self.syntax_tree(),
                    provider,
                )
                .map_err(FactQueryError::SymbolGraph)
            },
        )
        .as_ref()
        .map_err(Clone::clone)
    }

    pub(in crate::compilation) fn discovery_symbol_graph(
        &self,
    ) -> Result<&SymbolGraph, FactQueryError> {
        self.evaluate_frozen_query(
            CompilationFactKey::DiscoverySymbolGraph,
            &self.state.discovery_symbol_graph,
            || {
                let provider = Arc::clone(self.compiler_known_provider());

                SymbolGraph::build_source_with_provider(
                    self.package_identity().clone(),
                    self.declaration_table(),
                    self.syntax_tree(),
                    provider,
                )
                .map_err(FactQueryError::SymbolGraph)
            },
        )
        .as_ref()
        .map_err(Clone::clone)
    }

    /// Returns the canonical semantic value store for this compilation snapshot.
    pub fn semantic_value_store(&self) -> Result<&SemanticValueStore, FactQueryError> {
        self.state
            .semantic_values
            .get_or_init(SemanticValueStore::try_new)
            .as_ref()
            .map_err(|error| FactQueryError::SemanticValueStoreCreate(*error))
    }

    pub(in crate::compilation) fn bound_unit_id(
        &self,
        key: &BoundUnitKey,
    ) -> Result<bray_bound_tree::BoundUnitId, FactQueryError> {
        let syntax = self.syntax_tree();

        self.state
            .bound_unit_identities
            .get_or_init(|| BoundUnitIdentityMap::from_syntax(syntax))
            .as_ref()
            .map_err(Clone::clone)?
            .unit_id(key)
    }

    pub(in crate::compilation) fn evaluate_query<'a, T>(
        &self,
        key: CompilationFactKey,
        cache: &'a FactCell<T>,
        compute: impl FnOnce() -> T + Send,
    ) -> &'a T
    where
        T: std::hash::Hash + Send,
    {
        crate::compilation::boundary::expect_uncancelled_query(
            "evaluate_query",
            cache.get_or_compute(
                &self.state.fact_runtime,
                key,
                &self.state.cancellation,
                || Ok(compute()),
            ),
        )
    }

    pub(in crate::compilation) fn evaluate_frozen_query<'a, T>(
        &self,
        key: CompilationFactKey,
        cache: &'a FactCell<T>,
        compute: impl FnOnce() -> T + Send,
    ) -> &'a T
    where
        T: std::hash::Hash + Send,
    {
        if let Some(value) = cache.get_if_published(&key).unwrap_or_else(|error| {
            panic!("frozen compilation value publication must remain valid: {error:?}")
        }) {
            self.state
                .fact_runtime
                .record_frozen_fact(&key)
                .unwrap_or_else(|error| {
                    panic!("frozen compilation dependency tracking must succeed: {error:?}")
                });

            return value;
        }

        self.evaluate_query(key, cache, compute)
    }

    pub(in crate::compilation) fn query_with_cancellation<'a, T>(
        &self,
        key: CompilationFactKey,
        cache: &'a FactCell<T>,
        cancellation: &CancellationToken,
        compute: impl FnOnce(&CancellationToken) -> Result<T, FactQueryError> + Send,
    ) -> Result<&'a T, FactQueryError>
    where
        T: std::hash::Hash + Send,
    {
        cache.get_or_compute_requested(&self.state.fact_runtime, key, cancellation, compute)
    }

    pub(in crate::compilation) fn unit_query<T>(
        &self,
        cache: &UnitQueryCache<T>,
        semantic_key: CompilationFactKey,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        compute: impl FnOnce(
            &CancellationToken,
        )
            -> Result<(DiagnosticResult<T>, Box<[BinderDependency]>), FactQueryError>
        + Send,
    ) -> Result<Arc<PublishedUnitResult<T>>, FactQueryError>
    where
        T: std::hash::Hash + Send + Sync,
    {
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(crate::QueryPriority::Normal);

        self.unit_query_with_priority(cache, semantic_key, key, cancellation, priority, compute)
    }

    pub(in crate::compilation) fn unit_query_with_priority<T>(
        &self,
        cache: &UnitQueryCache<T>,
        semantic_key: CompilationFactKey,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: crate::QueryPriority,
        compute: impl FnOnce(
            &CancellationToken,
        )
            -> Result<(DiagnosticResult<T>, Box<[BinderDependency]>), FactQueryError>
        + Send,
    ) -> Result<Arc<PublishedUnitResult<T>>, FactQueryError>
    where
        T: std::hash::Hash + Send + Sync,
    {
        cache.get_or_compute_with_priority(
            &self.state.fact_runtime,
            cancellation,
            priority,
            semantic_key,
            key,
            compute,
        )
    }
}

fn load_source_inputs(
    package_identity: &PackageIdentity,
    package_source_authority: crate::PackageSourceAuthority,
    source_inputs: Vec<SourceInput>,
) -> Result<(SourceStore, DiagnosticBag), CompilationLoadError> {
    let mut sources = SourceStore::with_capacity(source_inputs.len());
    let mut source_identities = BTreeSet::new();
    let mut diagnostics = DiagnosticBag::new();

    if !package_source_authority.accepts(package_identity) {
        diagnostics.add(package_source_authority_diagnostic(
            next_diagnostic_id(&diagnostics)?,
            package_identity,
            package_source_authority,
        ));
    }

    if source_inputs.is_empty() {
        diagnostics.add(missing_source_input_diagnostic(next_diagnostic_id(
            &diagnostics,
        )?));
    }

    for (source_index, source_input) in (0_u64..).zip(source_inputs) {
        // Preserve request metadata so source-load diagnostics can identify the input.
        let diagnostic_context =
            SourceInputDiagnosticContext::from_input(source_index, &source_input);

        if !source_identities.insert(source_input.identity()) {
            diagnostics.add(duplicate_source_input_diagnostic(
                next_diagnostic_id(&diagnostics)?,
                diagnostic_context,
            ));

            continue;
        }

        match sources.insert_input(source_input) {
            Ok(_) => {}
            Err(error) => {
                let stops_loading = matches!(error, SourceLoadError::TooManySources { .. });

                diagnostics.add(source_load_diagnostic(
                    next_diagnostic_id(&diagnostics)?,
                    diagnostic_context,
                    error,
                ));

                if stops_loading {
                    break;
                }
            }
        }
    }

    Ok((sources, diagnostics))
}

impl fmt::Debug for Compilation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Compilation")
            .field("package_identity", self.package_identity())
            .field("options", &self.options())
            .field("source_count", &self.source_count())
            .finish_non_exhaustive()
    }
}

fn empty_query_caches<T>(len: usize) -> Vec<FactCell<T>> {
    (0..len).map(|_| FactCell::new()).collect()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey, BoundUnitKind};
    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_declarations::{DeclarationKind, ModulePath};
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId, DiagnosticKind,
        DiagnosticNote, DiagnosticNoteKind, DiagnosticSourceInput, DiagnosticSourceInputOrigin,
        SeverityKind,
    };
    use bray_source::{
        SourceId, SourceIdentity, SourceInput, SourceInputKind, SourceOriginKind, SourceVersion,
        TextSize,
    };
    use bray_symbols::{
        FunctionSymbolId, PackageIdentity, StructSymbolId, SymbolGraph, SymbolOrigin,
    };
    use bray_syntax::SourceSyntaxNode;

    use crate::compilation::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };
    use crate::fact::{CompilationFactKey, CompilationInputKey, FactQueryError};
    use crate::request::{CompilationOptions, CompilationRequest};
    use crate::test_support::{
        diagnostic_kinds, package_identity, source_callable_body_key,
        source_callable_body_key_from_symbols, source_input,
    };
    use crate::worker::WorkerBudget;

    use super::Compilation;

    #[test]
    fn empty_compilation_requests_produce_diagnostics() {
        let compilation =
            match Compilation::load(CompilationRequest::new(package_identity(), Vec::new())) {
                Ok(compilation) => compilation,
                Err(error) => panic!("empty requests should load with diagnostics: {error:?}"),
            };

        let diagnostic = bray_testing::single_diagnostic(compilation.source_diagnostics());

        assert_eq!(diagnostic.kind(), DiagnosticKind::RequestMissingSourceInput);

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::SourceCount,
                DiagnosticArgValue::SourceCount(0)
            )]
        );

        assert_eq!(
            diagnostic.notes(),
            &[DiagnosticNote::new(DiagnosticNoteKind::SourceInputRequired)]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.source_diagnostics(),
            DiagnosticKind::RequestMissingSourceInput,
        );
    }

    #[test]
    fn compilation_load_validates_package_source_authority() {
        let standard_library = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard library package identity must be valid"));

        let ordinary_request =
            CompilationRequest::new(standard_library, vec![source_input("module std;", 0)]);

        let ordinary = Compilation::load(ordinary_request)
            .unwrap_or_else(|error| panic!("ordinary request should load: {error:?}"));

        assert_eq!(
            diagnostic_kinds(ordinary.source_diagnostics()),
            [DiagnosticKind::RequestReservedPackageIdentity]
        );

        let standard_library = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard library package identity must be valid"));

        let standard_library_request =
            CompilationRequest::new(standard_library, vec![source_input("module std;", 0)])
                .with_standard_library_source_authority();

        let authorized = Compilation::load(standard_library_request)
            .unwrap_or_else(|error| panic!("authorized request should load: {error:?}"));

        assert!(authorized.source_diagnostics().is_empty());

        let invalid_authority = CompilationRequest::new(
            package_identity(),
            vec![source_input("module application;", 0)],
        )
        .with_standard_library_source_authority();

        let invalid = Compilation::load(invalid_authority)
            .unwrap_or_else(|error| panic!("invalid authority request should load: {error:?}"));

        assert_eq!(
            diagnostic_kinds(invalid.source_diagnostics()),
            [DiagnosticKind::RequestStandardLibraryPackageIdentityRequired]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            ordinary.source_diagnostics(),
            DiagnosticKind::RequestReservedPackageIdentity,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            invalid.source_diagnostics(),
            DiagnosticKind::RequestStandardLibraryPackageIdentityRequired,
        );
    }

    #[test]
    fn compilations_load_source_inputs_in_request_order() {
        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        );

        let request = CompilationRequest::with_options(
            package_identity(),
            vec![
                source_input("module first\n", 1),
                source_input("module second\n", 2),
            ],
            options.clone(),
        );

        let compilation = match Compilation::load(request) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        assert_eq!(compilation.options(), &options);
        assert_eq!(compilation.worker_budget(), WorkerBudget::serial());
        assert_eq!(compilation.source_count(), 2);

        assert!(compilation.source_diagnostics().is_empty());
        assert!(!compilation.is_empty());

        assert_eq!(
            compilation.source_text(SourceId::new(0)),
            Some("module first\n")
        );

        assert_eq!(
            compilation.source_text(SourceId::new(1)),
            Some("module second\n")
        );

        let first = match compilation.source(SourceId::new(0)) {
            Some(snapshot) => snapshot,
            None => panic!("first source should be loaded"),
        };

        assert_eq!(first.source_id(), SourceId::new(0));
        assert_eq!(first.origin().kind(), SourceOriginKind::Virtual);
        assert_eq!(first.version(), SourceVersion::new(1));
        assert_eq!(compilation.sources().len(), 2);
    }

    #[test]
    fn load_diagnostics_do_not_include_syntax_diagnostics() {
        let compilation =
            match Compilation::load_sources(package_identity(), vec![source_input("$", 0)]) {
                Ok(compilation) => compilation,
                Err(error) => panic!("source-only compilation should load: {error:?}"),
            };

        assert!(compilation.source_diagnostics().is_empty());

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
            ]
        );
    }

    #[test]
    fn misplaced_known_directives_flow_into_compiler_diagnostics() {
        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input(
                concat!(
                    "module app;\n",
                    "@link(name = \"native\")\n",
                    "struct NativeMutex;\n",
                    "func use(pos pointer: RawPointer<NativeMutex>) {}\n",
                ),
                0,
            )],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        let invalid_target = bray_testing::diagnostics_of_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::SyntaxInvalidDirectiveTarget,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_target,
            DiagnosticKind::SyntaxInvalidDirectiveTarget,
        );
    }

    #[test]
    fn check_diagnostics_request_syntax_and_declaration_semantics() {
        let invalid_utf8 = SourceInput::file_bytes(
            SourceIdentity::new(20),
            "bad-utf8.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let compilation = match Compilation::load(CompilationRequest::new(
            package_identity(),
            vec![source_input("$", 0), invalid_utf8],
        )) {
            Ok(compilation) => compilation,
            Err(error) => panic!("compilation should load: {error:?}"),
        };

        assert!(compilation.state.declaration_table_result.get().is_none());

        assert_eq!(compilation.syntax_tree().source_units().len(), 1);

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [
                DiagnosticKind::SourceInvalidUtf8,
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
            ]
        );

        assert!(compilation.state.declaration_table_result.get().is_some());
        assert!(compilation.declaration_diagnostics().is_empty());
    }

    #[test]
    fn compilations_store_utf8_source_load_diagnostics() {
        let invalid = SourceInput::file_bytes(
            SourceIdentity::new(10),
            "bad.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input("valid", 0), invalid],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("invalid UTF-8 should become a diagnostic: {error:?}"),
        };

        let diagnostics = compilation.source_diagnostics();

        assert_eq!(compilation.source_count(), 1);
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.len(), 1);

        let diagnostic = bray_testing::single_diagnostic(diagnostics);

        assert_eq!(diagnostic.id(), DiagnosticId::new(0));
        assert_eq!(diagnostic.kind(), DiagnosticKind::SourceInvalidUtf8);
        assert_eq!(diagnostic.severity(), SeverityKind::Error);
        assert_eq!(diagnostic.primary_span(), None);

        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::new(
                    DiagnosticArgName::TextOffset,
                    DiagnosticArgValue::TextOffset(TextSize::ZERO)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::ByteCount,
                    DiagnosticArgValue::ByteCount(1)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::SourceInput,
                    DiagnosticArgValue::SourceInput(DiagnosticSourceInput::new(
                        1,
                        SourceInputKind::File,
                        DiagnosticSourceInputOrigin::File("bad.bray".into())
                    ))
                )
            ]
        );

        assert_eq!(diagnostic.notes().len(), 1);

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::SourceInvalidUtf8,
        );
    }

    #[test]
    fn compilations_reject_duplicate_logical_source_inputs() {
        let first = SourceInput::virtual_text(
            SourceIdentity::new(10),
            "first.bray",
            SourceVersion::new(1),
            "module first;",
        );

        let duplicate = SourceInput::virtual_text(
            SourceIdentity::new(10),
            "duplicate.bray",
            SourceVersion::new(2),
            "module duplicate;",
        );

        let compilation = Compilation::load_sources(package_identity(), vec![first, duplicate])
            .unwrap_or_else(|error| {
                panic!("duplicate source input must become a diagnostic: {error:?}")
            });

        assert_eq!(compilation.source_count(), 1);

        let diagnostic = bray_testing::single_diagnostic(compilation.source_diagnostics());

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::RequestDuplicateSourceInput
        );

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::source_input(DiagnosticSourceInput::new(
                1,
                SourceInputKind::VirtualText,
                DiagnosticSourceInputOrigin::Name("duplicate.bray".to_owned()),
            ))]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.source_diagnostics(),
            DiagnosticKind::RequestDuplicateSourceInput,
        );
    }

    #[test]
    fn check_diagnostics_are_cached() {
        let compilation =
            match Compilation::load_sources(package_identity(), vec![source_input("$", 0)]) {
                Ok(compilation) => compilation,
                Err(error) => panic!("test compilation should load: {error:?}"),
            };

        let first = compilation.check_diagnostics();
        let second = compilation.check_diagnostics();

        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn selected_targets_are_snapshot_inputs_without_requesting_source_queries() {
        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input("module app;", 0)],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        assert!(compilation.state.syntax_tree_result.get().is_none());
        assert!(compilation.state.declaration_table_result.get().is_none());

        let first = compilation.available_compiler_known_symbols();
        let second = compilation.available_compiler_known_symbols();

        assert!(std::ptr::eq(first, second));

        assert_eq!(
            compilation
                .selected_target()
                .target()
                .integer_width_bits()
                .get(),
            64
        );

        assert!(compilation.state.syntax_tree_result.get().is_none());
        assert!(compilation.state.declaration_table_result.get().is_none());

        let graph = match compilation.symbol_graph() {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        assert!(std::ptr::eq(
            first.provider(),
            graph.compiler_known_provider()
        ));

        assert!(first.provider().declaration_symbols().len() > first.declarations().len());

        for key in [
            "TargetReal16",
            "TargetReal128",
            "TargetComplex32",
            "TargetComplex256",
        ] {
            assert_eq!(
                first.declaration_symbol::<StructSymbolId>(&declaration_key(key)),
                None
            );
        }

        assert!(
            first
                .declaration_symbol::<FunctionSymbolId>(&declaration_key("MemoryCopy"))
                .is_some()
        );
    }

    #[test]
    fn concurrent_selected_target_requests_share_one_immutable_context() {
        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input("module app;", 0)],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        let pointers = std::thread::scope(|scope| {
            let requests = (0..4)
                .map(|_| scope.spawn(|| std::ptr::from_ref(compilation.selected_target()) as usize))
                .collect::<Vec<_>>();

            requests
                .into_iter()
                .map(|request| match request.join() {
                    Ok(pointer) => pointer,
                    Err(_) => panic!("selected target request must not panic"),
                })
                .collect::<Vec<_>>()
        });

        assert!(pointers.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn compilations_publish_independent_views_for_their_target_properties() {
        let portable = compilation_with_optional_real16(false);
        let real16 = compilation_with_optional_real16(true);

        let key = declaration_key("TargetReal16");

        assert_eq!(
            portable
                .available_compiler_known_symbols()
                .declaration_symbol::<StructSymbolId>(&key),
            None
        );

        assert!(
            real16
                .available_compiler_known_symbols()
                .declaration_symbol::<StructSymbolId>(&key)
                .is_some()
        );

        assert_eq!(
            portable
                .available_compiler_known_symbols()
                .provider()
                .declaration_symbols()
                .len(),
            real16
                .available_compiler_known_symbols()
                .provider()
                .declaration_symbols()
                .len()
        );
    }

    #[test]
    fn source_unit_syntax_is_cached_by_source_id() {
        let compilation =
            match Compilation::load_sources(package_identity(), vec![source_input("$", 0)]) {
                Ok(compilation) => compilation,
                Err(error) => panic!("test compilation should load: {error:?}"),
            };

        let first_syntax = match compilation.source_unit_syntax(SourceId::new(0)) {
            Some(result) => result,
            None => panic!("source should have syntax"),
        };

        let second_syntax = match compilation.source_unit_syntax(SourceId::new(0)) {
            Some(result) => result,
            None => panic!("source should have syntax again"),
        };

        assert!(std::ptr::eq(first_syntax, second_syntax));
    }

    #[test]
    fn declaration_chunks_are_lazy_and_cached_by_source_id() {
        let compilation = declaration_compilation();

        let [first_syntax_cache, second_syntax_cache] =
            compilation.state.source_unit_syntax.as_slice()
        else {
            panic!("expected two source-unit syntax caches");
        };

        let [first_declaration_cache, second_declaration_cache] =
            compilation.state.declaration_chunks.as_slice()
        else {
            panic!("expected two declaration chunk caches");
        };

        assert!(
            compilation
                .state
                .source_unit_syntax
                .iter()
                .all(|cache| cache.get().is_none())
        );

        assert!(
            compilation
                .state
                .declaration_chunks
                .iter()
                .all(|cache| cache.get().is_none())
        );

        assert!(compilation.state.declaration_table_result.get().is_none());

        let first = match compilation.declaration_chunk(SourceId::new(0)) {
            Some(result) => result,
            None => panic!("first source should have a declaration chunk"),
        };

        assert!(first.diagnostics().is_empty());
        assert_eq!(first.chunk().module_parts().len(), 1);

        assert!(first_syntax_cache.get().is_some());
        assert!(second_syntax_cache.get().is_none());

        assert!(first_declaration_cache.get().is_some());
        assert!(second_declaration_cache.get().is_none());

        assert!(compilation.state.declaration_table_result.get().is_none());

        assert_eq!(
            compilation
                .state
                .fact_runtime
                .dependencies(&CompilationFactKey::DeclarationChunk(SourceId::new(0))),
            Ok(Some(
                vec![CompilationFactKey::SourceUnitSyntax(SourceId::new(0))].into_boxed_slice()
            ))
        );

        let second = match compilation.declaration_chunk(SourceId::new(0)) {
            Some(result) => result,
            None => panic!("first source should have the same declaration chunk"),
        };

        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn declaration_table_query_requests_all_chunks_and_is_cached() {
        let compilation = declaration_compilation();

        let first = compilation.declaration_table_result();
        let second = compilation.declaration_table_result();

        assert!(std::ptr::eq(first, second));
        assert!(first.diagnostics().is_empty());

        assert!(
            compilation
                .state
                .declaration_chunks
                .iter()
                .all(|cache| cache.get().is_some())
        );

        assert!(compilation.state.check_diagnostics.get().is_none());

        assert_eq!(
            compilation
                .state
                .fact_runtime
                .dependencies(&CompilationFactKey::DeclarationTable),
            Ok(Some(
                vec![
                    CompilationFactKey::DeclarationChunk(SourceId::new(0)),
                    CompilationFactKey::DeclarationChunk(SourceId::new(1)),
                ]
                .into_boxed_slice()
            ))
        );

        assert!(std::ptr::eq(first.table(), compilation.declaration_table()));

        assert!(std::ptr::eq(
            first.diagnostics(),
            compilation.declaration_diagnostics()
        ));

        let module = match first.table().module_container(&ModulePath::new(["core"])) {
            Some(module) => module,
            None => panic!("core module should exist"),
        };

        assert_eq!(module.module_parts().len(), 2);

        assert_eq!(
            module
                .declarations()
                .iter()
                .map(|id| match first.table().declaration(*id) {
                    Some(record) => record.kind(),
                    None => panic!("module declaration ID should exist: {id:?}"),
                })
                .collect::<Vec<_>>(),
            [DeclarationKind::Function, DeclarationKind::Constant]
        );
    }

    #[test]
    fn declaration_diagnostics_flow_into_check_diagnostics() {
        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input(
                "module core; struct Point {} struct Point {}",
                0,
            )],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        assert_eq!(
            diagnostic_kinds(compilation.declaration_diagnostics()),
            [DiagnosticKind::DeclarationDuplicateName]
        );

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [DiagnosticKind::DeclarationDuplicateName]
        );
    }

    #[test]
    fn bound_and_control_flow_results_share_nested_unit_identity() {
        let compilation = checked_body_compilation();
        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound callable query must complete: {error:?}"),
        };

        let nested = bound.value().nested_units();

        assert_eq!(nested.len(), 2);
        assert!(nested[0].source() < nested[1].source());

        let mut recovery_states = Vec::new();

        for nested_key in nested {
            let child = match compilation.control_flow(nested_key.clone()) {
                Ok(child) => child,
                Err(error) => panic!("nested control-flow analysis must be available: {error:?}"),
            };

            recovery_states.push(child.value().is_recovered());
        }

        assert_eq!(recovery_states, [false, false]);

        let parent = match compilation.control_flow(key) {
            Ok(parent) => parent,
            Err(error) => panic!("parent control-flow analysis must complete: {error:?}"),
        };

        assert_eq!(parent.value().kind(), BoundUnitKind::CallableBody);
    }

    #[test]
    fn target_independent_control_flow_does_not_request_selected_target_properties() {
        let compilation = checked_body_compilation();
        let key = source_callable_body_key(&compilation);

        let result = compilation.control_flow(key.clone());

        assert!(result.is_ok());

        let dependencies = match compilation
            .state
            .fact_runtime
            .input_dependencies(&CompilationFactKey::CheckedControlFlow(key))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("checked control flow must publish its dependencies"),
            Err(error) => panic!("checked control-flow dependencies must be readable: {error:?}"),
        };

        assert!(!dependencies.contains(&CompilationInputKey::SelectedTarget));
    }

    #[test]
    fn declaration_owned_expression_queries_use_the_same_finalization_path() {
        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input("module app; const size: i32 = 1;", 0)],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("constant compilation must load: {error:?}"),
        };

        let key = source_constant_template_key(&compilation);

        let checked = match compilation.control_flow(key) {
            Ok(checked) => checked,
            Err(error) => panic!("constant template query must complete: {error:?}"),
        };

        assert_eq!(checked.value().kind(), BoundUnitKind::ConstantTemplate);
    }

    #[test]
    fn foreign_unit_keys_cannot_publish_into_a_compilation_cache() {
        let compilation = checked_body_compilation();

        let Some(foreign_package) = PackageIdentity::try_new("foreign.package") else {
            panic!("foreign test package identity must be valid");
        };

        let foreign_symbols = match SymbolGraph::build_source(
            foreign_package,
            compilation.declaration_table(),
            compilation.syntax_tree(),
        ) {
            Ok(symbols) => symbols,
            Err(error) => panic!("foreign test symbol graph must build: {error:?}"),
        };

        let foreign_key = source_callable_body_key_from_symbols(&compilation, &foreign_symbols);

        let foreign = compilation.control_flow(foreign_key.clone());

        assert!(
            matches!(
                &foreign,
                Err(FactQueryError::SemanticQuery(error))
                    if error.cause() == &SemanticQueryFailure::contract(
                        SemanticQueryContext::Unit(foreign_key.clone()),
                        SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
                    )
            ),
            "{foreign:?}"
        );

        assert_eq!(
            compilation
                .state
                .checked_control_flow
                .is_published(&foreign_key),
            Ok(false)
        );

        let local_key = source_callable_body_key(&compilation);
        let local = compilation.control_flow(local_key);

        assert!(local.is_ok());
    }

    #[test]
    fn bound_unit_ids_ignore_serial_reversed_and_parallel_demand_order() {
        let first = unit_identity_compilation();
        let first_callable = source_callable_body_key(&first);
        let first_constant = source_constant_template_key(&first);

        let first_callable_id = unit_id(&first, &first_callable);
        let first_constant_id = unit_id(&first, &first_constant);

        let reversed = unit_identity_compilation();
        let reversed_callable = source_callable_body_key(&reversed);
        let reversed_constant = source_constant_template_key(&reversed);

        let reversed_constant_id = unit_id(&reversed, &reversed_constant);
        let reversed_callable_id = unit_id(&reversed, &reversed_callable);

        assert_eq!(first_callable_id, reversed_callable_id);
        assert_eq!(first_constant_id, reversed_constant_id);

        let parallel = unit_identity_compilation();
        let parallel_callable = source_callable_body_key(&parallel);
        let parallel_constant = source_constant_template_key(&parallel);

        let (parallel_callable_id, parallel_constant_id) = std::thread::scope(|scope| {
            let callable = scope.spawn(|| unit_id(&parallel, &parallel_callable));
            let constant = scope.spawn(|| unit_id(&parallel, &parallel_constant));

            (join_unit_id(callable), join_unit_id(constant))
        });

        assert_eq!(first_callable_id, parallel_callable_id);
        assert_eq!(first_constant_id, parallel_constant_id);
    }

    #[test]
    fn check_diagnostics_merge_available_phase_diagnostics() {
        let invalid = SourceInput::file_bytes(
            SourceIdentity::new(11),
            "bad.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input("$", 0), invalid],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("invalid UTF-8 should become a diagnostic: {error:?}"),
        };

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [
                DiagnosticKind::SourceInvalidUtf8,
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
            ]
        );
    }

    #[test]
    fn compilations_are_send_and_sync() {
        assert_send_sync::<Compilation>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn declaration_compilation() -> Compilation {
        match Compilation::load_sources(
            package_identity(),
            vec![
                source_input("module core; func first() {}", 0),
                source_input("module core { const Size: Int = 1; }", 1),
            ],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        }
    }

    fn checked_body_compilation() -> Compilation {
        let source = concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let first = lambda()\n",
            "    {\n",
            "        missing;\n",
            "    };\n",
            "    let second = lambda()\n",
            "    {\n",
            "    };\n",
            "}\n",
        );

        match Compilation::load_sources(package_identity(), vec![source_input(source, 0)]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("checked-body compilation must load: {error:?}"),
        }
    }

    fn unit_identity_compilation() -> Compilation {
        let source = concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        );

        match Compilation::load_sources(package_identity(), vec![source_input(source, 0)]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("unit identity compilation must load: {error:?}"),
        }
    }

    fn unit_id(compilation: &Compilation, key: &BoundUnitKey) -> bray_bound_tree::BoundUnitId {
        match compilation.bound_unit_id(key) {
            Ok(unit) => unit,
            Err(error) => panic!("test unit ID must be available: {error:?}"),
        }
    }

    fn join_unit_id(
        handle: std::thread::ScopedJoinHandle<'_, bray_bound_tree::BoundUnitId>,
    ) -> bray_bound_tree::BoundUnitId {
        match handle.join() {
            Ok(unit) => unit,
            Err(_) => panic!("unit identity worker must not panic"),
        }
    }

    fn source_constant_template_key(compilation: &Compilation) -> BoundUnitKey {
        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        let Some(constant) = symbols
            .constants()
            .iter()
            .find(|constant| constant.origin() == SymbolOrigin::Source)
        else {
            panic!("test symbol graph must contain one source constant");
        };

        let [source_unit] = compilation.syntax_tree().source_units() else {
            panic!("constant compilation must contain one source unit");
        };

        let Some(declaration) = source_unit.constant_declarations().next() else {
            panic!("constant source unit must contain one declaration");
        };

        let Some(expression) = declaration.expression() else {
            panic!("constant declaration must contain an expression");
        };

        let source = BoundSourceAnchor::new(
            bray_declarations::SyntaxAnchor::from_node(&expression),
            expression.source().version(),
        );

        match BoundUnitKey::constant_template(constant.key().clone(), source) {
            Some(key) => key,
            None => panic!("source constant must own a constant template"),
        }
    }

    fn compilation_with_optional_real16(real16: bool) -> Compilation {
        let baseline = crate::SelectedTarget::baseline();
        let profile = baseline.profile();
        let baseline_properties = profile.properties();

        let properties = bray_target::TargetProperties::new(
            baseline_properties.identity().clone(),
            bray_target::TargetScalarSupport::new(real16, false, false, false),
            baseline_properties.atomics(),
            baseline_properties.abis(),
            baseline_properties.c_abi(),
            baseline_properties.address_spaces(),
            baseline_properties.alignments(),
            baseline_properties.operations(),
        );

        let profile = match bray_target::TargetProfile::try_new(
            profile.identity().clone(),
            profile.machine().clone(),
            properties,
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("test target profile must be valid: {error:?}"),
        };

        let target = crate::SelectedTarget::new(profile, baseline.runtime_abi());

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        );

        let request = CompilationRequest::with_options(
            package_identity(),
            vec![source_input("module app;", 0)],
            options,
        );

        match Compilation::load(request) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        }
    }

    fn declaration_key(value: &str) -> CompilerKnownDeclarationKey {
        match CompilerKnownDeclarationKey::try_new(value) {
            Some(key) => key,
            None => panic!("test compiler-known declaration key should be valid"),
        }
    }
}
