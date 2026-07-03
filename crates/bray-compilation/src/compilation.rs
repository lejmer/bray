use std::num::NonZeroUsize;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag, DiagnosticId,
    DiagnosticKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_source::{
    SourceId, SourceInput, SourceLoadError, SourceSnapshot, SourceStore, SourceUtf8Error, TextSize,
};

/// Positive CPU worker budget for compiler-owned work.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkerBudget {
    workers: NonZeroUsize,
}

impl WorkerBudget {
    /// Creates a worker budget from a positive worker count.
    pub fn new(workers: usize) -> Result<Self, WorkerBudgetError> {
        match NonZeroUsize::new(workers) {
            Some(workers) => Ok(Self::from_nonzero(workers)),
            None => Err(WorkerBudgetError::Zero),
        }
    }

    /// Creates a worker budget from a known non-zero worker count.
    pub const fn from_nonzero(workers: NonZeroUsize) -> Self {
        Self { workers }
    }

    /// Returns the serial worker budget.
    pub const fn serial() -> Self {
        Self {
            workers: NonZeroUsize::MIN,
        }
    }

    /// Returns a worker budget based on host parallelism, falling back to serial.
    pub fn available_parallelism() -> Self {
        match std::thread::available_parallelism() {
            Ok(workers) => Self::from_nonzero(workers),
            Err(_) => Self::serial(),
        }
    }

    /// Returns the positive worker count.
    pub const fn get(self) -> usize {
        self.workers.get()
    }
}

impl Default for WorkerBudget {
    fn default() -> Self {
        Self::available_parallelism()
    }
}

/// Error returned when constructing a worker budget.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkerBudgetError {
    /// Worker budgets must be positive.
    Zero,
}

/// Options for one compiler operation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct CompilationOptions {
    worker_budget: WorkerBudget,
}

impl CompilationOptions {
    /// Creates compilation options.
    pub const fn new(worker_budget: WorkerBudget) -> Self {
        Self { worker_budget }
    }

    /// Returns the compiler-owned CPU worker budget.
    pub const fn worker_budget(self) -> WorkerBudget {
        self.worker_budget
    }
}

/// Request used to build a [`Compilation`].
#[derive(Debug, Eq, PartialEq)]
pub struct CompilationRequest {
    options: CompilationOptions,
    sources: Vec<SourceInput>,
}

impl CompilationRequest {
    /// Creates a compilation request from source inputs and default options.
    pub fn new(sources: Vec<SourceInput>) -> Self {
        Self::with_options(sources, CompilationOptions::default())
    }

    /// Creates a compilation request from source inputs and explicit options.
    pub fn with_options(sources: Vec<SourceInput>, options: CompilationOptions) -> Self {
        Self { options, sources }
    }

    /// Returns the compilation options.
    pub const fn options(&self) -> CompilationOptions {
        self.options
    }

    /// Returns the source inputs in request order.
    pub fn sources(&self) -> &[SourceInput] {
        &self.sources
    }

    /// Consumes the request into its parts.
    pub fn into_parts(self) -> (CompilationOptions, Vec<SourceInput>) {
        (self.options, self.sources)
    }
}

impl From<Vec<SourceInput>> for CompilationRequest {
    fn from(sources: Vec<SourceInput>) -> Self {
        Self::new(sources)
    }
}

/// Error returned when building durable compilation state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompilationBuildError {
    /// A source input could not be loaded.
    SourceLoad {
        source_index: usize,
        error: SourceLoadError,
    },
    /// The compiler cannot assign another compact diagnostic ID.
    TooManyDiagnostics {
        /// Number of diagnostics already assigned.
        count: usize,
    },
}

impl CompilationBuildError {
    /// Returns the source input index associated with this build error.
    pub const fn source_index(self) -> Option<usize> {
        match self {
            Self::SourceLoad { source_index, .. } => Some(source_index),
            Self::TooManyDiagnostics { .. } => None,
        }
    }
}

