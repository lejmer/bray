use super::super::inspection::{
    format_english_inspection_failure, format_english_native_linker_build_failure,
};
use super::super::native::{
    format_english_document_parse_kind, format_english_external_tool_operation,
};
use super::super::source::{
    format_english_io_error_kind, format_english_path, format_english_quoted_text,
};

const fn format_english_project_operation(
    operation: bray_diagnostics::DiagnosticProjectOperation,
) -> &'static str {
    use bray_diagnostics::DiagnosticProjectOperation as Operation;

    match operation {
        Operation::WorkspacePath => "resolve the workspace path",
        Operation::WorkflowProgressOutput => "write build progress",
        Operation::CommandRouting => "route the selected command",
        Operation::LanguageServerProcess => "run the language server",
        Operation::LanguageServerProtocolOutput => "forward language-server protocol output",
        Operation::FormatterWorkingDirectory => "select the formatter working directory",
        Operation::FormatterInput => "write source text to the formatter",
        Operation::FormatterProcess => "run the formatter",
        Operation::Inspection => "inspect compiler output",
        Operation::InspectionReportOutput => "write the compiler inspection report",
        Operation::ProjectInspectionJson => "encode project inspection output",
        Operation::CompilerJsonOutput => "decode compiler JSON output",
        Operation::ProductOutputDirectory => "resolve the product output directory",
        Operation::InterfaceCachePath => "construct the interface cache path",
        Operation::InterfaceCacheDirectory => "create the interface cache directory",
        Operation::ThinLtoCacheDirectory => "create the native optimization cache directory",
        Operation::CompilerProcess => "run the compiler",
        Operation::CompilerProfileOutputDirectory => "create the profile output directory",
        Operation::CompilerProfileReportOutput => "write the compiler profile report",
        Operation::CompilerProfileSerialization => "encode the compiler profile report",
        Operation::CompilerProfileSummary => "write the compiler profile summary",
        Operation::StandardOutput => "write standard output",
        Operation::StandardError => "write standard error",
        Operation::DiagnosticOutput => "write compiler diagnostics",
        Operation::WorkflowUnitCount => "count workflow units",
        Operation::DependencyInterface => "select a dependency interface",
        Operation::ExecutableOutputName => "construct the executable output name",
        Operation::PublishedExecutable => "publish the executable",
        Operation::TestSourcePackageIdentity => "construct the test source-package identity",
        Operation::ToolchainRoot => "resolve the toolchain root",
        Operation::ToolchainExecutable => "locate the Bray executable",
        Operation::GitClone => "start the Git clone process",
        Operation::GitCloneStatus => "complete the Git clone process",
        Operation::ProjectProcess => "run the selected project executable",
        Operation::VendorDirectory => "prepare the vendor directory",
        Operation::CreateVendorDirectory => "create the vendor directory",
        Operation::ProfileReportComparison => "compare compiler profile reports",
        Operation::ProfileReportRead => "read a compiler profile report",
        Operation::ProfileReportDecode => "decode a compiler profile report",
        Operation::ProfileReportValidation => "validate a compiler profile report",
        Operation::TestReportJson => "encode the test report",
        Operation::TestBatchRequest => "read the test batch request",
        Operation::TestConcurrency => "compute test concurrency",
        Operation::TestExecutionPlan => "construct the test execution plan",
        Operation::TestHostPublication => "publish the test host",
        Operation::TestFilter => "apply the test filter",
        Operation::TestHostLocation => "locate the test host",
        Operation::TestAdmission => "admit a test to the execution schedule",
        Operation::TestHostCompletion => "complete the test host process",
        Operation::TestScheduleCompletion => "complete the test schedule",
        Operation::TestCancellationHandler => "install the test cancellation handler",
    }
}

