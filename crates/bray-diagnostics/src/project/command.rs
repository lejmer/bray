/// Exact user selection or command contract rejected before project execution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectSelectionProblem {
    /// `--cpu-count` was zero although worker counts are positive.
    WorkerCountZero,
    /// Profiling was requested for a command that does not invoke the compiler.
    ProfileRequiresCompilation,
    /// A standard-library root was not absolute after command-line path resolution.
    InvalidStandardLibraryRoot {
        /// Exact root path supplied to the compiler configuration.
        path: PathBuf,
    },
    /// A package identity supplied by the user is syntactically invalid.
    InvalidPackageIdentity(String),
    /// A source-package identity supplied by the user is syntactically invalid.
    InvalidSourcePackageIdentity(String),
    /// A package version supplied by the user is not a semantic version.
    InvalidPackageVersion(String),
    /// A product identity supplied by the user is syntactically invalid.
    InvalidProductIdentity(String),
    /// Parallel dependency option lists have different lengths.
    DependencyArgumentCountMismatch {
        /// Number of selected dependency products.
        products: u32,
        /// Number of selected dependency interface paths.
        interfaces: u32,
        /// Number of selected dependency implementation paths.
        implementations: u32,
    },
    /// A selected dependency product identity is invalid.
    InvalidDependencyProduct(String),
    /// A compiler-internal platform-service binding argument is invalid.
    InvalidPlatformServiceBinding(String),
    /// No product and target pair matches the complete command selection.
    NoMatchingProduct(String),
    /// A command requiring one product and target matched more than one.
    SingleProductRequired { actual: u32 },
    /// A package selector is syntactically invalid.
    InvalidPackageSelector(String),
    /// A package selector does not name a project package.
    UnknownPackage(String),
    /// A target selector does not name a project target.
    UnknownTarget(String),
    /// A selected product is not available for the selected target.
    ProductTargetUnavailable { product: String, target: String },
    /// An install name violates the portable dependency-name grammar.
    InvalidInstallName(String),
    /// A required executable artifact is absent from a completed build result.
    MissingExecutable,
    /// A required executable output path is absent from a completed build result.
    MissingExecutableOutput,
    /// A required test executable output path is absent.
    MissingTestExecutableOutput,
    /// A required test catalog output path is absent.
    MissingTestCatalogOutput,
    /// A required published test host is absent.
    MissingTestHost,
    /// The selected inspection command does not support the product category.
    UnsupportedInspectionProduct(String),
    /// A command requires an explicit target selection.
    TargetRequired,
    /// A test name filter is empty or otherwise violates the selection grammar.
    InvalidTestFilter(String),
}

/// Exact internal project-command operation that could not complete.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectOperation {
    WorkspacePath,
    WorkflowProgressOutput,
    CommandRouting,
    LanguageServerProcess,
    /// Language-server protocol bytes forwarded by the Tack process.
    LanguageServerProtocolOutput,
    FormatterWorkingDirectory,
    FormatterInput,
    FormatterProcess,
    Inspection,
    /// Publication of a compiler inspection report to its selected output path.
    InspectionReportOutput,
    ProjectInspectionJson,
    CompilerJsonOutput,
    ProductOutputDirectory,
    InterfaceCachePath,
    InterfaceCacheDirectory,
    /// Creation of the persistent native ThinLTO cache directory.
    ThinLtoCacheDirectory,
    CompilerProcess,
    CompilerProfileOutputDirectory,
    /// Publication of a compiler profile report to its selected output path.
    CompilerProfileReportOutput,
    /// Serialization of a complete compiler profile report.
    CompilerProfileSerialization,
    /// Human-readable compiler profile summary output.
    CompilerProfileSummary,
    /// Process standard-output stream.
    StandardOutput,
    /// Process standard-error stream.
    StandardError,
    /// Final structured or human-readable diagnostic output.
    DiagnosticOutput,
    WorkflowUnitCount,
    DependencyInterface,
    ExecutableOutputName,
    PublishedExecutable,
    TestSourcePackageIdentity,
    ToolchainRoot,
    ToolchainExecutable,
    GitClone,
    GitCloneStatus,
    ProjectProcess,
    VendorDirectory,
    CreateVendorDirectory,
    ProfileReportComparison,
    ProfileReportRead,
    ProfileReportDecode,
    ProfileReportValidation,
    TestReportJson,
    /// Decoding a typed same-process test batch request.
    TestBatchRequest,
    TestConcurrency,
    TestExecutionPlan,
    TestHostPublication,
    TestFilter,
    TestHostLocation,
    TestAdmission,
    TestHostCompletion,
    TestScheduleCompletion,
    TestCancellationHandler,
}