/// Durable immutable state for one compiler operation.
#[derive(Debug, Eq, PartialEq)]
pub struct Compilation {
    options: CompilationOptions,
    sources: SourceStore,
    diagnostics: DiagnosticBag,
}

impl Compilation {
    /// Builds durable compilation state from a request.
    pub fn build(request: impl Into<CompilationRequest>) -> Result<Self, CompilationBuildError> {
        let (options, source_inputs) = request.into().into_parts();

        let mut sources = SourceStore::with_capacity(source_inputs.len());
        let mut diagnostics = DiagnosticBag::new();

        for (source_index, source_input) in source_inputs.into_iter().enumerate() {
            match sources.insert_input(source_input) {
                Ok(_) => {}
                Err(SourceLoadError::InvalidUtf8(error)) => {
                    diagnostics.add(invalid_utf8_diagnostic(
                        next_diagnostic_id(&diagnostics)?,
                        error,
                    ));
                }
                Err(error) => {
                    return Err(CompilationBuildError::SourceLoad {
                        source_index,
                        error,
                    });
                }
            }
        }

        Ok(Self {
            options,
            sources,
            diagnostics,
        })
    }

    /// Builds durable compilation state from source inputs and default options.
    pub fn from_sources(sources: Vec<SourceInput>) -> Result<Self, CompilationBuildError> {
        Self::build(CompilationRequest::new(sources))
    }

    /// Returns the compilation options.
    pub const fn options(&self) -> CompilationOptions {
        self.options
    }

    /// Returns the compiler-owned CPU worker budget.
    pub const fn worker_budget(&self) -> WorkerBudget {
        self.options.worker_budget()
    }

    /// Returns the loaded source snapshots.
    pub const fn sources(&self) -> &SourceStore {
        &self.sources
    }

    /// Returns the final merged diagnostics for this compilation.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns a new compilation with `diagnostics` folded into the final bag.
    ///
    /// The combination and deduplication policy belongs to [`DiagnosticBag`];
    /// compilation only publishes the resulting immutable state.
    pub fn with_additional_diagnostics(self, diagnostics: DiagnosticBag) -> Self {
        let diagnostics = self.diagnostics.merged(&diagnostics);

        Self {
            options: self.options,
            sources: self.sources,
            diagnostics,
        }
    }

    /// Returns the loaded source snapshot for `source_id`.
    pub fn source(&self, source_id: SourceId) -> Option<&SourceSnapshot> {
        self.sources.get(source_id)
    }

    /// Returns the loaded source text for `source_id`.
    pub fn source_text(&self, source_id: SourceId) -> Option<&str> {
        self.sources.text(source_id)
    }

    /// Returns the number of loaded source snapshots.
    pub const fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns whether this compilation has no source snapshots.
    pub const fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

fn next_diagnostic_id(diagnostics: &DiagnosticBag) -> Result<DiagnosticId, CompilationBuildError> {
    let raw = match u32::try_from(diagnostics.len()) {
        Ok(raw) => raw,
        Err(_) => {
            return Err(CompilationBuildError::TooManyDiagnostics {
                count: diagnostics.len(),
            });
        }
    };

    Ok(DiagnosticId::new(raw))
}

fn invalid_utf8_diagnostic(id: DiagnosticId, error: SourceUtf8Error) -> Diagnostic {
    let mut diagnostic =
        Diagnostic::new(id, DiagnosticKind::LexicalInvalidUtf8, SeverityKind::Error)
            .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

    if let Ok(offset) = TextSize::try_from(error.valid_up_to()) {
        diagnostic = diagnostic.with_arg(DiagnosticArg::new(
            DiagnosticArgName::TextOffset,
            DiagnosticArgValue::TextOffset(offset),
        ));
    }

    if let Some(byte_count) = invalid_utf8_byte_count(error) {
        diagnostic = diagnostic.with_arg(DiagnosticArg::new(
            DiagnosticArgName::ByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        ));
    }

    diagnostic
}

fn invalid_utf8_byte_count(error: SourceUtf8Error) -> Option<u32> {
    let len = error.invalid_sequence_len()?;

    u32::try_from(len).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        Compilation, CompilationOptions, CompilationRequest, WorkerBudget, WorkerBudgetError,
    };
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
    use bray_source::{SourceId, SourceIdentity, SourceInput, SourceOriginKind, SourceVersion};

