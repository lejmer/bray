use std::sync::Arc;

use bray_binder::CheckedUnitComputation;
use bray_bound_tree::BoundUnitKey;
use bray_declarations::{
    DeclarationChunkResult, DeclarationTable, DeclarationTableResult,
    discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_diagnostics::DiagnosticBag;
use bray_parser::{SourceUnitSyntaxResult, SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceId, SourceInput, SourceLoadError, SourceSnapshot, SourceStore};
use bray_symbols::{AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider};
use bray_syntax::SyntaxTree;

use crate::fact::{
    CancellationToken, CheckedUnitFact, CheckedUnitFactCaches, CompilationFactKey, FactCell,
    FactQueryError, FactRuntime, PublishedCheckedUnit,
};
use crate::request::{CompilationOptions, CompilationRequest};
use crate::worker::WorkerBudget;

use super::load::{
    CompilationLoadError, SourceInputDiagnosticContext, missing_source_input_diagnostic,
    next_diagnostic_id, source_load_diagnostic,
};

/// Durable immutable compilation context and demand-driven fact entrypoint.
#[derive(Clone, Debug)]
pub struct Compilation {
    state: Arc<CompilationState>,
}

#[derive(Debug)]
struct CompilationState {
    options: CompilationOptions,
    sources: SourceStore,
    source_diagnostics: DiagnosticBag,
    fact_runtime: FactRuntime,
    cancellation: CancellationToken,
    source_unit_syntax: Vec<FactCell<SourceUnitSyntaxResult>>,
    syntax_tree_result: FactCell<SyntaxTreeResult>,
    declaration_chunks: Vec<FactCell<DeclarationChunkResult>>,
    declaration_table_result: FactCell<DeclarationTableResult>,
    available_compiler_known_symbols: FactCell<AvailableCompilerKnownSymbols>,
    checked_units: CheckedUnitFactCaches,
    check_diagnostics: FactCell<DiagnosticBag>,
}

impl Compilation {
    /// Loads source inputs into durable compilation state without requesting derived facts.
    pub fn load(request: impl Into<CompilationRequest>) -> Result<Self, CompilationLoadError> {
        let (options, source_inputs) = request.into().into_parts();

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

        Ok(Self {
            state: Arc::new(CompilationState {
                options,
                sources,
                source_diagnostics: diagnostics,
                fact_runtime: FactRuntime::default(),
                cancellation: CancellationToken::new(),
                source_unit_syntax: empty_fact_caches(source_count),
                syntax_tree_result: FactCell::new(),
                declaration_chunks: empty_fact_caches(source_count),
                declaration_table_result: FactCell::new(),
                available_compiler_known_symbols: FactCell::new(),
                checked_units: CheckedUnitFactCaches::new(),
                check_diagnostics: FactCell::new(),
            }),
        })
    }

    /// Loads source inputs with default options without requesting derived facts.
    pub fn load_sources(sources: Vec<SourceInput>) -> Result<Self, CompilationLoadError> {
        Self::load(CompilationRequest::new(sources))
    }

    /// Returns the compilation options.
    pub fn options(&self) -> CompilationOptions {
        self.state.options
    }

    /// Returns the compiler-owned CPU worker budget.
    pub fn worker_budget(&self) -> WorkerBudget {
        self.state.options.worker_budget()
    }

    /// Returns the target capability facts selected for this compilation.
    pub fn target_availability(&self) -> crate::TargetAvailabilityFacts {
        self.state.options.target_availability()
    }

    /// Returns the lazily published target-filtered compiler-known symbol view.
    pub fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.fact(
            CompilationFactKey::AvailableCompilerKnownSymbols,
            &self.state.available_compiler_known_symbols,
            || {
                let provider = match CompilerKnownSymbolProvider::build() {
                    Ok(provider) => Arc::new(provider),
                    // Generated catalog validation makes provider failure a compiler invariant.
                    Err(error) => panic!("compiler-known symbol provider is invalid: {error:?}"),
                };

                let target = self.target_availability();

                provider.available_symbols(|rule| target.supports(rule))
            },
        )
    }

    /// Returns the loaded source snapshots.
    pub fn sources(&self) -> &SourceStore {
        &self.state.sources
    }

    /// Returns diagnostics produced while loading source inputs.
    pub fn source_diagnostics(&self) -> &DiagnosticBag {
        &self.state.source_diagnostics
    }

    /// TODO: Replace this temporary command-facing API with a checked-program
    ///       result fact once binding and checking facts exist.
    ///
    /// Returns diagnostics for the current check command behavior.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::CheckDiagnostics,
            &self.state.check_diagnostics,
            || {
                DiagnosticBag::merged_all([
                    self.source_diagnostics(),
                    self.syntax_tree_result().diagnostics(),
                    self.declaration_diagnostics(),
                ])
            },
        )
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
        let syntax = self.source_unit_syntax(source_id)?;
        let cache = self.state.declaration_chunks.get(source_id.to_index()?)?;

        Some(self.fact(
            CompilationFactKey::DeclarationChunk(source_id),
            cache,
            || discover_source_unit_declarations(syntax.source_unit()),
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

    fn fact<'a, T>(
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
        }
    }

    // TODO(compilation): Remove this expectation when category-specific binders request checked
    //                    unit publication through this internal compilation fact boundary
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "category-specific checked-unit binders are implemented by subsequent issues"
        )
    )]
    pub(super) fn checked_unit<T: CheckedUnitFact>(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        compute: impl FnOnce(&CancellationToken) -> Result<CheckedUnitComputation<T>, FactQueryError>,
    ) -> Result<Arc<PublishedCheckedUnit<T>>, FactQueryError> {
        self.state
            .checked_units
            .get_or_compute(&self.state.fact_runtime, cancellation, key, || {
                compute(cancellation)
            })
    }
}