pub(crate) fn format_english_project_command_failure(
    failure: &bray_diagnostics::DiagnosticProjectCommandFailure,
) -> String {
    use bray_diagnostics::DiagnosticProjectCommandFailure as Failure;

    match failure {
        Failure::HostIo { operation, error } => format!(
            "could not {} because the host reported {}",
            format_english_project_operation(*operation),
            format_english_io_error_kind(*error)
        ),
        Failure::Io {
            operation,
            path,
            error,
        } => format!(
            "could not {} at {} because the host reported {}",
            format_english_project_operation(*operation),
            format_english_path(path),
            format_english_io_error_kind(*error)
        ),
        Failure::CurrentExecutable { operation, error } => format!(
            "could not {} because the host could not identify the current executable ({})",
            format_english_project_operation(*operation),
            format_english_io_error_kind(*error)
        ),
        Failure::MissingParent { operation, path } => format!(
            "could not {} because {} has no parent directory",
            format_english_project_operation(*operation),
            format_english_path(path)
        ),
        Failure::PathContract {
            operation,
            path,
            requirement,
        } => {
            let requirement = match requirement {
                bray_diagnostics::DiagnosticPathRequirement::OwnedDirectory => {
                    "an existing project-owned directory that is not a symbolic link"
                }
                bray_diagnostics::DiagnosticPathRequirement::Missing => "an unused path",
            };

            format!(
                "could not {} because {} must be {}",
                format_english_project_operation(*operation),
                format_english_path(path),
                requirement
            )
        }
        Failure::Process {
            operation,
            program,
            native_operation,
            failure,
        } => {
            format!(
                "could not {} with {} during {} because the platform reported {}",
                format_english_project_operation(*operation),
                program.display(),
                format_english_external_tool_operation(*native_operation),
                format_english_project_process_failure(*failure)
            )
        }
        Failure::ProcessExit {
            operation,
            program,
            code,
        } => code.map_or_else(
            || {
                format!(
                    "{} failed for {} without an exit code",
                    format_english_project_operation(*operation),
                    format_english_path(program)
                )
            },
            |code| {
                format!(
                    "{} failed for {} with exit code {code}",
                    format_english_project_operation(*operation),
                    format_english_path(program)
                )
            },
        ),
        Failure::CompilerOutputMissing {
            product,
            program,
            code,
        } => code.map_or_else(
            || {
                format!(
                    "the compiler process for {product} terminated unexpectedly before it could report a specific cause ({})",
                    format_english_path(program),
                )
            },
            |code| {
                format!(
                    "the compiler process for {product} terminated unexpectedly with exit code {code} before it could report a specific cause ({})",
                    format_english_path(program),
                )
            },
        ),
        Failure::CompilerOutputInvalid {
            product,
            program,
            code,
            problem,
            detail,
        } => {
            let exit = code.map_or_else(
                || "without an exit code".to_owned(),
                |code| format!("with exit code {code}"),
            );

            let detail = detail
                .as_ref()
                .map_or_else(String::new, |detail| format!(": {detail}"));

            format!(
                "the compiler process for {product} produced an invalid structured report {exit} ({}) because of {}{}",
                format_english_path(program),
                format_english_document_parse_kind(*problem),
                detail,
            )
        }
        Failure::Document {
            operation,
            path,
            problem,
            detail,
        } => {
            let path = path.as_ref().map_or_else(String::new, |path| {
                format!(" at {}", format_english_path(path))
            });

            let detail = detail
                .as_ref()
                .map_or_else(String::new, |detail| format!(": {detail}"));

            format!(
                "could not {}{} because of {}{}",
                format_english_project_operation(*operation),
                path,
                format_english_document_parse_kind(*problem),
                detail,
            )
        }
        Failure::TestExecutionPlan(problem) => format_english_test_plan_problem(problem),
        Failure::TestScheduling(problem) => match problem {
            bray_diagnostics::DiagnosticTestSchedulingProblem::InvocationNotActive(test) => {
                format!(
                    "received a result for inactive test {}",
                    format_english_quoted_text(test)
                )
            }
            bray_diagnostics::DiagnosticTestSchedulingProblem::ResultIdentityMismatch(test) => {
                format!(
                    "received a result for {} while completing a different test",
                    format_english_quoted_text(test)
                )
            }
        },
        Failure::ToolStreamIo {
            operation,
            stream,
            error,
        } => format!(
            "could not {} because {} I/O failed with {}",
            format_english_project_operation(*operation),
            format_english_tool_stream(*stream),
            format_english_io_error_kind(*error)
        ),
        Failure::ToolStreamInvalidUtf8 { operation, stream } => format!(
            "could not {} because the tool's {} is not valid UTF-8",
            format_english_project_operation(*operation),
            format_english_tool_stream(*stream)
        ),
        Failure::ToolProtocol { operation, failure } => format!(
            "could not {} because the compiler's child-process protocol {}",
            format_english_project_operation(*operation),
            format_english_tool_protocol_failure(*failure)
        ),
        Failure::ProfileComparison(problem) => match problem {
            bray_diagnostics::DiagnosticProfileComparisonProblem::Context { before, after } => {
                format!(
                    "profile reports describe different compilation contexts ({} versus {})",
                    format_english_profile_context(before),
                    format_english_profile_context(after)
                )
            }
            bray_diagnostics::DiagnosticProfileComparisonProblem::Descriptor { kind, id } => {
                format!(
                    "profile reports define incompatible metadata for {} descriptor {id}",
                    format_english_profile_descriptor_kind(*kind)
                )
            }
        },
        Failure::ProfileValidation { path, problem } => format!(
            "profile report {} {}",
            format_english_path(path),
            format_english_profile_validation_problem(problem)
        ),
        Failure::MissingTestHostLocation(test) => format!(
            "selected test {} has no published host executable",
            format_english_quoted_text(test)
        ),
        Failure::MissingResult(operation) => format!(
            "could not {} because the completed result omitted a required value",
            format_english_project_operation(*operation)
        ),
        Failure::CapacityExceeded(operation) => format!(
            "could not {} because its compact count range was exceeded",
            format_english_project_operation(*operation)
        ),
        Failure::CompilationDiagnosticCapacityExceeded { count } => format!(
            "could not load the compilation because {count} diagnostics already occupy every compact diagnostic identity"
        ),
        Failure::CompilationDiagnosticCountUnrepresentable => String::from(
            "could not load the compilation because the host diagnostic count exceeds the compiler protocol range",
        ),
        Failure::NativeLinkerConstruction { target, failure } => format!(
            "could not configure native linking for target {} because the compiler's {} contract is invalid",
            format_english_quoted_text(target),
            format_english_native_linker_build_failure(*failure)
        ),
        Failure::Inspection(failure) => format!(
            "could not construct the compiler inspection report because {}",
            format_english_inspection_failure(*failure)
        ),
        Failure::Invariant(operation) => format!(
            "could not {} because validated command state became inconsistent",
            format_english_project_operation(*operation)
        ),
    }
}