/// Filesystem shape required by a project command before it can modify a path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPathRequirement {
    /// The path must identify a real project-owned directory, not a symbolic link.
    OwnedDirectory,
    /// No filesystem entry may already exist at the path.
    Missing,
}

/// Exact platform failure reported while starting or communicating with a project tool.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectProcessFailure {
    /// The host reported a stable I/O failure category.
    Io(DiagnosticIoErrorKind),
    /// A length or allocation request exceeded the platform representation.
    InvalidSize,
    /// The platform could not allocate another native thread identity.
    ThreadIdentityExhausted,
    /// The platform could not allocate another event generation.
    EventGenerationExhausted,
    /// An event identity did not belong to the active event set.
    InvalidEventIdentity,
    /// A native runtime thread had already been initialized.
    RuntimeThreadAlreadyInitialized,
    /// A synchronization primitive was poisoned by a failed owner.
    SynchronizationPoisoned,
    /// The selected platform does not support the requested operation.
    Unsupported,
}

/// Standard stream owned by one compiler-host child tool.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticToolStream {
    /// Tool standard input.
    StandardInput,
    /// Tool standard output.
    StandardOutput,
    /// Tool standard error.
    StandardError,
}

/// Exact compiler-owned child-process protocol invariant that failed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticToolProtocolFailure {
    /// A configured piped stream was absent after process creation.
    MissingStream(DiagnosticToolStream),
    /// A compiler-owned stream pump terminated by panicking.
    StreamThreadPanicked(DiagnosticToolStream),
}

impl DiagnosticProjectProcessFailure {
    /// Returns the stable machine key for this platform failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::InvalidSize => "invalid_size",
            Self::ThreadIdentityExhausted => "thread_identity_exhausted",
            Self::EventGenerationExhausted => "event_generation_exhausted",
            Self::InvalidEventIdentity => "invalid_event_identity",
            Self::RuntimeThreadAlreadyInitialized => "runtime_thread_already_initialized",
            Self::SynchronizationPoisoned => "synchronization_poisoned",
            Self::Unsupported => "unsupported",
        }
    }
}

impl DiagnosticPathRequirement {
    /// Returns the stable machine key for this path requirement.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OwnedDirectory => "owned_directory",
            Self::Missing => "missing",
        }
    }
}

/// Exact execution-plan contract rejected by the test scheduler.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTestExecutionPlanProblem {
    /// Selected entries and invocation policies contain different item counts.
    InvocationCountMismatch { entries: u32, invocations: u32 },
    /// An invocation does not identify the entry at the same canonical position.
    InvocationIdentityMismatch(String),
    /// More than one selected entry claims the same product-qualified identity.
    DuplicateIdentity(String),
    /// One invocation exceeds the command-wide output-capture reservation.
    CaptureBudgetExceeded {
        test: String,
        required_bytes: u64,
        maximum_bytes: u64,
    },
}

/// Exact test-schedule state rejected while publishing a host result.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTestSchedulingProblem {
    /// The result identifies a test that is not currently active.
    InvocationNotActive(String),
    /// The result identity differs from the admitted invocation.
    ResultIdentityMismatch(String),
}

