use std::ffi::OsString;
use std::io::{self, Read, Write};
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
use crate::tack::model::{TackCommand, TackInspection, TackInvocation, TackSelection};
use crate::tack::project::{
    ProductSelectionKind, load_graph, root_source_files, select_products, select_target,
};
use crate::tack::result::TackRunResult;
use crate::tack::tool::{NativeToolExecutor, Tool, ToolExecutor, ToolOutput, ToolRequest};
use crate::tack::toolchain::Toolchain;

/// Runs Bray Tack using independently installed toolchain executables.
pub fn run_tack(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    run_tack_with_io(
        arguments,
        &NativeToolExecutor,
        io::stdin(),
        &mut stdout,
        &mut stderr,
    )
}

/// Runs Bray Tack and returns its structured outcome.
pub fn run_tack_result(arguments: impl IntoIterator<Item = OsString>) -> TackRunResult {
    run_tack_result_with_input(arguments, &NativeToolExecutor, io::empty())
}

fn run_tack_with_io(
    arguments: impl IntoIterator<Item = OsString>,
    executor: &dyn ToolExecutor,
    stdin: impl Read + Send + 'static,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    let result =
        run_tack_result_with_input_and_output(arguments, executor, Box::new(stdin), stdout);

    if stdout.write_all(result.stdout().as_bytes()).is_err()
        || stderr.write_all(result.stderr().as_bytes()).is_err()
    {
        return ExitCode::FAILURE;
    }

    if !result.diagnostics().is_empty()
        && write_diagnostic_groups(
            result.diagnostic_groups(),
            result.output_format(),
            stdout,
            stderr,
        )
        .is_err()
    {
        return ExitCode::FAILURE;
    }

    result.exit_code()
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

    execute_invocation(invocation, executor, stdin, protocol_output)
}

