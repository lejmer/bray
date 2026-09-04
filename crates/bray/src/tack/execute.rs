// rust-style: allow(module-too-large, reason = "Tack command execution shares one project-loading and structured-result orchestration boundary")

use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_diagnostics::{
    DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticIoErrorKind,
    DiagnosticProjectCommandFailure, DiagnosticProjectOperation, DiagnosticProjectSelectionProblem,
};
use bray_project::ProjectGraph;
use bray_test_protocol::{MAXIMUM_TEST_BATCH_REQUEST_BYTES, TestBatchRequest};
use bray_tooling::{OutputFormat, write_diagnostic_groups, write_diagnostics};

use crate::tack::compiler::ProjectCompiler;
use crate::tack::error::{operation_diagnostics, selection_diagnostics, tool_execution_failure};
use crate::tack::init::initialize_project;
use crate::tack::inspection::render_project_inspection;
use crate::tack::install::{install_git_repository, run_project_process};
use crate::tack::model::{
    TackCommand, TackInspection, TackInvocation, TackProfileConfiguration, TackSelection,
};
use crate::tack::output::{
    failure, result_from_operation, result_from_output, result_from_outputs,
};
use crate::tack::profile::run_profile_command;
use crate::tack::progress::WorkflowProgress;
use crate::tack::project::{
    PlannedProduct, ProductSelectionKind, load_graph, root_source_files, select_products,
    select_target,
};
use crate::tack::result::TackRunResult;
use crate::tack::testing::{BuiltTestHost, TestBuildProvenance};
use crate::tack::tool::{
    NativeToolExecutor, Tool, ToolExecutionError, ToolExecutor, ToolOutput, ToolRequest, ToolStream,
};
use crate::tack::toolchain::Toolchain;

/// Runs Bray Tack using independently installed toolchain executables.
pub fn run_tack(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let interactive = io::stderr().is_terminal();
    let mut protocol_output = Vec::new();

    let result = run_tack_result_with_input_and_output(
        arguments,
        &NativeToolExecutor,
        Box::new(io::stdin()),
        &mut protocol_output,
        interactive,
    );

    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    if let Err(error) = stdout.write_all(&protocol_output) {
        write_tack_output_failure(
            DiagnosticProjectOperation::LanguageServerProtocolOutput,
            error.kind(),
            FailedTackStream::StandardOutput,
            &mut stdout,
            &mut stderr,
        );

        return ExitCode::FAILURE;
    }

    if let Err(error) = stderr.write_all(result.stderr().as_bytes()) {
        write_tack_output_failure(
            DiagnosticProjectOperation::StandardError,
            error.kind(),
            FailedTackStream::StandardError,
            &mut stdout,
            &mut stderr,
        );

        return ExitCode::FAILURE;
    }

    if let Err(error) = stdout.write_all(result.stdout().as_bytes()) {
        write_tack_output_failure(
            DiagnosticProjectOperation::StandardOutput,
            error.kind(),
            FailedTackStream::StandardOutput,
            &mut stdout,
            &mut stderr,
        );

        return ExitCode::FAILURE;
    }

    if !result.diagnostics().is_empty()
        && let Err(error) = write_diagnostic_groups(
            result.diagnostic_groups(),
            result.output_format(),
            &mut stdout,
            &mut stderr,
        )
    {
        let failed_stream = match result.output_format() {
            OutputFormat::Text => FailedTackStream::StandardError,
            OutputFormat::Json => FailedTackStream::StandardOutput,
        };

        write_tack_output_failure(
            DiagnosticProjectOperation::DiagnosticOutput,
            error.kind(),
            failed_stream,
            &mut stdout,
            &mut stderr,
        );

        return ExitCode::FAILURE;
    }

    result.exit_code()
}

#[derive(Clone, Copy)]
enum FailedTackStream {
    StandardOutput,
    StandardError,
}

fn write_tack_output_failure(
    operation: DiagnosticProjectOperation,
    error: io::ErrorKind,
    failed_stream: FailedTackStream,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) {
    let diagnostic = DiagnosticProjectCommandFailure::HostIo {
        operation,
        error: DiagnosticIoErrorKind::from(error),
    }
    .diagnostic(bray_diagnostics::DiagnosticId::new(0));

    let diagnostics = DiagnosticBag::single(diagnostic);

    let _ = match failed_stream {
        FailedTackStream::StandardOutput => write_diagnostics(
            &diagnostics,
            None,
            OutputFormat::Text,
            &mut io::sink(),
            stderr,
        ),
        FailedTackStream::StandardError => write_diagnostics(
            &diagnostics,
            None,
            OutputFormat::Text,
            &mut io::sink(),
            stdout,
        ),
    };
}

/// Runs Bray Tack and returns its structured outcome.
pub fn run_tack_result(arguments: impl IntoIterator<Item = OsString>) -> TackRunResult {
    run_tack_result_with_input(arguments, &NativeToolExecutor, io::empty())
}

fn run_tack_result_with_input(
    arguments: impl IntoIterator<Item = OsString>,
    executor: &dyn ToolExecutor,
    stdin: impl Read + Send + 'static,
) -> TackRunResult {
    let mut protocol_output = Vec::new();

    let mut result = run_tack_result_with_input_and_output(
        arguments,
        executor,
        Box::new(stdin),
        &mut protocol_output,
        false,
    );

    let protocol_output = match String::from_utf8(protocol_output) {
        Ok(output) => output,
        Err(_) => {
            return failure(
                operation_diagnostics(tool_execution_failure(
                    DiagnosticProjectOperation::LanguageServerProcess,
                    ToolExecutionError::InvalidUtf8(ToolStream::StandardOutput),
                )),
                result.output_format(),
            );
        }
    };

    result.prepend_stdout(protocol_output);

    result
}

fn run_tack_result_with_input_and_output(
    arguments: impl IntoIterator<Item = OsString>,
    executor: &dyn ToolExecutor,
    stdin: Box<dyn Read + Send>,
    protocol_output: &mut dyn Write,
    interactive: bool,
) -> TackRunResult {
    let invocation = match TackInvocation::try_from_arguments(arguments) {
        Ok(invocation) => invocation,
        Err(error) => {
            let exit_code = error.exit_code();
            let output_format = error.output_format();

            let (diagnostics, stdout, stderr) = error.into_output();

            return TackRunResult::with_output(
                exit_code,
                diagnostics,
                output_format,
                stdout,
                stderr,
            );
        }
    };

    execute_invocation(invocation, executor, stdin, protocol_output, interactive)
}

fn execute_invocation(
    invocation: TackInvocation,
    executor: &dyn ToolExecutor,
    stdin: Box<dyn Read + Send>,
    protocol_output: &mut dyn Write,
    interactive: bool,
) -> TackRunResult {
    let progress = WorkflowProgress::new(
        interactive && invocation.output_format() == OutputFormat::Text,
        invocation.verbose(),
    );

    let mut result =
        execute_invocation_with_progress(invocation, executor, stdin, protocol_output, &progress);

    if let Err(problem) = progress.write_to_result(&mut result) {
        return failure(
            operation_diagnostics(DiagnosticProjectCommandFailure::Document {
                operation: DiagnosticProjectOperation::WorkflowProgressOutput,
                path: None,
                problem,
                detail: None,
            }),
            result.output_format(),
        );
    }

    result
}