fn empty_fact_caches<T>(len: usize) -> Vec<FactCell<T>> {
    (0..len).map(|_| FactCell::new()).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use bray_binder::{BinderDependency, CheckedUnitComputation};
    use bray_compiler_known::{AvailabilityRule, CompilerKnownDeclarationKey};
    use bray_declarations::{DeclarationKind, ModulePath};
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag,
        DiagnosticId, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticResult,
        SeverityKind,
    };
    use bray_source::{
        SourceId, SourceIdentity, SourceInput, SourceInputKind, SourceOriginKind, SourceVersion,
        TextSize,
    };
    use bray_symbols::{FunctionSymbolId, StructSymbolId};

    use crate::TargetAvailabilityFacts;
    use crate::fact::{CancellationToken, FactQueryError};
    use crate::request::{CompilationOptions, CompilationRequest};
    use crate::test_support::{checked_callable_body, constant_template_key};
    use crate::worker::WorkerBudget;

    use super::Compilation;

    #[test]
    fn empty_compilation_requests_produce_diagnostics() {
        let compilation = match Compilation::load(Vec::new()) {
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
        let options = CompilationOptions::new(WorkerBudget::serial());

        let request = CompilationRequest::with_options(
            vec![
                source_input("module first\n", 1),
                source_input("module second\n", 2),
            ],
            options,
        );

        let compilation = match Compilation::load(request) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        assert_eq!(compilation.options(), options);
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
        let compilation = match Compilation::load_sources(vec![source_input("$", 0)]) {
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

        let compilation = match Compilation::load(vec![source_input("$", 0), invalid_utf8]) {
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

        let compilation = match Compilation::load_sources(vec![source_input("valid", 0), invalid]) {
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
        let compilation = match Compilation::load_sources(vec![source_input("$", 0)]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        let first = compilation.check_diagnostics();
        let second = compilation.check_diagnostics();

        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn checked_units_are_request_scoped_lazy_compilation_facts() {
        let compilation = declaration_compilation();

        let cancellation = CancellationToken::new();
        let computations = AtomicUsize::new(0);

        let (key, body) = checked_callable_body(20);

        let dependency = BinderDependency::Unit(constant_template_key(21));

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(7),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let first =
            match compilation.checked_unit(key.clone(), &cancellation, |request_cancellation| {
                assert!(std::ptr::eq(request_cancellation, &cancellation));

                computations.fetch_add(1, Ordering::SeqCst);

                Ok(CheckedUnitComputation::new(
                    DiagnosticResult::new(body.clone(), DiagnosticBag::single(diagnostic.clone())),
                    [dependency.clone()],
                ))
            }) {
                Ok(result) => result,
                Err(error) => panic!("checked callable body must publish: {error:?}"),
            };

        let second = match compilation.checked_unit(key, &CancellationToken::new(), |_| {
            panic!("cached checked unit must not be recomputed")
        }) {
            Ok(result) => result,
            Err(error) => panic!("checked callable body must be cached: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        assert_eq!(first.result().value(), &body);
        assert_eq!(first.result().diagnostics().diagnostics(), &[diagnostic]);
        assert_eq!(first.dependencies(), &[dependency]);
        assert_eq!(computations.load(Ordering::SeqCst), 1);

        let cancelled = CancellationToken::new();

        cancelled.cancel();

        let (retry_key, retry_body) = checked_callable_body(22);

        let cancelled_result = compilation.checked_unit(retry_key.clone(), &cancelled, |_| {
            Ok(CheckedUnitComputation::new(
                DiagnosticResult::without_diagnostics(retry_body.clone()),
                [],
            ))
        });

        assert!(matches!(cancelled_result, Err(FactQueryError::Cancelled)));

        let retried = compilation.checked_unit(retry_key, &CancellationToken::new(), |_| {
            Ok(CheckedUnitComputation::new(
                DiagnosticResult::without_diagnostics(retry_body.clone()),
                [],
            ))
        });

        assert!(retried.is_ok());
    }

    #[test]
    fn compiler_known_availability_is_lazy_cached_and_keeps_the_complete_provider() {
        let compilation = match Compilation::load_sources(vec![source_input("module app;", 0)]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        assert!(
            compilation
                .state
                .available_compiler_known_symbols
                .get()
                .is_none()
        );

        let first = compilation.available_compiler_known_symbols();
        let second = compilation.available_compiler_known_symbols();

        assert!(std::ptr::eq(first, second));

        assert!(
            compilation
                .state
                .available_compiler_known_symbols
                .get()
                .is_some()
        );

        assert_eq!(
            first.provider().declaration_symbols().len(),
            first.declarations().len() + 2
        );

        assert_eq!(
            first.declaration_symbol::<StructSymbolId>(&declaration_key("TargetReal16")),
            None
        );

        assert_eq!(
            first.declaration_symbol::<FunctionSymbolId>(&declaration_key("MemoryCopy")),
            None
        );
    }

    #[test]
    fn compilations_publish_independent_views_for_their_target_facts() {
        let portable = compilation_with_target(TargetAvailabilityFacts::portable());
        let real16 = compilation_with_target(
            TargetAvailabilityFacts::portable().with_rule(AvailabilityRule::Real16, true),
        );

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
        let compilation = match Compilation::load_sources(vec![source_input("$", 0)]) {
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
        let compilation = match Compilation::load_sources(vec![source_input(
            "module core; struct Point {} struct Point {}",
            0,
        )]) {
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
    fn check_diagnostics_merge_available_phase_diagnostics() {
        let invalid = SourceInput::file_bytes(
            SourceIdentity::new(11),
            "bad.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let compilation = match Compilation::load_sources(vec![source_input("$", 0), invalid]) {
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

    fn diagnostic_kinds(diagnostics: &DiagnosticBag) -> Vec<DiagnosticKind> {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect()
    }

    fn declaration_compilation() -> Compilation {
        match Compilation::load_sources(vec![
            source_input("module core; func first() {}", 0),
            source_input("module core { const Size: Int = 1; }", 1),
        ]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        }
    }

    fn compilation_with_target(target: TargetAvailabilityFacts) -> Compilation {
        let options =
            CompilationOptions::new(WorkerBudget::serial()).with_target_availability(target);

        let request =
            CompilationRequest::with_options(vec![source_input("module app;", 0)], options);

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

    fn source_input(text: &str, version: u32) -> SourceInput {
        SourceInput::virtual_text(
            SourceIdentity::new(version),
            format!("source-{version}"),
            SourceVersion::new(u64::from(version)),
            text,
        )
    }
}
