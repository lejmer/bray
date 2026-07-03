use std::num::NonZeroUsize;

use bray_source::{SourceId, SourceInput, SourceLoadError, SourceSnapshot, SourceStore};

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
}

impl CompilationBuildError {
    /// Returns the source input index associated with this build error.
    pub const fn source_index(self) -> usize {
        match self {
            Self::SourceLoad { source_index, .. } => source_index,
        }
    }
}

/// Durable immutable state for one compiler operation.
#[derive(Debug, Eq, PartialEq)]
pub struct Compilation {
    options: CompilationOptions,
    sources: SourceStore,
}

impl Compilation {
    /// Builds durable compilation state from a request.
    pub fn build(request: impl Into<CompilationRequest>) -> Result<Self, CompilationBuildError> {
        let (options, source_inputs) = request.into().into_parts();

        let mut sources = SourceStore::with_capacity(source_inputs.len());

        for (source_index, source_input) in source_inputs.into_iter().enumerate() {
            if let Err(error) = sources.insert_input(source_input) {
                return Err(CompilationBuildError::SourceLoad {
                    source_index,
                    error,
                });
            }
        }

        Ok(Self { options, sources })
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

#[cfg(test)]
mod tests {
    use super::{
        Compilation, CompilationBuildError, CompilationOptions, CompilationRequest, WorkerBudget,
        WorkerBudgetError,
    };
    use bray_source::{
        SourceId, SourceIdentity, SourceInput, SourceLoadError, SourceOriginKind, SourceUtf8Error,
        SourceVersion,
    };

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
    fn compilations_report_source_load_failures_with_request_index() {
        let invalid = SourceInput::file_bytes(
            SourceIdentity::new(10),
            "bad.bray",
            SourceVersion::new(0),
            vec![0xff],
        );

        let error = match Compilation::from_sources(vec![source_input("valid", 0), invalid]) {
            Ok(compilation) => panic!("test compilation should fail: {compilation:?}"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            CompilationBuildError::SourceLoad {
                source_index: 1,
                error: SourceLoadError::InvalidUtf8(SourceUtf8Error::new(0, Some(1)))
            }
        );

        assert_eq!(error.source_index(), 1);
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