/// Exact incompatibility between two otherwise valid compiler profile reports.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProfileComparisonProblem {
    /// Package, product, or target identity differs between the reports.
    Context {
        /// Package/product/target identity of the older report.
        before: DiagnosticProfileContext,
        /// Package/product/target identity of the newer report.
        after: DiagnosticProfileContext,
    },
    /// A shared descriptor identity has incompatible metadata.
    Descriptor {
        /// Descriptor domain owning the numeric identity.
        kind: DiagnosticProfileDescriptorKind,
        /// Numeric descriptor identity with incompatible metadata.
        id: u16,
    },
}

/// Stable package, product, and target identity carried by a profile report.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticProfileContext {
    /// Canonical source package identity.
    pub package: String,
    /// Canonical selected product identity.
    pub product: String,
    /// Canonical compilation target identity.
    pub target: String,
}

/// Descriptor domain owning a profile descriptor identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProfileDescriptorKind {
    /// Compiler operation descriptor.
    Operation,
    /// Demand-driven query descriptor.
    Query,
    /// Compilation metric descriptor.
    Metric,
}

/// Exact structural contract violated by a compiler profile report.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProfileValidationProblem {
    /// The report schema revision is unsupported by this toolchain.
    SchemaRevision { expected: u32, actual: u32 },
    /// The descriptor catalog repeats a numeric identity.
    DuplicateDescriptor {
        kind: DiagnosticProfileDescriptorKind,
        id: u16,
    },
    /// Sparse observations repeat a descriptor reference.
    DuplicateObservation {
        kind: DiagnosticProfileDescriptorKind,
        id: u16,
    },
    /// An observation or event references an undeclared descriptor.
    UnknownDescriptor {
        kind: DiagnosticProfileDescriptorKind,
        id: u16,
    },
    /// A selected runtime artifact has no stable identity.
    InvalidRuntimeArtifactIdentity { index: usize },
    /// Runtime artifacts are duplicated or are not in canonical identity order.
    NonCanonicalRuntimeArtifacts { first: String, second: String },
    /// A selected runtime role has no stable identity.
    InvalidRuntimeRole { index: usize },
    /// Runtime roles are duplicated or are not in canonical identity order.
    NonCanonicalRuntimeRoles { first: String, second: String },
    /// A native callback entry has no stable symbol identity.
    InvalidNativeCallbackEntry { index: usize },
    /// Native callback entries are duplicated or are not in canonical symbol order.
    NonCanonicalNativeCallbackEntries { first: String, second: String },
    /// Scheduler aggregates contradict the configured worker budget or ready-work counts.
    InvalidSchedulerStatistics,
    /// Query aggregates contradict their request, evaluation, or distribution counts.
    InvalidQueryStatistics { id: u16 },
}