fn execute_invocation_with_progress(
    invocation: TackInvocation,
    executor: &dyn ToolExecutor,
    mut stdin: Box<dyn Read + Send>,
    protocol_output: &mut dyn Write,
    progress: &WorkflowProgress,
) -> TackRunResult {
    let (
        workspace_root,
        toolchain_root,
        standard_library_source,
        worker_count,
        output_format,
        profile,
        command,
    ) = invocation.into_parts();

    let workspace_root = match std::path::absolute(&workspace_root) {
        Ok(workspace_root) => workspace_root,
        Err(error) => {
            return failure(
                operation_diagnostics(DiagnosticProjectCommandFailure::Io {
                    operation: DiagnosticProjectOperation::WorkspacePath,
                    path: workspace_root,
                    error: DiagnosticIoErrorKind::from(error.kind()),
                }),
                output_format,
            );
        }
    };

    match command {
        TackCommand::Storage { options, action } => {
            let graph = match load_graph(&workspace_root, standard_library_source) {
                Ok(graph) => graph,
                Err(diagnostics) => return failure(diagnostics, output_format),
            };

            return crate::tack::storage::run_storage_command(
                &workspace_root,
                &graph,
                options,
                action,
                output_format,
            );
        }
        TackCommand::Init { directory, package } => {
            let directory = directory.unwrap_or(workspace_root);

            let directory = match std::path::absolute(&directory) {
                Ok(directory) => directory,
                Err(error) => {
                    return failure(
                        operation_diagnostics(DiagnosticProjectCommandFailure::Io {
                            operation: DiagnosticProjectOperation::WorkspacePath,
                            path: directory,
                            error: DiagnosticIoErrorKind::from(error.kind()),
                        }),
                        output_format,
                    );
                }
            };

            return result_from_operation(
                initialize_project(&directory, package.as_deref()),
                output_format,
            );
        }
        TackCommand::VendorInstall { name, repository } => {
            return result_from_operation(
                install_git_repository(&workspace_root, &name, &repository),
                output_format,
            );
        }
        TackCommand::Format {
            check,
            configuration,
            files,
        } => {
            return run_format(
                &workspace_root,
                worker_count,
                check,
                configuration,
                files,
                output_format,
                executor,
                stdin.as_mut(),
            );
        }
        TackCommand::ProfileShow { report } => {
            return run_profile_command(report, None, output_format);
        }
        TackCommand::ProfileCompare { before, after } => {
            return run_profile_command(before, Some(after), output_format);
        }
        _ => {}
    }

    let toolchain = match Toolchain::select(toolchain_root) {
        Ok(toolchain) => toolchain,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let graph = match load_graph(&workspace_root, standard_library_source) {
        Ok(graph) => graph,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    match command {
        TackCommand::Check(selection) => run_check(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            profile.as_ref(),
            output_format,
            executor,
        ),
        TackCommand::Build {
            selection,
            configuration,
        } => run_build(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            configuration,
            profile.as_ref(),
            output_format,
            executor,
            progress,
        ),
        TackCommand::Run {
            selection,
            configuration,
            arguments,
        } => run_one(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            configuration,
            arguments,
            profile.as_ref(),
            output_format,
            executor,
            progress,
        ),
        TackCommand::Test {
            selection,
            configuration,
            no_build,
            batch_request,
            native_link_inputs,
            options,
        } => run_tests(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            configuration,
            no_build,
            batch_request.as_deref(),
            native_link_inputs,
            options,
            profile.as_ref(),
            output_format,
            executor,
            progress,
        ),
        TackCommand::Inspect {
            selection,
            inspection,
            source_id,
            position,
            source,
        } => run_inspect(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            inspection,
            source_id,
            position,
            source,
            profile.as_ref(),
            output_format,
            executor,
        ),
        TackCommand::LanguageServer { target } => run_language_server(
            workspace_root,
            &graph,
            worker_count,
            target.as_deref(),
            output_format,
            executor,
            stdin,
            protocol_output,
        ),
        TackCommand::Storage { .. }
        | TackCommand::Init { .. }
        | TackCommand::Format { .. }
        | TackCommand::ProfileShow { .. }
        | TackCommand::ProfileCompare { .. }
        | TackCommand::VendorInstall { .. } => failure(
            operation_diagnostics(DiagnosticProjectCommandFailure::Invariant(
                DiagnosticProjectOperation::CommandRouting,
            )),
            output_format,
        ),
    }
}

fn run_check(
    workspace_root: &Path,
    graph: &ProjectGraph,
    toolchain: &Toolchain,
    worker_count: usize,
    selection: &TackSelection,
    profile: Option<&TackProfileConfiguration>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
) -> TackRunResult {
    let products = match select_products(graph, selection, ProductSelectionKind::Any, false) {
        Ok(products) => products,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let mut compiler = ProjectCompiler::new(
        workspace_root,
        graph,
        toolchain,
        worker_count,
        output_format,
        profile,
        executor,
    );

    let mut outputs = Vec::new();

    for product in products {
        match compiler.check(&product) {
            Ok(product_outputs) => outputs.extend(product_outputs),
            Err(diagnostics) => return failure(diagnostics, output_format),
        }
    }

    result_from_outputs(outputs, output_format)
}

#[expect(
    clippy::too_many_arguments,
    reason = "build routing keeps the selected project, process, and command inputs explicit"
)]
fn run_build(
    workspace_root: &Path,
    graph: &ProjectGraph,
    toolchain: &Toolchain,
    worker_count: usize,
    selection: &TackSelection,
    configuration: crate::tack::model::TackBuildConfiguration,
    profile: Option<&TackProfileConfiguration>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    progress: &WorkflowProgress,
) -> TackRunResult {
    let products = match select_products(graph, selection, ProductSelectionKind::Any, false) {
        Ok(products) => products,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let mut compiler = ProjectCompiler::new(
        workspace_root,
        graph,
        toolchain,
        worker_count,
        output_format,
        profile,
        executor,
    );

    let mut outputs = Vec::new();

    for product in products {
        let plan = match compiler.build_progress_plan(&product, configuration) {
            Ok(plan) => plan,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        let session = progress.begin(plan);

        match compiler.build(&product, configuration, Some(&session)) {
            Ok(build) => {
                let (product_outputs, _) = build.into_parts();

                let success = product_outputs.iter().all(ToolOutput::success);

                session.finish(success);
                outputs.extend(product_outputs);
            }
            Err(diagnostics) => {
                session.finish(false);

                return failure(diagnostics, output_format);
            }
        }
    }

    result_from_outputs(outputs, output_format)
}

#[expect(
    clippy::too_many_arguments,
    reason = "run routing keeps the selected project, process, and command inputs explicit"
)]
fn run_one(
    workspace_root: &Path,
    graph: &ProjectGraph,
    toolchain: &Toolchain,
    worker_count: usize,
    selection: &TackSelection,
    configuration: crate::tack::model::TackBuildConfiguration,
    arguments: Vec<OsString>,
    profile: Option<&TackProfileConfiguration>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    progress: &WorkflowProgress,
) -> TackRunResult {
    let products = match select_products(graph, selection, ProductSelectionKind::Executable, true) {
        Ok(products) => products,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let Some(product) = products.first() else {
        return failure(
            selection_diagnostics(DiagnosticProjectSelectionProblem::MissingExecutable),
            output_format,
        );
    };

    let mut compiler = ProjectCompiler::new(
        workspace_root,
        graph,
        toolchain,
        worker_count,
        output_format,
        profile,
        executor,
    );

    let plan = match compiler.build_progress_plan(product, configuration) {
        Ok(plan) => plan,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let session = progress.begin(plan);

    let (outputs, executable) = match compiler.build(product, configuration, Some(&session)) {
        Ok(result) => result.into_parts(),
        Err(diagnostics) => {
            session.finish(false);

            return failure(diagnostics, output_format);
        }
    };

    let compilation_succeeded = outputs.iter().all(ToolOutput::success);

    session.finish(compilation_succeeded);

    if !compilation_succeeded {
        return result_from_outputs(outputs, output_format);
    }

    let Some(executable) = executable else {
        return failure(
            selection_diagnostics(DiagnosticProjectSelectionProblem::MissingExecutableOutput),
            output_format,
        );
    };

    let _publication_guard = match compiler.lock_published_product(product, configuration) {
        Ok(guard) => guard,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let exit_code = match run_project_process(&executable, &arguments, workspace_root) {
        Ok(exit_code) => exit_code,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let mut result = result_from_outputs(outputs, output_format);
    result.set_exit_code(exit_code);

    result
}

#[expect(
    clippy::too_many_arguments,
    reason = "test routing keeps the selected project, process, and command inputs explicit"
)]
fn run_tests(
    workspace_root: &Path,
    graph: &ProjectGraph,
    toolchain: &Toolchain,
    worker_count: usize,
    selection: &TackSelection,
    configuration: crate::tack::model::TackBuildConfiguration,
    no_build: bool,
    batch_request: Option<&Path>,
    native_link_inputs: Vec<String>,
    options: crate::tack::model::TackTestOptions,
    profile: Option<&TackProfileConfiguration>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    progress: &WorkflowProgress,
) -> TackRunResult {
    let batch_request = match batch_request.map(load_test_batch_request).transpose() {
        Ok(request) => request,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let products = match select_products(graph, selection, ProductSelectionKind::Test, false) {
        Ok(products) => products,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let mut compiler = ProjectCompiler::new(
        workspace_root,
        graph,
        toolchain,
        worker_count,
        output_format,
        profile,
        executor,
    )
    .with_native_link_inputs(native_link_inputs);

    let mut outputs = Vec::new();
    let mut hosts = Vec::new();
    let mut retained_generations = Vec::new();
    let mut build_provenance = TestBuildProvenance::new(no_build);

    for product in products {
        if no_build {
            let expected = match compiler.test_product_identity(&product, configuration) {
                Ok(identity) => identity,
                Err(diagnostics) => return failure(diagnostics, output_format),
            };

            let retained =
                match retain_matching_test_product(&compiler, &product, configuration, &expected) {
                    Ok(retained) => retained,
                    Err(diagnostics) => return failure(diagnostics, output_format),
                };

            let host = match test_host_from_generation(&retained) {
                Ok(host) => host,
                Err(diagnostics) => return failure(diagnostics, output_format),
            };

            hosts.push(host);

            build_provenance.push(product.product_identity(), retained.identity().to_hex());

            retained_generations.push(retained);

            continue;
        }

        let plan = match compiler.build_progress_plan(&product, configuration) {
            Ok(plan) => plan,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        let session = progress.begin(plan);

        let (build, expected) = match compiler.build_test(&product, configuration, Some(&session)) {
            Ok(result) => result,
            Err(diagnostics) => {
                session.finish(false);

                return failure(diagnostics, output_format);
            }
        };

        let (product_outputs, _) = build.into_parts();

        let compilation_succeeded = product_outputs.iter().all(ToolOutput::success);

        session.finish(compilation_succeeded);

        outputs.extend(product_outputs);

        if !compilation_succeeded {
            continue;
        }

        let retained =
            match retain_matching_test_product(&compiler, &product, configuration, &expected) {
                Ok(retained) => retained,
                Err(diagnostics) => return failure(diagnostics, output_format),
            };

        let host = match test_host_from_generation(&retained) {
            Ok(host) => host,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        hosts.push(host);

        build_provenance.push(product.product_identity(), retained.identity().to_hex());

        retained_generations.push(retained);
    }

    let _retained_generations = retained_generations;

    let mut result = result_from_outputs(outputs, output_format);

    if result.exit_code() != ExitCode::SUCCESS {
        return result;
    }

    if let Some(batch_request) = batch_request {
        let (reports, rendered) = match crate::tack::testing::execute_batch(
            workspace_root,
            hosts,
            &batch_request,
            worker_count,
            progress.interactive(),
            &build_provenance,
        ) {
            Ok(report) => report,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        result.replace_stdout(rendered);

        if reports.iter().any(|(_, report)| !report.succeeded()) {
            result.set_exit_code(ExitCode::FAILURE);
        }
    } else {
        let (report, rendered) = match crate::tack::testing::execute(
            workspace_root,
            hosts,
            &options,
            worker_count,
            output_format,
            progress.interactive(),
            &build_provenance,
        ) {
            Ok(report) => report,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        result.replace_stdout(rendered);

        if !report.succeeded() {
            result.set_exit_code(ExitCode::FAILURE);
        }
    }

    result
}

fn retain_matching_test_product(
    compiler: &ProjectCompiler<'_>,
    product: &PlannedProduct,
    configuration: crate::tack::model::TackBuildConfiguration,
    expected: &bray_emitter::ProductBuildIdentity,
) -> Result<bray_emitter::RetainedProductGeneration, DiagnosticBag> {
    let retained = compiler.retain_test_product(product, configuration)?;
    let identity = product.product_identity();

    let Some(actual) = retained.build_identity() else {
        return Err(selection_diagnostics(
            DiagnosticProjectSelectionProblem::MissingReusableBuildIdentity(identity),
        ));
    };

    if let Some(part) = actual.mismatch(expected) {
        return Err(bray_tooling::reusable_build_identity_mismatch_diagnostics(
            identity, part,
        ));
    }

    Ok(retained)
}

fn test_host_from_generation(
    retained: &bray_emitter::RetainedProductGeneration,
) -> Result<BuiltTestHost, DiagnosticBag> {
    let executable = retained
        .artifact_path(bray_emitter::ArtifactKind::Executable, 0)
        .ok_or_else(|| {
            selection_diagnostics(DiagnosticProjectSelectionProblem::MissingTestExecutableOutput)
        })?;

    let test_catalog = retained
        .artifact_path(bray_emitter::ArtifactKind::TestCatalog, 0)
        .ok_or_else(|| {
            selection_diagnostics(DiagnosticProjectSelectionProblem::MissingTestCatalogOutput)
        })?;

    BuiltTestHost::try_new(executable.to_path_buf(), test_catalog.to_path_buf())
        .ok_or_else(|| selection_diagnostics(DiagnosticProjectSelectionProblem::MissingTestHost))
}

fn load_test_batch_request(path: &Path) -> Result<TestBatchRequest, DiagnosticBag> {
    let bytes = read_test_batch_request(path)?;

    let request = serde_json::from_slice::<TestBatchRequest>(&bytes).map_err(|error| {
        let problem = DiagnosticDocumentParseKind::from(error.classify());

        test_batch_document_diagnostics(path, problem)
    })?;

    request
        .validate()
        .map_err(|_| test_batch_document_diagnostics(path, DiagnosticDocumentParseKind::Schema))?;

    Ok(request)
}

fn read_test_batch_request(path: &Path) -> Result<Vec<u8>, DiagnosticBag> {
    let file = std::fs::File::open(path).map_err(|error| {
        operation_diagnostics(DiagnosticProjectCommandFailure::Io {
            operation: DiagnosticProjectOperation::TestBatchRequest,
            path: path.to_path_buf(),
            error: DiagnosticIoErrorKind::from(error.kind()),
        })
    })?;

    let limit = u64::try_from(MAXIMUM_TEST_BATCH_REQUEST_BYTES)
        .map_err(|_| test_batch_document_diagnostics(path, DiagnosticDocumentParseKind::Schema))?
        .saturating_add(1);

    let mut bytes = Vec::with_capacity(MAXIMUM_TEST_BATCH_REQUEST_BYTES.saturating_add(1));

    file.take(limit).read_to_end(&mut bytes).map_err(|error| {
        operation_diagnostics(DiagnosticProjectCommandFailure::Io {
            operation: DiagnosticProjectOperation::TestBatchRequest,
            path: path.to_path_buf(),
            error: DiagnosticIoErrorKind::from(error.kind()),
        })
    })?;

    if bytes.len() > MAXIMUM_TEST_BATCH_REQUEST_BYTES {
        return Err(test_batch_document_diagnostics(
            path,
            DiagnosticDocumentParseKind::Schema,
        ));
    }

    Ok(bytes)
}

fn test_batch_document_diagnostics(
    path: &Path,
    problem: DiagnosticDocumentParseKind,
) -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::Document {
        operation: DiagnosticProjectOperation::TestBatchRequest,
        path: Some(path.to_path_buf()),
        problem,
        detail: None,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "inspection routing keeps each independently selected CLI value explicit"
)]
fn run_inspect(
    workspace_root: &Path,
    graph: &ProjectGraph,
    toolchain: &Toolchain,
    worker_count: usize,
    selection: &TackSelection,
    inspection: TackInspection,
    source_id: u32,
    position: Option<u32>,
    source: bool,
    profile: Option<&TackProfileConfiguration>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
) -> TackRunResult {
    if inspection == TackInspection::Project {
        return match render_project_inspection(graph, output_format) {
            Ok(stdout) => TackRunResult::with_output(
                ExitCode::SUCCESS,
                DiagnosticBag::new(),
                output_format,
                stdout,
                String::new(),
            ),
            Err(diagnostics) => failure(diagnostics, output_format),
        };
    }

    let products = match select_products(graph, selection, ProductSelectionKind::Any, true) {
        Ok(products) => products,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let Some(product) = products.first() else {
        return failure(
            selection_diagnostics(
                DiagnosticProjectSelectionProblem::UnsupportedInspectionProduct(
                    selection.product.as_deref().unwrap_or("*").to_owned(),
                ),
            ),
            output_format,
        );
    };

    let mut compiler = ProjectCompiler::new(
        workspace_root,
        graph,
        toolchain,
        worker_count,
        output_format,
        profile,
        executor,
    );

    let outputs = match compiler.inspect(product, inspection, source_id, position, source) {
        Ok(outputs) => outputs,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let all_succeeded = outputs.iter().all(ToolOutput::success);

    if all_succeeded {
        return outputs
            .into_iter()
            .next_back()
            .map(|output| result_from_output(output, output_format))
            .unwrap_or_else(|| {
                failure(
                    operation_diagnostics(DiagnosticProjectCommandFailure::MissingResult(
                        DiagnosticProjectOperation::Inspection,
                    )),
                    output_format,
                )
            });
    }

    result_from_outputs(outputs, output_format)
}

#[expect(
    clippy::too_many_arguments,
    reason = "language-server routing keeps the selected tool and protocol streams explicit"
)]
fn run_language_server(
    workspace_root: PathBuf,
    graph: &ProjectGraph,
    worker_count: usize,
    target: Option<&str>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    input: Box<dyn Read + Send>,
    protocol_output: &mut dyn Write,
) -> TackRunResult {
    let target = match select_target(graph, target) {
        Ok(Some(target)) => target,
        Ok(None) => match graph.targets().first() {
            Some(target) => target,
            None => {
                return failure(
                    selection_diagnostics(DiagnosticProjectSelectionProblem::TargetRequired),
                    output_format,
                );
            }
        },
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let mut request = ToolRequest::new(Tool::LanguageServer, &workspace_root);

    request
        .arg("--workspace")
        .arg(workspace_root.into_os_string())
        .arg("--target")
        .arg(target.name())
        .arg("--cpu-count")
        .arg(worker_count.to_string());

    match executor.serve(request, input, protocol_output) {
        Ok(output) => result_from_output(output, output_format),
        Err(error) => failure(
            operation_diagnostics(tool_execution_failure(
                DiagnosticProjectOperation::LanguageServerProcess,
                error,
            )),
            output_format,
        ),
    }
}

fn run_format(
    workspace_root: &Path,
    worker_count: usize,
    check: bool,
    configuration: Option<PathBuf>,
    files: Vec<PathBuf>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    stdin: &mut dyn Read,
) -> TackRunResult {
    let explicit_files = !files.is_empty();

    let graph = if configuration.is_none() || files.is_empty() {
        match load_graph(workspace_root, false) {
            Ok(graph) => Some(graph),
            Err(diagnostics) => return failure(diagnostics, output_format),
        }
    } else {
        None
    };

    let mut files = if let Some(graph) = graph.as_ref().filter(|_| files.is_empty()) {
        root_source_files(graph, workspace_root)
    } else {
        files
    };

    let needs_invocation_directory = explicit_files && files.as_slice() != [PathBuf::from("-")]
        || configuration
            .as_ref()
            .is_some_and(|path| path.is_relative());

    let invocation_directory = if needs_invocation_directory {
        match std::env::current_dir() {
            Ok(directory) => Some(directory),
            Err(error) => {
                return failure(
                    operation_diagnostics(DiagnosticProjectCommandFailure::HostIo {
                        operation: DiagnosticProjectOperation::FormatterWorkingDirectory,
                        error: DiagnosticIoErrorKind::from(error.kind()),
                    }),
                    output_format,
                );
            }
        }
    } else {
        None
    };

    if explicit_files && files.as_slice() != [PathBuf::from("-")] {
        let invocation_directory = invocation_directory
            .as_deref()
            .unwrap_or_else(|| unreachable!("explicit files require an invocation directory"));

        for path in &mut files {
            if path.is_relative() {
                *path = invocation_directory.join(&*path);
            }
        }

        files.sort_unstable();
    }

    let configuration = configuration.map(|path| {
        if path.is_relative() {
            invocation_directory
                .as_deref()
                .unwrap_or_else(|| unreachable!("relative configuration requires a directory"))
                .join(path)
        } else {
            path
        }
    });

    let configuration = configuration.or_else(|| {
        graph
            .as_ref()
            .and_then(ProjectGraph::formatter_configuration)
            .map(|path| path.beneath(workspace_root))
    });

    let mut request = ToolRequest::new(Tool::Formatter, workspace_root);

    request
        .arg("--format")
        .arg(output_format.as_str())
        .arg("--cpu-count")
        .arg(worker_count.to_string());

    if let Some(configuration) = configuration {
        request.arg("--config").arg(configuration.into_os_string());
    }

    if check {
        request.arg("--check");
    }

    if files.as_slice() == [PathBuf::from("-")] {
        let mut bytes = Vec::new();

        if let Err(error) = stdin.read_to_end(&mut bytes) {
            return failure(
                operation_diagnostics(DiagnosticProjectCommandFailure::HostIo {
                    operation: DiagnosticProjectOperation::FormatterInput,
                    error: DiagnosticIoErrorKind::from(error.kind()),
                }),
                output_format,
            );
        }

        request.input(bytes);
    }

    request.args(files.into_iter().map(PathBuf::into_os_string));

    match executor.capture(request) {
        Ok(output) => result_from_output(output, output_format),
        Err(error) => failure(
            operation_diagnostics(tool_execution_failure(
                DiagnosticProjectOperation::FormatterProcess,
                error,
            )),
            output_format,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::io::{Cursor, Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::Mutex;

    use bray_diagnostics::DiagnosticKind;
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{load_test_batch_request, run_tack_result_with_input};
    use crate::tack::tool::{
        Tool, ToolExecutionError, ToolExecutor, ToolOutput, ToolRequest, ToolStream,
    };
    use crate::test_support::{ProjectWorkspace, unique_temporary_directory};

    #[derive(Debug)]
    struct RecordedRequest {
        tool: Tool,
        arguments: Vec<OsString>,
        working_directory: PathBuf,
        input: Option<Vec<u8>>,
    }

    #[derive(Default)]
    struct RecordingExecutor {
        requests: Mutex<Vec<RecordedRequest>>,
    }

    struct InvalidProtocolExecutor;

    impl ToolExecutor for InvalidProtocolExecutor {
        fn capture(&self, _request: ToolRequest) -> Result<ToolOutput, ToolExecutionError> {
            Err(ToolExecutionError::StreamThreadPanicked(
                ToolStream::StandardOutput,
            ))
        }

        fn serve(
            &self,
            _request: ToolRequest,
            _input: Box<dyn Read + Send>,
            output: &mut dyn Write,
        ) -> Result<ToolOutput, ToolExecutionError> {
            output
                .write_all(&[0xff])
                .map_err(|error| ToolExecutionError::StreamIo {
                    stream: ToolStream::StandardOutput,
                    error: error.kind(),
                })?;

            Ok(ToolOutput::new(true, String::new(), String::new()))
        }
    }

    impl RecordingExecutor {
        fn requests(&self) -> Vec<RecordedRequest> {
            let mut requests = self
                .requests
                .lock()
                .unwrap_or_else(|error| error.into_inner());

            std::mem::take(&mut *requests)
        }

        fn record(&self, request: &ToolRequest) {
            self.requests
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(RecordedRequest {
                    tool: request.tool(),
                    arguments: request.arguments().to_vec(),
                    working_directory: request.working_directory().to_path_buf(),
                    input: request.input_bytes().map(<[u8]>::to_vec),
                });
        }

        fn publish_compiler_outputs(&self, request: &ToolRequest) -> Result<(), ()> {
            if request.tool() != Tool::Compiler
                || !has_argument_pair(request.arguments(), "--artifact", "executable")
            {
                return Ok(());
            }

            let output_root = argument_value(request.arguments(), "--output").ok_or(())?;

            let output_directory =
                argument_value(request.arguments(), "--managed-output-directory").ok_or(())?;

            let product = argument_value(request.arguments(), "--product").ok_or(())?;
            let output = Path::new(output_root).join(output_directory);

            std::fs::create_dir_all(&output).map_err(|_| ())?;
            std::fs::write(output.join(product), b"test executable").map_err(|_| ())?;

            Ok(())
        }
    }

    impl ToolExecutor for RecordingExecutor {
        fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ToolExecutionError> {
            self.publish_compiler_outputs(&request).map_err(|()| {
                ToolExecutionError::StreamThreadPanicked(ToolStream::StandardOutput)
            })?;

            self.record(&request);

            let json = request
                .arguments()
                .windows(2)
                .any(|pair| pair == ["--format", "json"]);

            Ok(ToolOutput::new(
                true,
                if json {
                    "{\"has_errors\":false,\"diagnostics\":[]}\n".to_owned()
                } else {
                    String::new()
                },
                String::new(),
            ))
        }

        fn serve(
            &self,
            request: ToolRequest,
            mut input: Box<dyn Read + Send>,
            output: &mut dyn Write,
        ) -> Result<ToolOutput, ToolExecutionError> {
            self.record(&request);

            std::io::copy(&mut input, output).map_err(|error| ToolExecutionError::StreamIo {
                stream: ToolStream::StandardOutput,
                error: error.kind(),
            })?;

            Ok(ToolOutput::new(true, String::new(), String::new()))
        }
    }

    struct DriverExecutor;

    impl ToolExecutor for DriverExecutor {
        fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ToolExecutionError> {
            assert_eq!(
                request.tool(),
                Tool::Compiler,
                "driver test executor only accepts compiler requests"
            );

            let result = bray_driver::run_result(
                std::iter::once(OsString::from("brayc")).chain(request.arguments().iter().cloned()),
            );

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            bray_tooling::write_diagnostics(
                result.diagnostics(),
                None,
                result.output_format(),
                &mut stdout,
                &mut stderr,
            )
            .map_err(|error| ToolExecutionError::StreamIo {
                stream: ToolStream::StandardOutput,
                error: error.kind(),
            })?;

            Ok(ToolOutput::new(
                result.exit_code() == ExitCode::SUCCESS,
                String::from_utf8(stdout)
                    .map_err(|_| ToolExecutionError::InvalidUtf8(ToolStream::StandardOutput))?,
                String::from_utf8(stderr)
                    .map_err(|_| ToolExecutionError::InvalidUtf8(ToolStream::StandardError))?,
            ))
        }

        fn serve(
            &self,
            _: ToolRequest,
            _: Box<dyn Read + Send>,
            _: &mut dyn Write,
        ) -> Result<ToolOutput, ToolExecutionError> {
            panic!("driver test executor does not serve streaming tools")
        }
    }

    fn argument_value<'arguments>(
        arguments: &'arguments [OsString],
        name: &str,
    ) -> Option<&'arguments OsStr> {
        arguments
            .windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].as_os_str())
    }

    #[test]
    fn initialized_project_is_accepted_by_check() {
        let parent = unique_temporary_directory();
        let workspace = parent.join("sample-project");
        let executor = RecordingExecutor::default();

        let initialization = run_tack_result_with_input(
            [
                "bray".into(),
                "init".into(),
                workspace.as_os_str().to_os_string(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(initialization.exit_code(), ExitCode::SUCCESS);
        assert!(executor.requests().is_empty());

        let check = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.as_os_str().to_os_string(),
                "check".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(check.exit_code(), ExitCode::SUCCESS, "{check:#?}");

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("check should invoke exactly one compiler: {requests:#?}");
        };

        assert!(has_argument_pair(
            &request.arguments,
            "--package",
            "sample-project"
        ));

        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn no_build_test_rejects_missing_retained_state_without_invoking_the_compiler() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        workspace.write(
            "app/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.application",
                "version": {"workspace": true},
                "features": [],
                "source_roots": [{"name": "main", "path": "src"}],
                "products": [{
                    "name": "tests",
                    "kind": "test",
                    "source_roots": ["main"],
                    "targets": ["native"],
                    "dependencies": [],
                    "outputs": ["executable"]
                }]
            }"#,
        );

        let result = run_tack_result_with_input(
            [
                OsString::from("bray"),
                OsString::from("--workspace"),
                workspace.path().as_os_str().to_os_string(),
                OsString::from("test"),
                OsString::from("--no-build"),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::FAILURE);
        assert!(executor.requests().is_empty());
    }

    #[test]
    fn storage_and_clean_use_managed_state_without_invoking_a_toolchain() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();
        let root = workspace.path().join("build");
        let target = bray_target::TargetIdentity::try_new("x86_64-unknown-linux-gnu").unwrap();

        let cache =
            bray_emitter::ManagedCache::thin_lto(&root, &target, "test-llvm", &|| false).unwrap();

        let file = cache.directory().join("content");

        std::fs::write(&file, b"optional cache").unwrap();
        drop(cache);

        for (command, dry_run, remains) in [
            ("storage", false, true),
            ("clean", true, true),
            ("clean", false, false),
        ] {
            let mut arguments = vec![
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_owned(),
                "--format".into(),
                "json".into(),
                "--toolchain-root".into(),
                workspace.path().join("absent-toolchain").into_os_string(),
                command.into(),
                "--kind".into(),
                "caches".into(),
                "--target".into(),
                "native".into(),
                "--toolchain-revision".into(),
                "test-llvm".into(),
            ];

            if dry_run {
                arguments.push("--dry-run".into());
            }

            let result = run_tack_result_with_input(arguments, &executor, Cursor::new(Vec::new()));
            assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");
            let report: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
            assert_eq!(report["kind"], "storage_report");
            assert_eq!(report["clean"], command == "clean");
            assert_eq!(report["dry_run"], dry_run);
            let row = &report["entries"][0];
            assert_eq!(row["bytes"], 14);
            assert_eq!(row["shared_bytes"], 0);
            assert_eq!(row["target"], target.as_str());
            assert_eq!(row["removed"], !remains);
            assert_eq!(file.exists(), remains);
        }

        assert!(executor.requests().is_empty());
    }

    #[test]
    fn check_invokes_the_compiler_for_the_selected_project_product() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "check".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("check should invoke exactly one compiler: {requests:#?}");
        };

        assert_eq!(request.tool, Tool::Compiler);
        assert_eq!(request.working_directory, workspace.path());

        assert!(has_argument_pair(
            &request.arguments,
            "--package",
            "example.application"
        ));

        assert!(has_argument_pair(
            &request.arguments,
            "--package-version",
            "0.1.0"
        ));

        assert!(has_argument_pair(
            &request.arguments,
            "--product",
            "application"
        ));

        assert!(request.arguments.iter().any(|argument| argument == "check"));

        assert!(
            request
                .arguments
                .iter()
                .map(PathBuf::from)
                .any(|argument| argument.ends_with("src/main.bray"))
        );
    }

    #[test]
    fn check_forwards_profiling_and_assigns_a_distinct_machine_report() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "--profile=trace".into(),
                "--profile-output".into(),
                "profiles".into(),
                "check".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("check should invoke exactly one compiler: {requests:#?}");
        };

        assert!(has_argument_pair(&request.arguments, "--profile", "trace"));

        let profile_output = request
            .arguments
            .windows(2)
            .find(|pair| pair[0] == "--profile-output")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap_or_else(|| panic!("compiler request must retain a profile output"));

        assert_eq!(
            profile_output.parent(),
            Some(workspace.path().join("profiles").as_path())
        );

        assert!(
            profile_output
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains("application"))
        );
    }

    #[test]
    fn dependency_interfaces_are_produced_before_the_importing_product() {
        let workspace = ProjectWorkspace::with_vendor();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "check".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let requests = executor.requests();

        let [dependency, product] = requests.as_slice() else {
            panic!("dependency and root compiler requests should be recorded: {requests:#?}");
        };

        assert!(
            dependency
                .arguments
                .iter()
                .any(|argument| argument == "--emit-interface")
        );

        assert!(has_argument_pair(
            &product.arguments,
            "--dependency-product",
            "example.math/math"
        ));

        assert!(
            product
                .arguments
                .iter()
                .any(|argument| argument == "--dependency-interface")
        );
    }

    #[test]
    fn standard_library_source_check_materializes_its_tested_library() {
        let workspace = ProjectWorkspace::standard_library();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "--standard-library-source".into(),
                "check".into(),
                "--package".into(),
                "std".into(),
                "--product".into(),
                "api".into(),
                "--target".into(),
                "native".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

        let requests = executor.requests();

        let [library, api] = requests.as_slice() else {
            panic!(
                "standard library source check must compile its library before API: {requests:#?}"
            );
        };

        assert!(has_argument_pair(
            &library.arguments,
            "--product",
            "library"
        ));

        assert!(
            library
                .arguments
                .iter()
                .any(|argument| argument == "--emit-interface")
        );

        assert!(
            library
                .arguments
                .iter()
                .any(|argument| argument == "--standard-library-source")
        );

        assert!(has_argument_pair(
            &library.arguments,
            "--platform-service",
            "platform.context.identity=std.platform.context_identity"
        ));

        assert!(has_argument_pair(&api.arguments, "--product", "api"));

        assert!(has_argument_pair(
            &api.arguments,
            "--dependency-product",
            "std/library"
        ));

        assert!(
            api.arguments
                .iter()
                .any(|argument| argument == "--dependency-interface")
        );

        assert!(
            api.arguments
                .iter()
                .any(|argument| argument == "--standard-library-source")
        );

        assert!(
            !api.arguments
                .iter()
                .any(|argument| argument == "--standard-library-root")
        );
    }

    #[test]
    fn standard_library_source_build_retains_native_provider_root() {
        let workspace = ProjectWorkspace::standard_library();
        let toolchain = unique_temporary_directory();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "--toolchain-root".into(),
                toolchain.as_os_str().to_os_string(),
                "--standard-library-source".into(),
                "build".into(),
                "--package".into(),
                "std".into(),
                "--product".into(),
                "api".into(),
                "--target".into(),
                "native".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

        let requests = executor.requests();

        let [library, api] = requests.as_slice() else {
            panic!(
                "standard library source build must compile its library before API: {requests:#?}"
            );
        };

        assert!(
            !library
                .arguments
                .iter()
                .any(|argument| argument == "--standard-library-root")
        );

        let standard_library = std::path::absolute(&toolchain)
            .unwrap_or_else(|error| panic!("test toolchain path should resolve: {error:?}"))
            .join("lib")
            .join("bray")
            .join("standard-library");

        assert!(
            api.arguments.windows(2).any(|pair| {
                pair[0] == "--standard-library-provider-root"
                    && pair[1] == standard_library.as_os_str()
            }),
            "native API request must retain provider root {standard_library:?}: {:#?}",
            api.arguments
        );

        assert!(
            !api.arguments
                .iter()
                .any(|argument| argument == "--standard-library-root")
        );

        assert!(has_argument_pair(
            &api.arguments,
            "--dependency-product",
            "std/library"
        ));

        assert!(
            api.arguments
                .iter()
                .any(|argument| argument == "--standard-library-source")
        );
    }

    #[test]
    fn build_forwards_the_manifest_artifact_set_to_the_compiler() {
        let workspace = ProjectWorkspace::basic();
        let toolchain = unique_temporary_directory();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "--toolchain-root".into(),
                toolchain.as_os_str().to_os_string(),
                "build".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        assert!(
            result
                .stderr()
                .contains("Building example.application/application [debug]")
        );

        assert!(result.stderr().contains("✓ Compiled example.application"));
        assert!(result.stderr().contains("✓ Finished application"));

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("build should invoke exactly one compiler: {requests:#?}");
        };

        assert!(has_argument_pair(
            &request.arguments,
            "--artifact",
            "executable"
        ));

        let standard_library = std::path::absolute(&toolchain)
            .unwrap_or_else(|error| panic!("test toolchain path should resolve: {error:?}"))
            .join("lib")
            .join("bray")
            .join("standard-library");

        assert!(
            request.arguments.windows(2).any(|pair| {
                pair[0] == "--standard-library-root" && pair[1] == standard_library.as_os_str()
            }),
            "compiler request should contain standard-library root {standard_library:?}: {:#?}",
            request.arguments
        );

        let output_root = request
            .arguments
            .windows(2)
            .find(|pair| pair[0] == "--output")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap_or_else(|| panic!("debug build must select an output directory"));

        assert!(output_root.ends_with("build"));

        assert!(has_argument_pair(
            &request.arguments,
            "--managed-output-directory",
            "native/debug/example.application"
        ));

        let runtime = std::path::absolute(&toolchain)
            .unwrap_or_else(|error| panic!("test toolchain path should resolve: {error:?}"))
            .join("lib")
            .join("bray")
            .join("runtime")
            .join("x86_64-unknown-linux-gnu")
            .join("bray-runtime.brayrt");

        assert!(
            request
                .arguments
                .windows(2)
                .any(|pair| { pair[0] == "--runtime-artifact" && pair[1] == runtime.as_os_str() }),
            "compiler request should contain runtime metadata {runtime:?}: {:#?}",
            request.arguments
        );
    }

    #[test]
    fn installed_toolchain_without_runtime_reports_exact_path_for_native_commands() {
        let workspace = ProjectWorkspace::basic();

        workspace.write(
            "app/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.application",
                "version": {"workspace": true},
                "features": [],
                "source_roots": [{"name": "main", "path": "src"}],
                "products": [
                    {
                        "name": "application",
                        "kind": "executable",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "dependencies": [],
                        "outputs": ["executable"]
                    },
                    {
                        "name": "tests",
                        "kind": "test",
                        "source_roots": ["main"],
                        "targets": ["native"],
                        "dependencies": [],
                        "outputs": ["executable"]
                    }
                ]
            }"#,
        );

        let toolchain = unique_temporary_directory();
        let standard_library = toolchain.join("lib/bray/standard-library");

        std::fs::create_dir_all(&standard_library).unwrap_or_else(|error| {
            panic!("synthetic standard library directory must be created: {error}")
        });

        let runtime = std::path::absolute(&toolchain)
            .unwrap_or_else(|error| panic!("test toolchain path must resolve: {error}"))
            .join("lib")
            .join("bray")
            .join("runtime")
            .join("x86_64-unknown-linux-gnu")
            .join("bray-runtime.brayrt");

        for command in ["build", "run", "test"] {
            let result = run_tack_result_with_input(
                [
                    OsString::from("bray"),
                    OsString::from("--workspace"),
                    workspace.path().as_os_str().to_os_string(),
                    OsString::from("--toolchain-root"),
                    toolchain.as_os_str().to_os_string(),
                    OsString::from(command),
                ],
                &DriverExecutor,
                Cursor::new(Vec::new()),
            );

            assert_eq!(result.exit_code(), ExitCode::FAILURE, "{command}");

            assert!(
                result.stderr().contains("E1116"),
                "{command}: {}",
                result.stderr()
            );

            assert!(
                result.stderr().contains(&runtime.display().to_string()),
                "{command}: {}",
                result.stderr()
            );

            assert!(!result.stderr().contains("E1106"), "{command}");
        }

        std::fs::remove_dir_all(&toolchain)
            .unwrap_or_else(|error| panic!("synthetic toolchain must be removed: {error}"));
    }

    #[test]
    fn release_build_selects_release_compiler_policy_and_output_directory() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "build".into(),
                "--release".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("build should invoke exactly one compiler: {requests:#?}");
        };

        assert!(
            request
                .arguments
                .iter()
                .any(|argument| argument == "--release")
        );

        let output_root = request
            .arguments
            .windows(2)
            .find(|pair| pair[0] == "--output")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap_or_else(|| panic!("release build must select an output directory"));

        assert!(output_root.ends_with("build"));

        assert!(has_argument_pair(
            &request.arguments,
            "--managed-output-directory",
            "native/release/example.application"
        ));
    }

    #[test]
    fn build_does_not_substitute_a_default_product_artifact() {
        let workspace = ProjectWorkspace::basic();
        workspace.set_product_outputs(&["backend_ir"]);

        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "build".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("build should invoke exactly one compiler: {requests:#?}");
        };

        assert!(has_argument_pair(
            &request.arguments,
            "--artifact",
            "backend-ir"
        ));

        assert!(!has_argument_pair(
            &request.arguments,
            "--artifact",
            "executable"
        ));
    }

    #[test]
    fn formatter_standard_input_crosses_the_process_boundary() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "--cpu-count".into(),
                "3".into(),
                "fmt".into(),
                "-".into(),
            ],
            &executor,
            Cursor::new(b"module app;".to_vec()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("format should invoke exactly one formatter: {requests:#?}");
        };

        assert_eq!(request.tool, Tool::Formatter);
        assert_eq!(request.input.as_deref(), Some(b"module app;".as_slice()));
        assert!(has_argument_pair(&request.arguments, "--cpu-count", "3"));
    }

    #[test]
    fn explicit_formatter_paths_remain_relative_to_the_invocation_directory() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "fmt".into(),
                "relative.bray".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("format should invoke exactly one formatter: {requests:#?}");
        };

        let expected = std::env::current_dir()
            .unwrap_or_else(|error| panic!("test invocation directory must exist: {error:?}"))
            .join("relative.bray");

        assert!(
            request
                .arguments
                .iter()
                .any(|argument| argument == expected.as_os_str())
        );
    }

    #[test]
    fn formatter_default_input_is_the_root_package_source_graph() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "fmt".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("format should invoke exactly one formatter: {requests:#?}");
        };

        assert!(
            request
                .arguments
                .iter()
                .map(PathBuf::from)
                .any(|path| path.ends_with("app/src/main.bray"))
        );
    }

    #[test]
    fn formatter_configuration_paths_follow_workspace_and_invocation_ownership() {
        let workspace = ProjectWorkspace::basic();
        workspace.set_formatter_configuration("configuration/workspace.json");

        let workspace_executor = RecordingExecutor::default();

        let workspace_result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "fmt".into(),
            ],
            &workspace_executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(workspace_result.exit_code(), ExitCode::SUCCESS);

        let workspace_requests = workspace_executor.requests();

        let [workspace_request] = workspace_requests.as_slice() else {
            panic!("workspace format should invoke exactly one formatter");
        };

        assert!(workspace_request.arguments.windows(2).any(|pair| {
            pair[0] == "--config"
                && PathBuf::from(&pair[1]).ends_with("configuration/workspace.json")
        }));

        let override_executor = RecordingExecutor::default();

        let override_result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "fmt".into(),
                "--config".into(),
                "override.json".into(),
                "-".into(),
            ],
            &override_executor,
            Cursor::new(b"module app;".to_vec()),
        );

        assert_eq!(override_result.exit_code(), ExitCode::SUCCESS);

        let override_requests = override_executor.requests();

        let [override_request] = override_requests.as_slice() else {
            panic!("overridden format should invoke exactly one formatter");
        };

        let expected_override = std::env::current_dir()
            .unwrap_or_else(|error| panic!("test invocation directory must exist: {error:?}"))
            .join("override.json");

        assert!(has_argument_pair(
            &override_request.arguments,
            "--config",
            expected_override
        ));
    }

    #[test]
    fn project_inspection_remains_owned_by_tack() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "inspect".into(),
                "project".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert!(result.stdout().contains("example.application"));
        assert!(executor.requests().is_empty());
    }

    #[test]
    fn source_inspections_do_not_receive_unit_only_arguments() {
        for inspection in ["source", "tokens", "syntax", "declarations", "symbols"] {
            let workspace = ProjectWorkspace::basic();
            let executor = RecordingExecutor::default();

            let result = run_tack_result_with_input(
                [
                    "bray".into(),
                    "--workspace".into(),
                    workspace.path().as_os_str().to_os_string(),
                    "inspect".into(),
                    inspection.into(),
                ],
                &executor,
                Cursor::new(Vec::new()),
            );

            assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

            let requests = executor.requests();

            let [request] = requests.as_slice() else {
                panic!("source inspection should invoke exactly one compiler");
            };

            assert!(
                !request
                    .arguments
                    .iter()
                    .any(|argument| argument == "--source-id")
            );

            assert!(
                !request
                    .arguments
                    .iter()
                    .any(|argument| argument == "--offset")
            );
        }
    }

    #[test]
    fn project_load_failures_do_not_invoke_toolchain_processes() {
        let workspace = unique_temporary_directory();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.into_os_string(),
                "check".into(),
            ],
            &executor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::FAILURE);
        assert!(!result.diagnostics().is_empty());
        assert!(executor.requests().is_empty());
    }

    #[test]
    fn language_server_protocol_streams_through_the_lsp_process() {
        let workspace = ProjectWorkspace::basic();
        let executor = RecordingExecutor::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "lsp".into(),
                "--target".into(),
                "native".into(),
            ],
            &executor,
            Cursor::new(b"protocol".to_vec()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert_eq!(result.stdout(), "protocol");

        let requests = executor.requests();

        let [request] = requests.as_slice() else {
            panic!("language server should be invoked once: {requests:#?}");
        };

        assert_eq!(request.tool, Tool::LanguageServer);
        assert!(has_argument_pair(&request.arguments, "--target", "native"));
    }

    #[test]
    fn invalid_language_server_protocol_bytes_fail_with_a_structured_diagnostic() {
        let workspace = ProjectWorkspace::basic();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "lsp".into(),
            ],
            &InvalidProtocolExecutor,
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::ProjectCommandFailed,
        );
    }

    #[test]
    fn test_batch_request_files_are_bounded_before_deserialization() {
        let directory = unique_temporary_directory();
        let path = directory.join("batch.json");

        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test batch directory must be created: {error:?}"));

        let file = std::fs::File::create(&path)
            .unwrap_or_else(|error| panic!("test batch request must be created: {error:?}"));

        file.set_len(
            u64::try_from(bray_test_protocol::MAXIMUM_TEST_BATCH_REQUEST_BYTES + 1)
                .unwrap_or_else(|error| panic!("test request size must fit u64: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("test batch request size must be set: {error:?}"));

        let diagnostics = load_test_batch_request(&path)
            .expect_err("oversized test batch request must be rejected");

        std::fs::remove_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test batch directory must be removed: {error:?}"));

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::ProjectCommandFailed);
    }

    #[test]
    fn decoded_test_batch_requests_retain_protocol_limits() {
        let directory = unique_temporary_directory();
        let path = directory.join("batch.json");

        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test batch directory must be created: {error:?}"));

        let identity = "a".repeat(bray_test_protocol::MAXIMUM_TEST_BATCH_PLAN_IDENTITY_BYTES + 1);

        let document = format!(
            "{{\"plans\":[{{\"identity\":\"{identity}\",\"filters\":[],\"maximum_concurrency\":1,\"timeout_milliseconds\":null}}]}}"
        );

        std::fs::write(&path, document)
            .unwrap_or_else(|error| panic!("test batch request must be written: {error:?}"));

        let diagnostics = load_test_batch_request(&path)
            .expect_err("decoded oversized plan identity must be rejected");

        std::fs::remove_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test batch directory must be removed: {error:?}"));

        assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::ProjectCommandFailed);
    }

    fn has_argument_pair(arguments: &[OsString], name: &str, value: impl AsRef<OsStr>) -> bool {
        let value = value.as_ref();

        arguments
            .windows(2)
            .any(|pair| pair[0] == name && pair[1] == value)
    }
}