const fn format_english_tool_stream(
    stream: bray_diagnostics::DiagnosticToolStream,
) -> &'static str {
    use bray_diagnostics::DiagnosticToolStream as Stream;

    match stream {
        Stream::StandardInput => "standard input",
        Stream::StandardOutput => "standard output",
        Stream::StandardError => "standard error",
    }
}

fn format_english_tool_protocol_failure(
    failure: bray_diagnostics::DiagnosticToolProtocolFailure,
) -> String {
    use bray_diagnostics::DiagnosticToolProtocolFailure as Failure;

    match failure {
        Failure::MissingStream(stream) => format!(
            "did not provide configured piped {}",
            format_english_tool_stream(stream)
        ),
        Failure::StreamThreadPanicked(stream) => format!(
            "pump for {} terminated unexpectedly",
            format_english_tool_stream(stream)
        ),
    }
}

fn format_english_profile_context(context: &bray_diagnostics::DiagnosticProfileContext) -> String {
    format!("{}/{}/{}", context.package, context.product, context.target)
}

const fn format_english_profile_descriptor_kind(
    kind: bray_diagnostics::DiagnosticProfileDescriptorKind,
) -> &'static str {
    match kind {
        bray_diagnostics::DiagnosticProfileDescriptorKind::Operation => "operation",
        bray_diagnostics::DiagnosticProfileDescriptorKind::Query => "query",
        bray_diagnostics::DiagnosticProfileDescriptorKind::Metric => "metric",
    }
}