/// Exact structured cause of a failed project command operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectCommandFailure {
    /// A host I/O operation without an owned filesystem path failed.
    HostIo {
        /// Command operation owning the stream or ambient host request.
        operation: DiagnosticProjectOperation,
        /// Stable host I/O failure category.
        error: DiagnosticIoErrorKind,
    },
    /// A host file or path operation failed.
    Io {
        /// Command operation owning the path.
        operation: DiagnosticProjectOperation,
        /// Exact selected or constructed path.
        path: PathBuf,
        /// Stable host I/O failure category.
        error: DiagnosticIoErrorKind,
    },
    /// The host could not identify the current Bray executable.
    CurrentExecutable {
        /// Command operation resolving the executable.
        operation: DiagnosticProjectOperation,
        /// Stable host I/O failure category.
        error: DiagnosticIoErrorKind,
    },
    /// A selected path violates the installed-toolchain layout contract.
    MissingParent {
        /// Command operation resolving the path.
        operation: DiagnosticProjectOperation,
        /// Path whose parent was required.
        path: PathBuf,
    },
    /// A path exists in a shape that violates the command's filesystem contract.
    PathContract {
        /// Command operation owning the path.
        operation: DiagnosticProjectOperation,
        /// Exact path that violated the contract.
        path: PathBuf,
        /// Required filesystem shape.
        requirement: DiagnosticPathRequirement,
    },
    /// A native process operation failed before an exit status was available.
    Process {
        /// Project operation owning the child process.
        operation: DiagnosticProjectOperation,
        /// Exact executable path or program name.
        program: PathBuf,
        /// Native process step that failed.
        native_operation: DiagnosticExternalToolOperation,
        /// Exact stable platform failure, including an I/O category when applicable.
        failure: DiagnosticProjectProcessFailure,
    },
    /// A child process completed unsuccessfully.
    ProcessExit {
        /// Project operation owning the child process.
        operation: DiagnosticProjectOperation,
        /// Exact executable path or program name.
        program: PathBuf,
        /// Process exit code when the host supplied one.
        code: Option<i32>,
    },
    /// A compiler child exited unsuccessfully without its required structured report.
    CompilerOutputMissing {
        /// Exact package/product identity compiled by the child.
        product: String,
        /// Exact compiler executable path or program name.
        program: PathBuf,
        /// Process exit code when the host supplied one.
        code: Option<i32>,
    },
    /// A compiler child produced a malformed structured report.
    CompilerOutputInvalid {
        /// Exact package/product identity compiled by the child.
        product: String,
        /// Exact compiler executable path or program name.
        program: PathBuf,
        /// Process exit code when the host supplied one.
        code: Option<i32>,
        /// Exact structural category rejected by the report decoder.
        problem: DiagnosticDocumentParseKind,
        /// Exact decoder report when the document library supplied one.
        detail: Option<String>,
    },
    /// I/O through one selected child-tool stream failed.
    ToolStreamIo {
        /// Project command operation owning the tool.
        operation: DiagnosticProjectOperation,
        /// Exact child-tool stream.
        stream: DiagnosticToolStream,
        /// Stable host I/O failure category.
        error: DiagnosticIoErrorKind,
    },
    /// A selected child tool emitted bytes outside its UTF-8 output contract.
    ToolStreamInvalidUtf8 {
        /// Project command operation owning the tool.
        operation: DiagnosticProjectOperation,
        /// Exact child-tool stream.
        stream: DiagnosticToolStream,
    },
    /// Compiler-owned child-process stream setup violated its invariant.
    ToolProtocol {
        /// Project command operation owning the tool.
        operation: DiagnosticProjectOperation,
        /// Exact rejected process protocol contract.
        failure: DiagnosticToolProtocolFailure,
    },
    /// Structured command input or output could not be encoded or decoded.
    Document {
        /// Command operation owning the document.
        operation: DiagnosticProjectOperation,
        /// Document path when the content came from a file.
        path: Option<PathBuf>,
        /// Stable syntax, schema, input, or serialization category.
        problem: DiagnosticDocumentParseKind,
        /// Exact parser or serializer report when the document library supplied one.
        detail: Option<String>,
    },
    /// Test scheduling rejected the exact selected work.
    TestExecutionPlan(DiagnosticTestExecutionPlanProblem),
    /// Test scheduling rejected a host result.
    TestScheduling(DiagnosticTestSchedulingProblem),
    /// Two selected profile reports cannot be compared meaningfully.
    ProfileComparison(DiagnosticProfileComparisonProblem),
    /// A selected profile report violates its structural contract.
    ProfileValidation {
        /// Exact selected profile path.
        path: PathBuf,
        /// Exact rejected profile contract.
        problem: DiagnosticProfileValidationProblem,
    },
    /// A selected test has no published host executable.
    MissingTestHostLocation(String),
    /// A completed compiler/build result omitted a required typed value.
    MissingResult(DiagnosticProjectOperation),
    /// A finite command count exceeded its compact representation.
    CapacityExceeded(DiagnosticProjectOperation),
    /// Compilation loading exhausted the compact diagnostic identity space.
    CompilationDiagnosticCapacityExceeded {
        /// Exact number of diagnostics that already occupy compact identities.
        count: u64,
    },
    /// The host diagnostic count exceeds the locale-neutral protocol representation.
    CompilationDiagnosticCountUnrepresentable,
    /// Compiler-owned native linker composition violated an internal contract.
    NativeLinkerConstruction {
        /// Canonical target triple whose linker composition failed.
        target: String,
        /// Exact rejected linker composition contract.
        failure: DiagnosticNativeLinkerBuildFailure,
    },
    /// Compiler inspection report construction violated an internal contract.
    Inspection(DiagnosticInspectionFailure),
    /// A command state transition contradicted an already validated invariant.
    Invariant(DiagnosticProjectOperation),
}

