// rust-style: allow(module-too-large, reason = "Tack command execution shares one project-loading and structured-result orchestration boundary")

use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_diagnostics::DiagnosticBag;
use bray_project::ProjectGraph;
use bray_tooling::{OutputFormat, write_diagnostic_groups};

use crate::tack::compiler::ProjectCompiler;
use crate::tack::error::{operation_diagnostics, selection_diagnostics};
use crate::tack::init::initialize_project;
use crate::tack::inspection::render_project_inspection;
use crate::tack::install::{install_git_repository, run_project_process};
use crate::tack::model::{
    TackCommand, TackInspection, TackInvocation, TackProfileConfiguration, TackSelection,
};
use crate::tack::output::{
    failure, result_from_operation, result_from_output, result_from_outputs,
};
use crate::tack::progress::WorkflowProgress;
use crate::tack::profile::run_profile_command;
use crate::tack::project::{
    ProductSelectionKind, load_graph, root_source_files, select_products, select_target,
};
use crate::tack::result::TackRunResult;
use crate::tack::testing::BuiltTestHost;
use crate::tack::tool::{NativeToolExecutor, Tool, ToolExecutor, ToolOutput, ToolRequest};
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

    if stdout.write_all(&protocol_output).is_err()
        || stderr.write_all(result.stderr().as_bytes()).is_err()
        || stdout.write_all(result.stdout().as_bytes()).is_err()
    {
        return ExitCode::FAILURE;
    }

    if !result.diagnostics().is_empty()
        && write_diagnostic_groups(
            result.diagnostic_groups(),
            result.output_format(),
            &mut stdout,
            &mut stderr,
        )
        .is_err()
    {
        return ExitCode::FAILURE;
    }

    result.exit_code()
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

    if let Ok(protocol_output) = String::from_utf8(protocol_output) {
        result.prepend_stdout(protocol_output);
    }

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

    if progress.write_to_result(&mut result).is_err() {
        return failure(
            operation_diagnostics("workflow_progress_output"),
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

    let workspace_root = match std::path::absolute(workspace_root) {
        Ok(workspace_root) => workspace_root,
        Err(_) => return failure(operation_diagnostics("workspace_path"), output_format),
    };

    match command {
        TackCommand::Init { directory, package } => {
            let directory = directory.unwrap_or(workspace_root);

            let directory = match std::path::absolute(directory) {
                Ok(directory) => directory,
                Err(_) => return failure(operation_diagnostics("workspace_path"), output_format),
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
            options,
        } => run_tests(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            configuration,
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
        } => run_inspect(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            inspection,
            source_id,
            position,
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
        TackCommand::Init { .. }
        | TackCommand::Format { .. }
        | TackCommand::ProfileShow { .. }
        | TackCommand::ProfileCompare { .. }
        | TackCommand::VendorInstall { .. } => {
            failure(operation_diagnostics("command_routing"), output_format)
        }
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
                let (product_outputs, _, _) = build.into_parts();

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
        return failure(selection_diagnostics("executable"), output_format);
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

    let (outputs, executable, _) = match compiler.build(product, configuration, Some(&session)) {
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
        return failure(selection_diagnostics("executable_output"), output_format);
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
    options: crate::tack::model::TackTestOptions,
    profile: Option<&TackProfileConfiguration>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    progress: &WorkflowProgress,
) -> TackRunResult {
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
    );

    let mut outputs = Vec::new();
    let mut hosts = Vec::new();

    for product in products {
        let plan = match compiler.build_progress_plan(&product, configuration) {
            Ok(plan) => plan,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        let session = progress.begin(plan);

        let (product_outputs, executable, test_catalog) =
            match compiler.build(&product, configuration, Some(&session)) {
                Ok(result) => result.into_parts(),
                Err(diagnostics) => {
                    session.finish(false);

                    return failure(diagnostics, output_format);
                }
            };

        let compilation_succeeded = product_outputs.iter().all(ToolOutput::success);

        session.finish(compilation_succeeded);

        outputs.extend(product_outputs);

        if !compilation_succeeded {
            continue;
        }

        let Some(executable) = executable else {
            return failure(
                selection_diagnostics("test_executable_output"),
                output_format,
            );
        };

        let Some(test_catalog) = test_catalog else {
            return failure(selection_diagnostics("test_catalog_output"), output_format);
        };

        let Some(host) = BuiltTestHost::try_new(executable, test_catalog) else {
            return failure(
                selection_diagnostics("test_host_publication"),
                output_format,
            );
        };

        hosts.push(host);
    }

    let mut result = result_from_outputs(outputs, output_format);

    if result.exit_code() != ExitCode::SUCCESS {
        return result;
    }

    let (report, rendered) = match crate::tack::testing::execute(
        workspace_root,
        hosts,
        &options,
        worker_count,
        output_format,
        progress.interactive(),
    ) {
        Ok(report) => report,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    result.replace_stdout(rendered);

    if !report.succeeded() {
        result.set_exit_code(ExitCode::FAILURE);
    }

    result
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
        return failure(selection_diagnostics("inspection_product"), output_format);
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

    let outputs = match compiler.inspect(product, inspection, source_id, position) {
        Ok(outputs) => outputs,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let all_succeeded = outputs.iter().all(ToolOutput::success);

    if all_succeeded {
        return outputs
            .into_iter()
            .next_back()
            .map(|output| result_from_output(output, output_format))
            .unwrap_or_else(|| failure(operation_diagnostics("inspection"), output_format));
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
            None => return failure(selection_diagnostics("target"), output_format),
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
        Err(()) => failure(
            operation_diagnostics("language_server_process"),
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
            Err(_) => {
                return failure(
                    operation_diagnostics("formatter_working_directory"),
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

        if stdin.read_to_end(&mut bytes).is_err() {
            return failure(operation_diagnostics("formatter_input"), output_format);
        }

        request.input(bytes);
    }

    request.args(files.into_iter().map(PathBuf::into_os_string));

    match executor.capture(request) {
        Ok(output) => result_from_output(output, output_format),
        Err(()) => failure(operation_diagnostics("formatter_process"), output_format),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::io::{Cursor, Read, Write};
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::sync::Mutex;

    use super::run_tack_result_with_input;
    use crate::tack::tool::{Tool, ToolExecutor, ToolOutput, ToolRequest};
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
    }

    impl ToolExecutor for RecordingExecutor {
        fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ()> {
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
        ) -> Result<ToolOutput, ()> {
            self.record(&request);

            std::io::copy(&mut input, output).map_err(|_| ())?;

            Ok(ToolOutput::new(true, String::new(), String::new()))
        }
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

        assert_eq!(profile_output.parent(), Some(workspace.path().join("profiles").as_path()));

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

        let output = request
            .arguments
            .windows(2)
            .find(|pair| pair[0] == "--output")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap_or_else(|| panic!("debug build must select an output directory"));

        assert!(output.ends_with("native/debug/example.application/application"));

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

        let output = request
            .arguments
            .windows(2)
            .find(|pair| pair[0] == "--output")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap_or_else(|| panic!("release build must select an output directory"));

        assert!(output.ends_with("native/release/example.application/application"));
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
                "language-server".into(),
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

    fn has_argument_pair(arguments: &[OsString], name: &str, value: impl AsRef<OsStr>) -> bool {
        let value = value.as_ref();

        arguments
            .windows(2)
            .any(|pair| pair[0] == name && pair[1] == value)
    }
}