fn format_english_profile_validation_problem(
    problem: &bray_diagnostics::DiagnosticProfileValidationProblem,
) -> String {
    use bray_diagnostics::DiagnosticProfileValidationProblem as Problem;

    match problem {
        Problem::SchemaRevision { expected, actual } => {
            format!(
                "uses schema revision {actual}, but this toolchain requires revision {expected}"
            )
        }
        Problem::DuplicateDescriptor { kind, id } => format!(
            "contains duplicate {} descriptor {id}",
            format_english_profile_descriptor_kind(*kind)
        ),
        Problem::DuplicateObservation { kind, id } => format!(
            "contains duplicate observation for {} descriptor {id}",
            format_english_profile_descriptor_kind(*kind)
        ),
        Problem::UnknownDescriptor { kind, id } => format!(
            "references undeclared {} descriptor {id}",
            format_english_profile_descriptor_kind(*kind)
        ),
        Problem::InvalidRuntimeArtifactIdentity { index } => {
            format!("contains an empty runtime artifact identity at index {index}")
        }
        Problem::NonCanonicalRuntimeArtifacts { first, second } => format!(
            "contains non-canonical runtime artifact identities {} followed by {}",
            format_english_quoted_text(first),
            format_english_quoted_text(second)
        ),
        Problem::InvalidRuntimeRole { index } => {
            format!("contains an empty runtime role at index {index}")
        }
        Problem::NonCanonicalRuntimeRoles { first, second } => format!(
            "contains non-canonical runtime roles {} followed by {}",
            format_english_quoted_text(first),
            format_english_quoted_text(second)
        ),
        Problem::InvalidNativeCallbackEntry { index } => {
            format!("contains an empty native callback entry at index {index}")
        }
        Problem::NonCanonicalNativeCallbackEntries { first, second } => format!(
            "contains non-canonical native callback entries {} followed by {}",
            format_english_quoted_text(first),
            format_english_quoted_text(second)
        ),
        Problem::InvalidSchedulerStatistics => {
            "contains inconsistent scheduler statistics".to_owned()
        }
        Problem::InvalidQueryStatistics { id } => {
            format!("contains inconsistent request statistics for descriptor {id}")
        }
    }
}

fn format_english_project_process_failure(
    failure: bray_diagnostics::DiagnosticProjectProcessFailure,
) -> String {
    use bray_diagnostics::DiagnosticProjectProcessFailure as Failure;

    match failure {
        Failure::Io(error) => format!("I/O failure ({})", format_english_io_error_kind(error)),
        Failure::InvalidSize => "an invalid size".to_owned(),
        Failure::ThreadIdentityExhausted => "thread identity exhaustion".to_owned(),
        Failure::EventGenerationExhausted => "event generation exhaustion".to_owned(),
        Failure::InvalidEventIdentity => "an invalid event identity".to_owned(),
        Failure::RuntimeThreadAlreadyInitialized => {
            "an already initialized runtime thread".to_owned()
        }
        Failure::SynchronizationPoisoned => "poisoned synchronization state".to_owned(),
        Failure::Unsupported => "an unsupported operation".to_owned(),
    }
}

fn format_english_test_plan_problem(
    problem: &bray_diagnostics::DiagnosticTestExecutionPlanProblem,
) -> String {
    use bray_diagnostics::DiagnosticTestExecutionPlanProblem as Problem;

    match problem {
        Problem::InvocationCountMismatch {
            entries,
            invocations,
        } => format!(
            "test plan has {entries} selected entries but {invocations} invocation policies"
        ),
        Problem::InvocationIdentityMismatch(test) => format!(
            "test invocation {} does not match its selected entry",
            format_english_quoted_text(test)
        ),
        Problem::DuplicateIdentity(test) => format!(
            "test identity {} is selected more than once",
            format_english_quoted_text(test)
        ),
        Problem::CaptureBudgetExceeded {
            test,
            required_bytes,
            maximum_bytes,
        } => format!(
            "test {} requires {required_bytes} output-capture bytes, but the command budget is {maximum_bytes} bytes",
            format_english_quoted_text(test)
        ),
    }
}