impl DiagnosticProjectCommandFailure {
    /// Returns whether this failure means Bray violated a previously validated invariant.
    pub const fn is_compiler_defect(&self) -> bool {
        matches!(
            self,
            Self::MissingResult(_)
                | Self::NativeLinkerConstruction { .. }
                | Self::Inspection(_)
                | Self::ToolProtocol { .. }
                | Self::Invariant(_)
                | Self::CompilerOutputMissing { .. }
                | Self::CompilerOutputInvalid { .. }
                | Self::Document {
                    operation: DiagnosticProjectOperation::CompilerJsonOutput,
                    ..
                }
        )
    }

    /// Converts this failure into its exact project or compiler-defect diagnostic.
    pub fn diagnostic(self, id: DiagnosticId) -> Diagnostic {
        let defect = self.is_compiler_defect();

        let kind = if defect {
            DiagnosticKind::ProjectCompilerDefect
        } else {
            DiagnosticKind::ProjectCommandFailed
        };

        let diagnostic = Diagnostic::new(id, kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::project_command_failure(self));

        if defect {
            diagnostic.with_note(DiagnosticNote::new(
                DiagnosticNoteKind::ReportCompilerDefect,
            ))
        } else {
            diagnostic
        }
    }
}

impl DiagnosticProjectOperation {
    /// Returns the stable machine key for this command operation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspacePath => "workspace_path",
            Self::WorkflowProgressOutput => "workflow_progress_output",
            Self::CommandRouting => "command_routing",
            Self::LanguageServerProcess => "language_server_process",
            Self::LanguageServerProtocolOutput => "language_server_protocol_output",
            Self::FormatterWorkingDirectory => "formatter_working_directory",
            Self::FormatterInput => "formatter_input",
            Self::FormatterProcess => "formatter_process",
            Self::Inspection => "inspection",
            Self::InspectionReportOutput => "inspection_report_output",
            Self::ProjectInspectionJson => "project_inspection_json",
            Self::CompilerJsonOutput => "compiler_json_output",
            Self::ProductOutputDirectory => "product_output_directory",
            Self::InterfaceCachePath => "interface_cache_path",
            Self::InterfaceCacheDirectory => "interface_cache_directory",
            Self::ThinLtoCacheDirectory => "thin_lto_cache_directory",
            Self::CompilerProcess => "compiler_process",
            Self::CompilerProfileOutputDirectory => "compiler_profile_output_directory",
            Self::CompilerProfileReportOutput => "compiler_profile_report_output",
            Self::CompilerProfileSerialization => "compiler_profile_serialization",
            Self::CompilerProfileSummary => "compiler_profile_summary",
            Self::StandardOutput => "standard_output",
            Self::StandardError => "standard_error",
            Self::DiagnosticOutput => "diagnostic_output",
            Self::WorkflowUnitCount => "workflow_unit_count",
            Self::DependencyInterface => "dependency_interface",
            Self::ExecutableOutputName => "executable_output_name",
            Self::PublishedExecutable => "published_executable",
            Self::TestSourcePackageIdentity => "test_source_package_identity",
            Self::ToolchainRoot => "toolchain_root",
            Self::ToolchainExecutable => "toolchain_executable",
            Self::GitClone => "git_clone",
            Self::GitCloneStatus => "git_clone_status",
            Self::ProjectProcess => "project_process",
            Self::VendorDirectory => "vendor_directory",
            Self::CreateVendorDirectory => "create_vendor_directory",
            Self::ProfileReportComparison => "profile_report_comparison",
            Self::ProfileReportRead => "profile_report_read",
            Self::ProfileReportDecode => "profile_report_decode",
            Self::ProfileReportValidation => "profile_report_validation",
            Self::TestReportJson => "test_report_json",
            Self::TestBatchRequest => "test_batch_request",
            Self::TestConcurrency => "test_concurrency",
            Self::TestExecutionPlan => "test_execution_plan",
            Self::TestHostPublication => "test_host_publication",
            Self::TestFilter => "test_filter",
            Self::TestHostLocation => "test_host_location",
            Self::TestAdmission => "test_admission",
            Self::TestHostCompletion => "test_host_completion",
            Self::TestScheduleCompletion => "test_schedule_completion",
            Self::TestCancellationHandler => "test_cancellation_handler",
        }
    }
}
use std::path::PathBuf;