fn execute_invocation(
    invocation: TackInvocation,
    executor: &dyn ToolExecutor,
    mut stdin: Box<dyn Read + Send>,
    protocol_output: &mut dyn Write,
) -> TackRunResult {
    let (workspace_root, toolchain_root, worker_count, output_format, command) =
        invocation.into_parts();

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
        TackCommand::Format { check, files } => {
            return run_format(
                &workspace_root,
                check,
                files,
                output_format,
                executor,
                stdin.as_mut(),
            );
        }
        _ => {}
    }

    let toolchain = match Toolchain::select(toolchain_root) {
        Ok(toolchain) => toolchain,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let graph = match load_graph(&workspace_root) {
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
            output_format,
            executor,
        ),
        TackCommand::Build(selection) => run_build(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            output_format,
            executor,
        ),
        TackCommand::Run {
            selection,
            arguments,
        } => run_one(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            arguments,
            output_format,
            executor,
        ),
        TackCommand::Test {
            selection,
            arguments,
        } => run_tests(
            &workspace_root,
            &graph,
            &toolchain,
            worker_count,
            &selection,
            arguments,
            output_format,
            executor,
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

fn run_build(
    workspace_root: &Path,
    graph: &ProjectGraph,
    toolchain: &Toolchain,
    worker_count: usize,
    selection: &TackSelection,
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
        executor,
    );

    let mut outputs = Vec::new();

    for product in products {
        match compiler.build(&product) {
            Ok((product_outputs, _)) => outputs.extend(product_outputs),
            Err(diagnostics) => return failure(diagnostics, output_format),
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
    arguments: Vec<OsString>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
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
        executor,
    );

    let (outputs, executable) = match compiler.build(product) {
        Ok(result) => result,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    if outputs.iter().any(|output| !output.success()) {
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
    arguments: Vec<OsString>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
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
        executor,
    );

    let mut outputs = Vec::new();
    let mut tests_succeeded = true;

    for product in products {
        let (product_outputs, executable) = match compiler.build(&product) {
            Ok(result) => result,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        let compilation_succeeded = product_outputs.iter().all(ToolOutput::success);

        outputs.extend(product_outputs);

        if compilation_succeeded {
            let Some(executable) = executable else {
                return failure(
                    selection_diagnostics("test_executable_output"),
                    output_format,
                );
            };

            match run_project_process(&executable, &arguments, workspace_root) {
                Ok(code) => tests_succeeded &= code == ExitCode::SUCCESS,
                Err(diagnostics) => return failure(diagnostics, output_format),
            }
        } else {
            tests_succeeded = false;
        }
    }

    let mut result = result_from_outputs(outputs, output_format);

    if !tests_succeeded {
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
    check: bool,
    files: Vec<PathBuf>,
    output_format: OutputFormat,
    executor: &dyn ToolExecutor,
    stdin: &mut dyn Read,
) -> TackRunResult {
    let explicit_files = !files.is_empty();

    let mut files = if files.is_empty() {
        let graph = match load_graph(workspace_root) {
            Ok(graph) => graph,
            Err(diagnostics) => return failure(diagnostics, output_format),
        };

        root_source_files(&graph, workspace_root)
    } else {
        files
    };

    if explicit_files && files.as_slice() != [PathBuf::from("-")] {
        let invocation_directory = match std::env::current_dir() {
            Ok(directory) => directory,
            Err(_) => {
                return failure(
                    operation_diagnostics("formatter_working_directory"),
                    output_format,
                );
            }
        };

        for path in &mut files {
            if path.is_relative() {
                *path = invocation_directory.join(&*path);
            }
        }
    }

    let mut request = ToolRequest::new(Tool::Formatter, workspace_root);

    request.arg("--format").arg(output_format.as_str());

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

fn result_from_operation(
    operation: Result<(), DiagnosticBag>,
    output_format: OutputFormat,
) -> TackRunResult {
    match operation {
        Ok(()) => TackRunResult::new(ExitCode::SUCCESS, DiagnosticBag::new(), output_format),
        Err(diagnostics) => failure(diagnostics, output_format),
    }
}

fn result_from_outputs(outputs: Vec<ToolOutput>, output_format: OutputFormat) -> TackRunResult {
    if output_format == OutputFormat::Json && outputs.len() > 1 {
        return aggregate_json_outputs(outputs, output_format);
    }

    let success = outputs.iter().all(ToolOutput::success);
    let mut stdout = String::new();
    let mut stderr = String::new();

    for output in outputs {
        let (_, child_stdout, child_stderr) = output.into_parts();

        stdout.push_str(&child_stdout);
        stderr.push_str(&child_stderr);
    }

    TackRunResult::with_output(
        exit_code(success),
        DiagnosticBag::new(),
        output_format,
        stdout,
        stderr,
    )
}

fn result_from_output(output: ToolOutput, output_format: OutputFormat) -> TackRunResult {
    let (success, stdout, stderr) = output.into_parts();

    TackRunResult::with_output(
        exit_code(success),
        DiagnosticBag::new(),
        output_format,
        stdout,
        stderr,
    )
}

fn aggregate_json_outputs(outputs: Vec<ToolOutput>, output_format: OutputFormat) -> TackRunResult {
    let success = outputs.iter().all(ToolOutput::success);
    let mut diagnostics = Vec::new();
    let mut stderr = String::new();

    for output in outputs {
        let (_, stdout, child_stderr) = output.into_parts();

        stderr.push_str(&child_stderr);

        let Ok(mut report) = serde_json::from_str::<serde_json::Value>(&stdout) else {
            return failure(operation_diagnostics("compiler_json_output"), output_format);
        };

        let Some(entries) = report
            .get_mut("diagnostics")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return failure(operation_diagnostics("compiler_json_output"), output_format);
        };

        diagnostics.append(entries);
    }

    let stdout = match serde_json::to_string_pretty(&serde_json::json!({
        "has_errors": !success,
        "diagnostics": diagnostics,
    })) {
        Ok(stdout) => format!("{stdout}\n"),
        Err(_) => return failure(operation_diagnostics("compiler_json_output"), output_format),
    };

    TackRunResult::with_output(
        exit_code(success),
        DiagnosticBag::new(),
        output_format,
        stdout,
        stderr,
    )
}

fn failure(diagnostics: DiagnosticBag, output_format: OutputFormat) -> TackRunResult {
    TackRunResult::new(ExitCode::FAILURE, diagnostics, output_format)
}

const fn exit_code(success: bool) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
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

    fn has_argument_pair(arguments: &[OsString], name: &str, value: &str) -> bool {
        arguments
            .windows(2)
            .any(|pair| pair[0] == name && pair[1] == value)
    }
}