    #[test]
    fn worker_budgets_are_positive() {
        assert_eq!(WorkerBudget::new(0), Err(WorkerBudgetError::Zero));

        let budget = match WorkerBudget::new(4) {
            Ok(budget) => budget,
            Err(error) => panic!("test worker budget should be valid: {error:?}"),
        };

        assert_eq!(budget.get(), 4);
        assert_eq!(WorkerBudget::serial().get(), 1);
        assert!(WorkerBudget::default().get() >= 1);
    }

    #[test]
    fn compilation_requests_hold_sources_and_options() {
        let options = CompilationOptions::new(WorkerBudget::serial());
        let request = CompilationRequest::with_options(vec![source_input("one", 1)], options);

        assert_eq!(request.options(), options);
        assert_eq!(request.sources().len(), 1);
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

        let compilation = match Compilation::build(request) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should build: {error:?}"),
        };

        assert_eq!(compilation.options(), options);
        assert_eq!(compilation.worker_budget(), WorkerBudget::serial());
        assert_eq!(compilation.source_count(), 2);
        assert!(compilation.diagnostics().is_empty());
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
    fn compilations_store_utf8_source_load_diagnostics() {
        let invalid = SourceInput::file_bytes(
            SourceIdentity::new(10),
            "bad.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let compilation = match Compilation::from_sources(vec![source_input("valid", 0), invalid]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("invalid UTF-8 should become a diagnostic: {error:?}"),
        };

        let diagnostics = compilation.diagnostics();

        assert_eq!(compilation.source_count(), 1);
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.len(), 1);

        let diagnostic = match diagnostics.diagnostics() {
            [diagnostic] => diagnostic,
            diagnostics => panic!("expected one diagnostic: {diagnostics:?}"),
        };

        assert_eq!(diagnostic.id(), DiagnosticId::new(0));
        assert_eq!(diagnostic.kind(), DiagnosticKind::LexicalInvalidUtf8);
        assert_eq!(diagnostic.severity(), SeverityKind::Error);
        assert_eq!(diagnostic.primary_span(), None);
        assert_eq!(diagnostic.args().len(), 2);
        assert_eq!(diagnostic.notes().len(), 1);
    }

    #[test]
    fn compilations_produce_final_diagnostics_from_phase_diagnostics() {
        let invalid = SourceInput::file_bytes(
            SourceIdentity::new(11),
            "bad.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let compilation = match Compilation::from_sources(vec![invalid]) {
            Ok(compilation) => compilation,
            Err(error) => panic!("invalid UTF-8 should become a diagnostic: {error:?}"),
        };

        let duplicate = match compilation.diagnostics().diagnostics() {
            [diagnostic] => diagnostic.clone(),
            diagnostics => panic!("expected one diagnostic: {diagnostics:?}"),
        };

        let lexical_warning = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Warning,
        );

        let phase_diagnostics =
            DiagnosticBag::from(vec![duplicate.clone(), lexical_warning.clone()]);

        let compilation = compilation.with_additional_diagnostics(phase_diagnostics);

        assert_eq!(
            compilation.diagnostics().diagnostics(),
            &[duplicate, lexical_warning]
        );
    }

    #[test]
    fn compilations_are_send_and_sync() {
        assert_send_sync::<Compilation>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn source_input(text: &str, version: u32) -> SourceInput {
        SourceInput::virtual_text(
            SourceIdentity::new(version),
            format!("source-{version}"),
            SourceVersion::new(u64::from(version)),
            text,
        )
    }
}
