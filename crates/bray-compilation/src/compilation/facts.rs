use std::sync::{Arc, OnceLock};

use bray_diagnostics::DiagnosticBag;
use bray_parser::{SourceUnitSyntaxResult, SyntaxTreeResult, parse_source_unit};
use bray_source::{SourceId, SourceInput, SourceLoadError, SourceSnapshot, SourceStore};
use bray_syntax::SyntaxTree;

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
    source_unit_syntax: Vec<OnceLock<SourceUnitSyntaxResult>>,
    syntax_tree_result: OnceLock<SyntaxTreeResult>,
    check_diagnostics: OnceLock<DiagnosticBag>,
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
                source_unit_syntax: empty_fact_caches(source_count),
                syntax_tree_result: OnceLock::new(),
                check_diagnostics: OnceLock::new(),
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

    /// Returns the loaded source snapshots.
    pub fn sources(&self) -> &SourceStore {
        &self.state.sources
    }

    /// Returns diagnostics produced while loading source inputs.
    pub fn source_diagnostics(&self) -> &DiagnosticBag {
        &self.state.source_diagnostics
    }

    /// TODO: Replace this temporary command-facing API with a checked-program
    ///       result fact once declaration, binding, and checking facts exist.
    ///
    /// Returns diagnostics for the current check command behavior.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        self.state.check_diagnostics.get_or_init(|| {
            self.source_diagnostics()
                .merged(self.syntax_tree_result().diagnostics())
        })
    }

    /// Returns the syntax result for one source unit.
    pub fn source_unit_syntax(&self, source_id: SourceId) -> Option<&SourceUnitSyntaxResult> {
        let snapshot = self.source(source_id)?;
        let cache = self.state.source_unit_syntax.get(source_id.to_index()?)?;

        Some(cache.get_or_init(|| parse_source_unit(snapshot)))
    }

    /// Returns the syntax tree result for all loaded source units.
    pub fn syntax_tree_result(&self) -> &SyntaxTreeResult {
        self.state.syntax_tree_result.get_or_init(|| {
            let source_units = self.sources().iter().map(|snapshot| {
                // Source stores and fact caches share the loaded source count.
                // The whole-source syntax result owns the composed source units.
                self.source_unit_syntax(snapshot.source_id())
                    .expect("source unit fact cache should match source store")
                    .clone()
            });

            SyntaxTreeResult::from_source_unit_results(source_units)
        })
    }

    /// Returns the syntax tree for all loaded source units.
    pub fn syntax_tree(&self) -> &SyntaxTree {
        self.syntax_tree_result().syntax_tree()
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
}

fn empty_fact_caches<T>(len: usize) -> Vec<OnceLock<T>> {
    (0..len).map(|_| OnceLock::new()).collect()
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag, DiagnosticId,
        DiagnosticKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
    };
    use bray_source::{
        SourceId, SourceIdentity, SourceInput, SourceInputKind, SourceOriginKind, SourceVersion,
        TextSize,
    };

    use crate::request::{CompilationOptions, CompilationRequest};
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
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn check_diagnostics_request_syntax_and_merge_syntax_diagnostics() {
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

        assert_eq!(compilation.syntax_tree().source_units().len(), 1);

        assert_eq!(
            diagnostic_kinds(compilation.check_diagnostics()),
            [
                DiagnosticKind::SourceInvalidUtf8,
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
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
    fn check_diagnostics_merge_source_and_syntax_diagnostics() {
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
                DiagnosticKind::SyntaxSkippedSyntax
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

    fn source_input(text: &str, version: u32) -> SourceInput {
        SourceInput::virtual_text(
            SourceIdentity::new(version),
            format!("source-{version}"),
            SourceVersion::new(u64::from(version)),
            text,
        )
    }
}