use crate::{
    Diagnostic, DiagnosticArg, DiagnosticDocumentParseKind, DiagnosticExternalToolOperation,
    DiagnosticId, DiagnosticInspectionFailure, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};

/// Exact immutable linker-capability contract rejected during compiler setup.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkerCapabilityBuildFailure {
    /// The driver category cannot own the declared capability family.
    DriverKindMismatch,
    /// The configured target is incompatible with the driver command family.
    UnsupportedTarget,
    /// The driver declares no supported target.
    MissingTargets,
    /// One architecture and object-format pair occurs more than once.
    DuplicateTarget,
}

/// Exact external-tool template contract rejected during compiler setup.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInvocationBuildFailure {
    /// The executable path is empty.
    EmptyProgram,
    /// The selected working directory is empty.
    EmptyCurrentDirectory,
    /// An environment variable name is empty.
    EmptyEnvironmentVariableName,
    /// An environment variable name occurs more than once.
    DuplicateEnvironmentVariableName,
    /// More than one response file uses the same path.
    DuplicateResponseFile,
}

/// Exact compiler-owned linker composition contract that failed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeLinkerBuildFailure {
    /// The compiler-owned archiver identity is invalid.
    ArchiveIdentity,
    /// The archive driver identity selects the wrong driver category.
    ArchiveDriverKindMismatch,
    /// The archive driver's capability record is invalid.
    ArchiveCapabilities(DiagnosticLinkerCapabilityBuildFailure),
    /// The archive driver program path is not explicit.
    ArchiveProgramPathNotExplicit,
    /// The archive driver invocation template is invalid.
    ArchiveInvocation(DiagnosticInvocationBuildFailure),
    /// The compiler-owned system-linker identity is invalid.
    SystemIdentity,
    /// The selected target cannot form a linker target identity.
    TargetIdentity,
    /// The system-linker program path is not explicit.
    SystemProgramPathNotExplicit,
    /// The system-linker invocation template is invalid.
    SystemInvocation(DiagnosticInvocationBuildFailure),
    /// The system-linker ThinLTO cache root is empty.
    SystemThinLtoCacheRootEmpty,
    /// The system-linker driver identity selects the wrong category.
    SystemDriverKindMismatch,
    /// The system-linker driver's capability record is invalid.
    SystemCapabilities(DiagnosticLinkerCapabilityBuildFailure),
    /// The compiler-host linker registry repeats one exact driver identity.
    DuplicateDriver,
}
