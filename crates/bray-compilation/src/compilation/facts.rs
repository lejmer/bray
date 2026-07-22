use std::fmt;
use std::sync::Arc;

use bray_binder::BinderDependency;
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, CheckedControlFlowFacts, CheckedExpressionTypes,
    CheckedSemanticSelections, DeclaredValueTypeTemplates,
};
use bray_checker::{TargetValidity, TargetValidityRequest};
use bray_declarations::{
    DeclarationChunkResult, DeclarationTable, DeclarationTableResult,
    discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::{
    ImportedSemanticFact, ImportedSemanticFacts, PackageInterfaceExportBundle,
};
use bray_parser::{SourceUnitSyntaxResult, SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceId, SourceInput, SourceLoadError, SourceSnapshot, SourceStore};
use bray_symbols::{
    AvailableCompilerKnownSymbols, CompilerKnownSymbolBuildError, CompilerKnownSymbolProvider,
    ConstantInstanceValueFact, ConstantTermId, ImplementationCandidateSet,
    ImplementationCoherenceDomainKey, ImplementationParticipationFact,
    ImplementationRequirementKey, ImportedSymbolSkeleton, PackageIdentity, SemanticFactResult,
    SemanticValueStore, SemanticValueStoreCreateError, SymbolGraph,
};
use bray_syntax::SyntaxTree;

use crate::fact::{
    BoundUnitIdentityMap, CancellationToken, CompilationFactKey, ConstantInstanceFactKey, FactCell,
    FactCellMap, FactQueryError, FactRuntime, ImportedSemanticFactKey, PublishedUnitFact,
    UnitFactCache,
};
use crate::request::{
    CompilationOptions, CompilationRequest, DependencyInterfaceInput, PackageInterfaceExportRequest,
};
use crate::worker::WorkerBudget;

use super::binder::CompilationSymbolFacts;
use super::load::{
    CompilationLoadError, SourceInputDiagnosticContext, missing_source_input_diagnostic,
    next_diagnostic_id, source_load_diagnostic,
};

pub(super) type CheckedExpressionSemantics = (CheckedExpressionTypes, CheckedSemanticSelections);

/// Durable immutable compilation context and demand-driven fact entrypoint.
#[derive(Clone)]
pub struct Compilation {
    pub(super) state: Arc<CompilationState>,
}

pub(super) struct CompilationState {
    package_identity: PackageIdentity,
    options: CompilationOptions,
    sources: SourceStore,
    source_diagnostics: DiagnosticBag,
    pub(super) package_interface_export: Option<PackageInterfaceExportRequest>,
    pub(super) dependency_interfaces: Box<[DependencyInterfaceInput]>,
    pub(super) fact_runtime: FactRuntime,
    pub(super) cancellation: CancellationToken,
    source_unit_syntax: Vec<FactCell<SourceUnitSyntaxResult>>,
    syntax_tree_result: FactCell<SyntaxTreeResult>,
    declaration_chunks: Vec<FactCell<DeclarationChunkResult>>,
    declaration_table_result: FactCell<DeclarationTableResult>,
    compiler_known_symbols:
        FactCell<Result<Arc<CompilerKnownSymbolProvider>, CompilerKnownSymbolBuildError>>,
    selected_target: FactCell<crate::SelectedTargetContext>,
    pub(super) target_validity:
        FactCellMap<TargetValidityRequest, Arc<bray_diagnostics::DiagnosticResult<TargetValidity>>>,
    bound_unit_identities: FactCell<Result<BoundUnitIdentityMap, FactQueryError>>,
    symbol_graph: FactCell<Result<SymbolGraph, FactQueryError>>,
    semantic_values: FactCell<Result<SemanticValueStore, SemanticValueStoreCreateError>>,
    pub(super) loaded_dependency_interfaces:
        Vec<FactCell<super::imported::LoadedDependencyInterface>>,
    pub(super) imported_symbol_skeleton:
        FactCell<DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>>,
    pub(super) imported_semantic_graphs:
        Vec<FactCell<bray_diagnostics::DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>>>,
    pub(super) imported_semantic_facts: FactCellMap<
        ImportedSemanticFactKey,
        Arc<bray_diagnostics::DiagnosticResult<Arc<[ImportedSemanticFact]>>>,
    >,
    pub(super) imported_diagnostics: FactCell<DiagnosticBag>,
    pub(super) implementation_participation: FactCellMap<
        ImplementationCoherenceDomainKey,
        Arc<SemanticFactResult<ImplementationParticipationFact>>,
    >,
    pub(super) implementation_index:
        FactCell<DiagnosticResult<Arc<super::implementation::ImplementationHeaderIndex>>>,
    pub(super) implementation_candidate_sets: FactCellMap<
        ImplementationRequirementKey,
        Arc<DiagnosticResult<ImplementationCandidateSet>>,
    >,
    pub(super) semantic_diagnostics: FactCell<DiagnosticBag>,
    pub(super) symbol_facts: CompilationSymbolFacts,
    pub(super) bound_units: UnitFactCache<BoundUnit>,
    pub(super) declared_value_type_templates: UnitFactCache<DeclaredValueTypeTemplates>,
    pub(super) checked_control_flow: UnitFactCache<CheckedControlFlowFacts>,
    pub(super) expression_semantics: UnitFactCache<CheckedExpressionSemantics>,
    pub(super) checked_expression_types: UnitFactCache<CheckedExpressionTypes>,
    pub(super) checked_semantic_selections: UnitFactCache<CheckedSemanticSelections>,
    pub(super) symbolic_constant_terms: UnitFactCache<ConstantTermId>,
    pub(super) constant_instances:
        FactCellMap<ConstantInstanceFactKey, Arc<SemanticFactResult<ConstantInstanceValueFact>>>,
    pub(super) check_diagnostics: FactCell<DiagnosticBag>,
    pub(super) package_interface_export_bundle: FactCell<
        Result<Arc<PackageInterfaceExportBundle>, super::export::PackageInterfaceExportError>,
    >,
}

impl Compilation {
    /// Loads source inputs into durable compilation state without requesting derived facts.
    pub fn load(request: CompilationRequest) -> Result<Self, CompilationLoadError> {
        let (
            package_identity,
            options,
            source_inputs,
            mut dependency_interfaces,
            package_interface_export,
        ) = request.into_parts();

        dependency_interfaces.sort_by(|left, right| {
            (left.package(), left.product()).cmp(&(right.package(), right.product()))
        });

        let mut sources = SourceStore::with_capacity(source_inputs.len());
        let mut diagnostics = DiagnosticBag::new();

        if source_inputs.is_empty() {
            diagnostics.add(missing_source_input_diagnostic(next_diagnostic_id(
                &diagnostics,
            )?));
        }

        for (source_index, source_input) in source_inputs.into_iter().enumerate() {
            // Preserve request-boundary metadata before handing ownership to
            // the loader so source-load diagnostics can identify the input.
            let diagnostic_context =
                SourceInputDiagnosticContext::from_input(source_index, &source_input);

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

        let source_count = sources.len();
        let dependency_count = dependency_interfaces.len();

        Ok(Self {
            state: Arc::new(CompilationState {
                package_identity,
                options,
                sources,
                source_diagnostics: diagnostics,
                package_interface_export,
                dependency_interfaces: dependency_interfaces.into_boxed_slice(),
                fact_runtime: FactRuntime::default(),
                cancellation: CancellationToken::new(),
                source_unit_syntax: empty_fact_caches(source_count),
                syntax_tree_result: FactCell::new(),
                declaration_chunks: empty_fact_caches(source_count),
                declaration_table_result: FactCell::new(),
                compiler_known_symbols: FactCell::new(),
                selected_target: FactCell::new(),
                target_validity: FactCellMap::new(),
                bound_unit_identities: FactCell::new(),
                symbol_graph: FactCell::new(),
                semantic_values: FactCell::new(),
                loaded_dependency_interfaces: empty_fact_caches(dependency_count),
                imported_symbol_skeleton: FactCell::new(),
                imported_semantic_graphs: empty_fact_caches(dependency_count),
                imported_semantic_facts: FactCellMap::new(),
                imported_diagnostics: FactCell::new(),
                implementation_participation: FactCellMap::new(),
                implementation_index: FactCell::new(),
                implementation_candidate_sets: FactCellMap::new(),
                semantic_diagnostics: FactCell::new(),
                symbol_facts: CompilationSymbolFacts::new(),
                bound_units: UnitFactCache::new(),
                declared_value_type_templates: UnitFactCache::new(),
                checked_control_flow: UnitFactCache::new(),
                expression_semantics: UnitFactCache::new(),
                checked_expression_types: UnitFactCache::new(),
                checked_semantic_selections: UnitFactCache::new(),
                symbolic_constant_terms: UnitFactCache::new(),
                constant_instances: FactCellMap::new(),
                check_diagnostics: FactCell::new(),
                package_interface_export_bundle: FactCell::new(),
            }),
        })
    }

    /// Loads one source package with default options without requesting derived facts.
    pub fn load_sources(
        package_identity: PackageIdentity,
        sources: Vec<SourceInput>,
    ) -> Result<Self, CompilationLoadError> {
        Self::load(CompilationRequest::new(package_identity, sources))
    }

    /// Returns the source package identity selected for this compilation.
    pub fn package_identity(&self) -> &PackageIdentity {
        &self.state.package_identity
    }

    /// Returns the compilation options.
    pub fn options(&self) -> &CompilationOptions {
        &self.state.options
    }

    /// Returns the compiler-owned CPU worker budget.
    pub fn worker_budget(&self) -> WorkerBudget {
        self.state.options.worker_budget()
    }

    /// Returns the selected target and its available compiler-known declarations.
    pub fn selected_target(&self) -> &crate::SelectedTargetContext {
        self.fact(
            CompilationFactKey::SelectedTarget,
            &self.state.selected_target,
            || {
                let provider = match self.compiler_known_provider() {
                    Ok(provider) => Arc::clone(provider),
                    // Generated catalog validation makes provider failure a compiler invariant.
                    Err(error) => panic!("compiler-known symbol provider is invalid: {error:?}"),
                };

                // The published fact retains its target independently of request options.
                let target = self.state.options.selected_target().clone();
                let available = provider.available_symbols(|rule| target.supports(rule));

                crate::SelectedTargetContext::new(target, available)
            },
        )
    }

    /// Returns the compiler-known symbols available for the selected target.
    pub fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.selected_target().available_compiler_known_symbols()
    }

    /// Returns the loaded source snapshots.
    pub fn sources(&self) -> &SourceStore {
        &self.state.sources
    }

    /// Returns diagnostics produced while loading source inputs.
    pub fn source_diagnostics(&self) -> &DiagnosticBag {
        &self.state.source_diagnostics
    }

    /// Returns the syntax result for one source unit.
    pub fn source_unit_syntax(&self, source_id: SourceId) -> Option<&SourceUnitSyntaxResult> {
        let snapshot = self.source(source_id)?;
        let cache = self.state.source_unit_syntax.get(source_id.to_index()?)?;

        Some(self.fact(
            CompilationFactKey::SourceUnitSyntax(source_id),
            cache,
            || parse_source_unit(snapshot),
        ))
    }

    /// Returns the syntax tree result for all loaded source units.
    pub fn syntax_tree_result(&self) -> &SyntaxTreeResult {
        self.fact(
            CompilationFactKey::SyntaxTree,
            &self.state.syntax_tree_result,
            || {
                let source_units = self.sources().iter().map(|snapshot| {
                    // Source stores and fact caches share the loaded source count.
                    // The whole-source syntax result owns the composed source units.
                    match self.source_unit_syntax(snapshot.source_id()) {
                        Some(result) => result.clone(),
                        None => panic!("source unit fact cache should match source store"),
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

        Some(self.fact(
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
        self.fact(
            CompilationFactKey::DeclarationTable,
            &self.state.declaration_table_result,
            || {
                let chunks = self.sources().iter().map(|snapshot| {
                    match self.declaration_chunk(snapshot.source_id()) {
                        Some(chunk) => chunk,
                        None => panic!("declaration chunk cache should match source store"),
                    }
                });

                merge_declaration_chunks(chunks)
            },
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
        self.fact(
            CompilationFactKey::SymbolGraph,
            &self.state.symbol_graph,
            || {
                let provider = self.compiler_known_provider().map(Arc::clone)?;

                // The graph owns the Arc-backed package identity after compilation retains its input.
                SymbolGraph::build_source_with_provider(
                    self.package_identity().clone(),
                    self.declaration_table(),
                    self.syntax_tree(),
                    provider,
                )
                .map_err(|_| FactQueryError::InfrastructureFailure)
            },
        )
        .as_ref()
        .map_err(Clone::clone)
    }

    /// Returns the canonical semantic value store for this compilation snapshot.
    pub fn semantic_value_store(&self) -> Result<&SemanticValueStore, FactQueryError> {
        self.fact(
            CompilationFactKey::SemanticValueStore,
            &self.state.semantic_values,
            SemanticValueStore::try_new,
        )
        .as_ref()
        .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    fn compiler_known_provider(&self) -> Result<&Arc<CompilerKnownSymbolProvider>, FactQueryError> {
        self.fact(
            CompilationFactKey::CompilerKnownSymbols,
            &self.state.compiler_known_symbols,
            || CompilerKnownSymbolProvider::build().map(Arc::new),
        )
        .as_ref()
        .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    pub(super) fn bound_unit_id(
        &self,
        key: &BoundUnitKey,
    ) -> Result<bray_bound_tree::BoundUnitId, FactQueryError> {
        self.fact(
            CompilationFactKey::BoundUnitIdentities,
            &self.state.bound_unit_identities,
            || BoundUnitIdentityMap::from_syntax(self.syntax_tree()),
        )
        .as_ref()
        .map_err(Clone::clone)?
        .unit_id(key)
    }

    /// Returns the loaded source snapshot for `source_id`.
    pub fn source(&self, source_id: SourceId) -> Option<&SourceSnapshot> {
        self.state.sources.get(source_id)
    }

    /// Returns the loaded source text for `source_id`.
    pub fn source_text(&self, source_id: SourceId) -> Option<&str> {
        self.state.sources.text(source_id)
    }

    /// Returns the number of loaded source snapshots.
    pub fn source_count(&self) -> usize {
        self.state.sources.len()
    }

    /// Returns whether this compilation has no source snapshots.
    pub fn is_empty(&self) -> bool {
        self.state.sources.is_empty()
    }

    pub(super) fn fact<'a, T>(
        &self,
        key: CompilationFactKey,
        cache: &'a FactCell<T>,
        compute: impl FnOnce() -> T,
    ) -> &'a T {
        match cache.get_or_compute(
            &self.state.fact_runtime,
            key,
            &self.state.cancellation,
            || Ok(compute()),
        ) {
            Ok(value) => value,
            Err(FactQueryError::Cancelled) => {
                panic!("uncancellable compilation fact was unexpectedly cancelled")
            }
            Err(FactQueryError::Cycle(cycle)) => {
                panic!("acyclic compilation fact dependency formed a cycle: {cycle:?}")
            }
            Err(FactQueryError::InfrastructureFailure) => {
                panic!("compilation fact infrastructure failed")
            }
            Err(FactQueryError::SemanticUnitContext(error)) => {
                panic!("semantic semantic unit context failed: {error:?}")
            }
            Err(FactQueryError::CheckerInfrastructure(error)) => {
                panic!("semantic checker infrastructure failed: {error:?}")
            }
        }
    }

    pub(super) fn query_fact_with_cancellation<'a, T>(
        &self,
        key: CompilationFactKey,
        cache: &'a FactCell<T>,
        cancellation: &CancellationToken,
        compute: impl FnOnce(&CancellationToken) -> Result<T, FactQueryError>,
    ) -> Result<&'a T, FactQueryError> {
        cache.get_or_compute(&self.state.fact_runtime, key, cancellation, || {
            compute(cancellation)
        })
    }

    pub(super) fn unit_fact<T>(
        &self,
        cache: &UnitFactCache<T>,
        fact_key: CompilationFactKey,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        compute: impl FnOnce(
            &CancellationToken,
        )
            -> Result<(DiagnosticResult<T>, Box<[BinderDependency]>), FactQueryError>,
    ) -> Result<Arc<PublishedUnitFact<T>>, FactQueryError> {
        cache.get_or_compute(
            &self.state.fact_runtime,
            cancellation,
            fact_key,
            key,
            || compute(cancellation),
        )
    }
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

fn empty_fact_caches<T>(len: usize) -> Vec<FactCell<T>> {
    (0..len).map(|_| FactCell::new()).collect()
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey, BoundUnitKind};
    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_declarations::{DeclarationKind, ModulePath};
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId, DiagnosticKind,
        DiagnosticNote, DiagnosticNoteKind, SeverityKind,
    };
    use bray_source::{
        SourceId, SourceIdentity, SourceInput, SourceInputKind, SourceOriginKind, SourceVersion,
        TextSize,
    };
    use bray_symbols::{
        FunctionSymbolId, PackageIdentity, StructSymbolId, SymbolGraph, SymbolOrigin,
    };
    use bray_syntax::SourceSyntaxNode;

    use crate::fact::{CompilationFactKey, FactQueryError};
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

        let diagnostic = match compilation.source_diagnostics().diagnostics() {
            [diagnostic] => diagnostic,
            diagnostics => panic!("expected one diagnostic: {diagnostics:?}"),
        };

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
    }

    #[test]
    fn compilations_load_source_inputs_in_request_order() {
        let options =
            CompilationOptions::new(WorkerBudget::serial(), crate::SelectedTarget::baseline());

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
    fn check_diagnostics_request_syntax_and_declaration_facts() {
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

        let diagnostic = match diagnostics.diagnostics() {
            [diagnostic] => diagnostic,
            diagnostics => panic!("expected one diagnostic: {diagnostics:?}"),
        };

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
                    DiagnosticArgName::InputIndex,
                    DiagnosticArgValue::InputIndex(1)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::SourceInputKind,
                    DiagnosticArgValue::SourceInputKind(SourceInputKind::File)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::FilePath,
                    DiagnosticArgValue::FilePath("bad.bray".into())
                )
            ]
        );

        assert_eq!(diagnostic.notes().len(), 1);
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
    fn selected_targets_are_lazy_cached_without_requesting_source_facts() {
        let compilation = match Compilation::load_sources(
            package_identity(),
            vec![source_input("module app;", 0)],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        assert!(compilation.state.selected_target.get().is_none());
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

        assert!(compilation.state.selected_target.get().is_some());
        assert!(compilation.state.syntax_tree_result.get().is_none());
        assert!(compilation.state.declaration_table_result.get().is_none());

        assert_eq!(
            compilation
                .state
                .fact_runtime
                .dependencies(&CompilationFactKey::SelectedTarget),
            Ok(Some(
                vec![CompilationFactKey::CompilerKnownSymbols].into_boxed_slice()
            ))
        );

        let graph = match compilation.symbol_graph() {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        assert!(std::ptr::eq(
            first.provider(),
            graph.compiler_known_provider()
        ));

        assert_eq!(
            first.provider().declaration_symbols().len(),
            first.declarations().len() + 5
        );

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

        assert_eq!(
            first.declaration_symbol::<FunctionSymbolId>(&declaration_key("MemoryCopy")),
            None
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
    fn compilations_publish_independent_views_for_their_target_facts() {
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
    fn source_unit_syntax_facts_are_cached_by_source_id() {
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
    fn declaration_chunk_facts_are_lazy_and_cached_by_source_id() {
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
    fn declaration_table_fact_requests_all_chunks_and_is_cached() {
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
    fn bound_and_control_flow_facts_share_nested_unit_identity() {
        let compilation = checked_body_compilation();
        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound callable query must complete: {error:?}"),
        };

        let nested = bound.value().nested_units();

        assert_eq!(nested.len(), 2);
        assert!(nested[0].source() < nested[1].source());

        let mut recovered = Vec::new();

        for nested_key in nested {
            let child = match compilation.checked_control_flow(nested_key.clone()) {
                Ok(child) => child,
                Err(error) => panic!("nested control-flow fact must be available: {error:?}"),
            };

            recovered.push(child.value().is_recovered());
        }

        assert_eq!(recovered, [true, false]);

        let parent = match compilation.checked_control_flow(key) {
            Ok(parent) => parent,
            Err(error) => panic!("parent control-flow fact must complete: {error:?}"),
        };

        assert_eq!(parent.value().kind(), BoundUnitKind::CallableBody);
    }

    #[test]
    fn target_independent_control_flow_does_not_request_selected_target_facts() {
        let compilation = checked_body_compilation();
        let key = source_callable_body_key(&compilation);

        let result = compilation.checked_control_flow(key.clone());

        assert!(result.is_ok());

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&CompilationFactKey::CheckedControlFlow(key))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("checked control flow must publish its dependencies"),
            Err(error) => panic!("checked control-flow dependencies must be readable: {error:?}"),
        };

        assert!(!dependencies.contains(&CompilationFactKey::SelectedTarget));
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

        let checked = match compilation.checked_control_flow(key) {
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

        let foreign = compilation.checked_control_flow(foreign_key.clone());

        assert!(matches!(
            foreign,
            Err(FactQueryError::InfrastructureFailure)
        ));

        assert_eq!(
            compilation
                .state
                .checked_control_flow
                .is_published(&foreign_key),
            Ok(false)
        );

        let local_key = source_callable_body_key(&compilation);
        let local = compilation.checked_control_flow(local_key);

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
        let baseline_facts = profile.facts();

        let facts = bray_target::TargetFacts::new(
            baseline_facts.identity().clone(),
            bray_target::TargetScalarFacts::new(real16, false, false, false),
            baseline_facts.atomics(),
            baseline_facts.abis(),
            baseline_facts.address_spaces(),
            baseline_facts.alignments(),
            baseline_facts.operations(),
        );

        let profile = match bray_target::TargetProfile::try_new(
            profile.identity().clone(),
            profile.machine().clone(),
            facts,
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("test target profile must be valid: {error:?}"),
        };

        let target = crate::SelectedTarget::new(profile, baseline.runtime_abi());
        let options = CompilationOptions::new(WorkerBudget::serial(), target);

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
