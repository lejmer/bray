use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use bray_diagnostics::DiagnosticBag;
use bray_tooling::{
    OutputFormat, exit_code_from_diagnostics,
    write_diagnostic_groups,
};

use crate::tack::compiler::ProjectCompiler;
use crate::tack::error::{
    operation_diagnostics, selection_diagnostics, unavailable_diagnostics,
};
use crate::tack::inspection::{
    render_compiler_inspection, render_project_inspection,
};
use crate::tack::install::{
    install_git_repository, run_project_process,
};
use crate::tack::model::{
    TackCommand, TackInspection, TackInvocation,
};
use crate::tack::project::{
    ProductSelectionKind, load_graph, root_source_files, select_products,
    select_target,
};
use crate::tack::result::TackRunResult;
use crate::tack::service::{
    TackFormatInput, TackFormatMode, TackFormatRequest,
    TackLanguageServerRequest, TackServices,
};

/// Runs Bray Tack with no optional formatter or language-server implementation.
pub fn run_tack(arguments: impl IntoIterator<Item = OsString>) -> ExitCode {
    run_tack_with_services(arguments, TackServices::new())
}

/// Runs Bray Tack with explicitly linked optional tool services.
pub fn run_tack_with_services(
    arguments: impl IntoIterator<Item = OsString>,
    services: TackServices<'_>,
) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    run_tack_with_io(
        arguments,
        services,
        io::stdin(),
        &mut stdout,
        &mut stderr,
    )
}

/// Runs Bray Tack and returns its structured outcome.
pub fn run_tack_result(
    arguments: impl IntoIterator<Item = OsString>,
) -> TackRunResult {
    run_tack_result_with_input(arguments, TackServices::new(), io::empty())
}

fn run_tack_with_io(
    arguments: impl IntoIterator<Item = OsString>,
    services: TackServices<'_>,
    stdin: impl Read + Send + 'static,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    let result = run_tack_result_with_input_and_output(
        arguments,
        services,
        Box::new(stdin),
        stdout,
    );

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
    services: TackServices<'_>,
    stdin: impl Read + Send + 'static,
) -> TackRunResult {
    let mut protocol_output = Vec::new();

    let mut result = run_tack_result_with_input_and_output(
        arguments,
        services,
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
    services: TackServices<'_>,
    stdin: Box<dyn Read + Send>,
    protocol_output: &mut impl Write,
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

    execute_invocation(invocation, services, stdin, protocol_output)
}

fn execute_invocation(
    invocation: TackInvocation,
    services: TackServices<'_>,
    mut stdin: Box<dyn Read + Send>,
    protocol_output: &mut impl Write,
) -> TackRunResult {
    let (workspace_root, worker_budget, output_format, command) =
        invocation.into_parts();

    let workspace_root = match std::path::absolute(workspace_root) {
        Ok(workspace_root) => workspace_root,
        Err(_) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                operation_diagnostics("workspace_path"),
                output_format,
            );
        }
    };

    match command {
        TackCommand::VendorInstall { name, repository } => {
            return result_from_operation(
                install_git_repository(&workspace_root, &name, &repository),
                output_format,
            );
        }
        TackCommand::Format { check, files } => {
            return run_format(
                workspace_root,
                check,
                files,
                output_format,
                services,
                stdin.as_mut(),
            );
        }
        _ => {}
    }

    let graph = match load_graph(&workspace_root) {
        Ok(graph) => graph,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    match command {
        TackCommand::Check(selection) => run_check(
            &workspace_root,
            &graph,
            worker_budget,
            &selection,
            output_format,
        ),
        TackCommand::Build(selection) => run_build(
            &workspace_root,
            &graph,
            worker_budget,
            &selection,
            output_format,
        ),
        TackCommand::Run {
            selection,
            arguments,
        } => run_one(
            &workspace_root,
            &graph,
            worker_budget,
            &selection,
            arguments,
            output_format,
        ),
        TackCommand::Test {
            selection,
            arguments,
        } => run_tests(
            &workspace_root,
            &graph,
            worker_budget,
            &selection,
            arguments,
            output_format,
        ),
        TackCommand::Inspect {
            selection,
            inspection,
            source_id,
            position,
        } => run_inspect(
            &workspace_root,
            &graph,
            worker_budget,
            &selection,
            inspection,
            source_id,
            position,
            output_format,
        ),
        TackCommand::LanguageServer { target } => {
            let Some(service) = services.language_server() else {
                return TackRunResult::new(
                    ExitCode::FAILURE,
                    unavailable_diagnostics("language_server"),
                    output_format,
                );
            };

            let target = match select_target(&graph, target.as_deref()) {
                Ok(Some(target)) => target.identity().clone(),
                Ok(None) => match graph.targets().first() {
                    Some(target) => target.identity().clone(),
                    None => {
                        return TackRunResult::new(
                            ExitCode::FAILURE,
                            selection_diagnostics("target"),
                            output_format,
                        );
                    }
                },
                Err(diagnostics) => {
                    return TackRunResult::new(
                        ExitCode::FAILURE,
                        diagnostics,
                        output_format,
                    );
                }
            };

            let request = TackLanguageServerRequest::new(
                workspace_root,
                Arc::new(graph),
                target,
                worker_budget,
            );

            let (exit_code, diagnostics, stdout, stderr) =
                service.run(request, stdin, protocol_output).into_parts();

            TackRunResult::with_output(
                exit_code,
                diagnostics,
                output_format,
                stdout,
                stderr,
            )
        }
        TackCommand::Format { .. } | TackCommand::VendorInstall { .. } => {
            TackRunResult::new(
                ExitCode::FAILURE,
                operation_diagnostics("command_routing"),
                output_format,
            )
        }
    }
}

fn run_check(
    workspace_root: &Path,
    graph: &bray_project::ProjectGraph,
    worker_budget: bray_compilation::WorkerBudget,
    selection: &crate::tack::model::TackSelection,
    output_format: OutputFormat,
) -> TackRunResult {
    let products = match select_products(
        graph,
        selection,
        ProductSelectionKind::Any,
        false,
    ) {
        Ok(products) => products,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let mut compiler = ProjectCompiler::new(workspace_root, graph, worker_budget);

    let mut result = TackRunResult::new(
        ExitCode::SUCCESS,
        DiagnosticBag::new(),
        output_format,
    );

    for product in products {
        match compiler.check(&product) {
            Ok(compilation) => {
                let diagnostics = compilation.check_diagnostics().clone();

                result.add_compilation_diagnostics(compilation, diagnostics);
            }
            Err(error) => result.add_unscoped_diagnostics(error),
        }
    }

    result.select_diagnostic_exit_code();

    result
}

fn run_build(
    workspace_root: &Path,
    graph: &bray_project::ProjectGraph,
    worker_budget: bray_compilation::WorkerBudget,
    selection: &crate::tack::model::TackSelection,
    output_format: OutputFormat,
) -> TackRunResult {
    let products = match select_products(
        graph,
        selection,
        ProductSelectionKind::Any,
        false,
    ) {
        Ok(products) => products,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let mut compiler = ProjectCompiler::new(workspace_root, graph, worker_budget);
    let mut diagnostics = DiagnosticBag::new();

    for product in products {
        match compiler.build(&product) {
            Ok(outcome) => {
                diagnostics = diagnostics.merged(outcome.diagnostics());
            }
            Err(error) => diagnostics = diagnostics.merged(&error),
        }
    }

    TackRunResult::new(
        exit_code_from_diagnostics(&diagnostics),
        diagnostics,
        output_format,
    )
}

fn run_one(
    workspace_root: &Path,
    graph: &bray_project::ProjectGraph,
    worker_budget: bray_compilation::WorkerBudget,
    selection: &crate::tack::model::TackSelection,
    arguments: Vec<OsString>,
    output_format: OutputFormat,
) -> TackRunResult {
    let products = match select_products(
        graph,
        selection,
        ProductSelectionKind::Executable,
        true,
    ) {
        Ok(products) => products,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let Some(product) = products.into_iter().next() else {
        return TackRunResult::new(
            ExitCode::FAILURE,
            selection_diagnostics("executable"),
            output_format,
        );
    };

    let mut compiler = ProjectCompiler::new(workspace_root, graph, worker_budget);

    let outcome = match compiler.build(&product) {
        Ok(outcome) => outcome,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let Some(executable) = outcome.executable() else {
        return TackRunResult::new(
            ExitCode::FAILURE,
            selection_diagnostics("executable_output"),
            output_format,
        );
    };

    let exit_code = match run_project_process(
        executable,
        &arguments,
        workspace_root,
    ) {
        Ok(exit_code) => exit_code,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    TackRunResult::new(
        exit_code,
        outcome.diagnostics().clone(),
        output_format,
    )
}

fn run_tests(
    workspace_root: &Path,
    graph: &bray_project::ProjectGraph,
    worker_budget: bray_compilation::WorkerBudget,
    selection: &crate::tack::model::TackSelection,
    arguments: Vec<OsString>,
    output_format: OutputFormat,
) -> TackRunResult {
    let products = match select_products(
        graph,
        selection,
        ProductSelectionKind::Test,
        false,
    ) {
        Ok(products) => products,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let mut compiler = ProjectCompiler::new(workspace_root, graph, worker_budget);
    let mut diagnostics = DiagnosticBag::new();
    let mut exit_code = ExitCode::SUCCESS;

    for product in products {
        let outcome = match compiler.build(&product) {
            Ok(outcome) => outcome,
            Err(error) => {
                diagnostics = diagnostics.merged(&error);
                exit_code = ExitCode::FAILURE;

                continue;
            }
        };

        diagnostics = diagnostics.merged(outcome.diagnostics());

        let Some(executable) = outcome.executable() else {
            diagnostics = diagnostics.merged(&selection_diagnostics(
                "test_executable_output",
            ));

            exit_code = ExitCode::FAILURE;

            continue;
        };

        match run_project_process(executable, &arguments, workspace_root) {
            Ok(code) if code == ExitCode::SUCCESS => {}
            Ok(_) => exit_code = ExitCode::FAILURE,
            Err(error) => {
                diagnostics = diagnostics.merged(&error);
                exit_code = ExitCode::FAILURE;
            }
        }
    }

    TackRunResult::new(exit_code, diagnostics, output_format)
}

#[expect(
    clippy::too_many_arguments,
    reason = "inspection routing keeps each independently selected CLI value explicit"
)]
fn run_inspect(
    workspace_root: &Path,
    graph: &bray_project::ProjectGraph,
    worker_budget: bray_compilation::WorkerBudget,
    selection: &crate::tack::model::TackSelection,
    inspection: TackInspection,
    source_id: u32,
    position: Option<bray_source::TextSize>,
    output_format: OutputFormat,
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
            Err(diagnostics) => TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            ),
        };
    }

    let products = match select_products(
        graph,
        selection,
        ProductSelectionKind::Any,
        true,
    ) {
        Ok(products) => products,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let Some(product) = products.into_iter().next() else {
        return TackRunResult::new(
            ExitCode::FAILURE,
            selection_diagnostics("inspection_product"),
            output_format,
        );
    };

    let mut compiler = ProjectCompiler::new(workspace_root, graph, worker_budget);

    let compilation = match compiler.compilation_for_inspection(&product) {
        Ok(compilation) => compilation,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    match render_compiler_inspection(
        &compilation,
        inspection,
        source_id,
        position,
        output_format,
    ) {
        Ok((stdout, diagnostics)) => TackRunResult::with_output(
            exit_code_from_diagnostics(&diagnostics),
            diagnostics,
            output_format,
            stdout,
            String::new(),
        ),
        Err(diagnostics) => TackRunResult::new(
            ExitCode::FAILURE,
            diagnostics,
            output_format,
        ),
    }
}

fn run_format(
    workspace_root: PathBuf,
    check: bool,
    files: Vec<PathBuf>,
    output_format: OutputFormat,
    services: TackServices<'_>,
    stdin: &mut dyn Read,
) -> TackRunResult {
    let Some(formatter) = services.formatter() else {
        return TackRunResult::new(
            ExitCode::FAILURE,
            unavailable_diagnostics("formatter"),
            output_format,
        );
    };

    let input = match format_input(&workspace_root, files, stdin) {
        Ok(input) => input,
        Err(diagnostics) => {
            return TackRunResult::new(
                ExitCode::FAILURE,
                diagnostics,
                output_format,
            );
        }
    };

    let mode = if check {
        TackFormatMode::Check
    } else {
        TackFormatMode::Write
    };

    let request = TackFormatRequest::new(workspace_root, mode, input);

    let (exit_code, diagnostics, stdout, stderr) =
        formatter.format(request).into_parts();

    TackRunResult::with_output(
        exit_code,
        diagnostics,
        output_format,
        stdout,
        stderr,
    )
}

fn format_input(
    workspace_root: &Path,
    files: Vec<PathBuf>,
    stdin: &mut dyn Read,
) -> Result<TackFormatInput, DiagnosticBag> {
    let standard_input = files.iter().any(|path| path == Path::new("-"));

    if standard_input {
        if files.len() != 1 {
            return Err(selection_diagnostics("format_standard_input"));
        }

        let mut bytes = Vec::new();

        stdin
            .read_to_end(&mut bytes)
            .map_err(|_| operation_diagnostics("read_standard_input"))?;

        return Ok(TackFormatInput::StandardInput(bytes));
    }

    if !files.is_empty() {
        return Ok(TackFormatInput::Files(
            files
                .into_iter()
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        workspace_root.join(path)
                    }
                })
                .collect(),
        ));
    }

    let graph = load_graph(workspace_root)?;

    Ok(TackFormatInput::Files(root_source_files(
        &graph,
        workspace_root,
    )))
}

fn result_from_operation(
    result: Result<(), DiagnosticBag>,
    output_format: OutputFormat,
) -> TackRunResult {
    match result {
        Ok(()) => TackRunResult::new(
            ExitCode::SUCCESS,
            DiagnosticBag::new(),
            output_format,
        ),
        Err(diagnostics) => TackRunResult::new(
            ExitCode::FAILURE,
            diagnostics,
            output_format,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Write};
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::sync::Mutex;

    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};

    use super::{
        run_tack_result_with_input, run_tack_with_io,
    };
    use crate::tack::{
        TackFormatInput, TackFormatMode, TackFormatRequest,
        TackFormatService, TackLanguageServerRequest,
        TackLanguageServerService, TackServiceResult, TackServices,
    };
    use crate::test_support::{
        ProjectWorkspace, unique_temporary_directory,
    };

    #[derive(Default)]
    struct RecordingFormatter {
        request: Mutex<Option<TackFormatRequest>>,
    }

    impl TackFormatService for RecordingFormatter {
        fn format(&self, request: TackFormatRequest) -> TackServiceResult {
            *self
                .request
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(request);

            TackServiceResult::new(
                ExitCode::SUCCESS,
                DiagnosticBag::new(),
                String::from("formatted\n"),
                String::new(),
            )
        }
    }

    #[derive(Default)]
    struct RecordingLanguageServer {
        package_count: Mutex<Option<usize>>,
        target: Mutex<Option<String>>,
    }

    impl TackLanguageServerService for RecordingLanguageServer {
        fn run(
            &self,
            request: TackLanguageServerRequest,
            _input: Box<dyn Read + Send>,
            output: &mut dyn Write,
        ) -> TackServiceResult {
            *self
                .package_count
                .lock()
                .unwrap_or_else(|error| error.into_inner()) =
                Some(request.graph().packages().len());

            *self
                .target
                .lock()
                .unwrap_or_else(|error| error.into_inner()) =
                Some(request.target().as_str().to_owned());

            output
                .write_all(b"language-server-protocol")
                .unwrap_or_else(|error| panic!("test protocol output should write: {error}"));

            TackServiceResult::new(
                ExitCode::SUCCESS,
                DiagnosticBag::new(),
                String::new(),
                String::new(),
            )
        }
    }

    fn run_check_output(
        workspace: &ProjectWorkspace,
        output_format: &str,
    ) -> (ExitCode, Vec<u8>, Vec<u8>) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_tack_with_io(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "--format".into(),
                output_format.into(),
                "check".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
            &mut stdout,
            &mut stderr,
        );

        (exit_code, stdout, stderr)
    }

    fn write_two_invalid_products(workspace: &ProjectWorkspace) {
        workspace.write(
            "app/bray-package.json",
            r#"{
                "format": 1,
                "identity": "example.application",
                "features": [],
                "source_roots": [
                    {
                        "name": "first",
                        "path": "first"
                    },
                    {
                        "name": "second",
                        "path": "second"
                    }
                ],
                "dependencies": [],
                "products": [
                    {
                        "name": "first",
                        "kind": "library",
                        "source_roots": ["first"],
                        "targets": ["native"],
                        "outputs": ["package_interface"]
                    },
                    {
                        "name": "second",
                        "kind": "library",
                        "source_roots": ["second"],
                        "targets": ["native"],
                        "outputs": ["package_interface"]
                    }
                ]
            }"#,
        );

        workspace.write("app/first/first.bray", "$");
        workspace.write("app/second/second.bray", "$");
    }

    fn write_backend_only_product(workspace: &ProjectWorkspace, product_kind: &str) {
        workspace.write(
            "app/bray-package.json",
            &format!(
                r#"{{
                    "format": 1,
                    "identity": "example.application",
                    "features": [],
                    "source_roots": [
                        {{
                            "name": "main",
                            "path": "src"
                        }}
                    ],
                    "dependencies": [],
                    "products": [
                        {{
                            "name": "application",
                            "kind": "{product_kind}",
                            "source_roots": ["main"],
                            "targets": ["native"],
                            "outputs": ["backend_ir"]
                        }}
                    ]
                }}"#,
            ),
        );

        if product_kind == "library" {
            workspace.write(
                "app/src/main.bray",
                "module app;\n\nfunc helper()\n{\n}\n",
            );
        }
    }

    fn output_directory_contains_extension(
        workspace: &ProjectWorkspace,
        extension: &str,
    ) -> bool {
        let directory = workspace
            .path()
            .join("build/native/example.application/application");

        std::fs::read_dir(directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().is_some_and(|value| value == extension))
    }

    #[test]
    fn check_loads_the_manifest_and_returns_compiler_exit_status() {
        let workspace = ProjectWorkspace::basic();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "check".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn check_renders_one_compilation_source_in_text_and_json() {
        let workspace = ProjectWorkspace::basic();

        workspace.write("app/src/main.bray", "$");

        let (text_code, text_stdout, text_stderr) =
            run_check_output(&workspace, "text");

        assert_eq!(text_code, ExitCode::FAILURE);
        assert!(text_stdout.is_empty());

        let text = String::from_utf8(text_stderr)
            .unwrap_or_else(|error| panic!("text diagnostics should be UTF-8: {error:?}"));

        assert!(text.contains("main.bray"));
        assert!(text.contains("$"));

        let (json_code, json_stdout, json_stderr) =
            run_check_output(&workspace, "json");

        assert_eq!(json_code, ExitCode::FAILURE);
        assert!(json_stderr.is_empty());

        let output: serde_json::Value = serde_json::from_slice(&json_stdout)
            .unwrap_or_else(|error| panic!("diagnostics should be JSON: {error:?}"));

        let lexical = output["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|diagnostic| {
                diagnostic["kind"] == DiagnosticKind::LexicalInvalidCharacter.as_str()
            })
            .unwrap_or_else(|| panic!("lexical diagnostic should be present"));

        let file_path = lexical["primary_span"]["source_origin"]["file_path"]
            .as_str()
            .unwrap_or_else(|| panic!("diagnostic should retain its source file"));

        assert!(file_path.ends_with("main.bray"));
    }

    #[test]
    fn check_keeps_matching_diagnostics_from_distinct_compilations() {
        let workspace = ProjectWorkspace::basic();

        write_two_invalid_products(&workspace);

        let (text_code, text_stdout, text_stderr) =
            run_check_output(&workspace, "text");

        assert_eq!(text_code, ExitCode::FAILURE);
        assert!(text_stdout.is_empty());

        let text = String::from_utf8(text_stderr)
            .unwrap_or_else(|error| panic!("text diagnostics should be UTF-8: {error:?}"));

        assert!(text.contains("first.bray"));
        assert!(text.contains("second.bray"));

        let (json_code, json_stdout, json_stderr) =
            run_check_output(&workspace, "json");

        assert_eq!(json_code, ExitCode::FAILURE);
        assert!(json_stderr.is_empty());

        let output: serde_json::Value = serde_json::from_slice(&json_stdout)
            .unwrap_or_else(|error| panic!("diagnostics should be JSON: {error:?}"));

        let diagnostics = output["diagnostics"]
            .as_array()
            .unwrap_or_else(|| panic!("JSON diagnostics should be an array"));

        let lexical: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic["kind"] == DiagnosticKind::LexicalInvalidCharacter.as_str()
            })
            .collect();

        assert_eq!(lexical.len(), 2);

        assert!(
            lexical
                .iter()
                .any(|diagnostic| diagnostic["primary_span"]["source_origin"]["file_path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("first.bray")))
        );

        assert!(
            lexical
                .iter()
                .any(|diagnostic| diagnostic["primary_span"]["source_origin"]["file_path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("second.bray")))
        );
    }

    #[test]
    fn check_never_acquires_a_missing_dependency() {
        let workspace = ProjectWorkspace::with_missing_vendor();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "check".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::ProjectDependencyPackageUnknown)
                .count(),
            1
        );

        assert!(!workspace.path().join("vendor").exists());
    }

    #[test]
    fn build_never_acquires_a_missing_dependency() {
        let workspace = ProjectWorkspace::with_missing_vendor();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "build".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::ProjectDependencyPackageUnknown)
                .count(),
            1
        );

        assert!(!workspace.path().join("vendor").exists());
        assert!(!workspace.path().join("build").exists());
    }

    #[test]
    fn check_consumes_the_explicit_vendored_graph_without_build_outputs() {
        let workspace = ProjectWorkspace::with_vendor();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "check".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert!(result.diagnostics().is_empty());
        assert!(!workspace.path().join("build").exists());
    }

    #[test]
    fn build_emits_the_selected_manifest_product() {
        let workspace = ProjectWorkspace::with_vendor();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "build".into(),
                "--package".into(),
                "example.math".into(),
                "--product".into(),
                "math".into(),
                "--target".into(),
                "native".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(
            result.exit_code(),
            ExitCode::SUCCESS,
            "{result:#?}"
        );

        assert!(
            workspace
                .path()
                .join("build")
                .join("native")
                .join("example.math")
                .join("math")
                .join("math.brayi")
                .is_file()
        );
    }

    #[test]
    fn build_emits_backend_only_library_outputs() {
        let workspace = ProjectWorkspace::basic();

        write_backend_only_product(&workspace, "library");

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "build".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

        assert!(output_directory_contains_extension(&workspace, "ll"));
    }

    #[test]
    fn build_emits_backend_only_executable_outputs() {
        let workspace = ProjectWorkspace::basic();

        write_backend_only_product(&workspace, "executable");

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "build".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS, "{result:#?}");

        assert!(output_directory_contains_extension(&workspace, "ll"));
    }

    #[test]
    fn formatter_receives_standard_input_without_file_discovery() {
        let formatter = RecordingFormatter::default();
        let workspace = unique_temporary_directory();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.as_os_str().to_os_string(),
                "fmt".into(),
                "--check".into(),
                "-".into(),
            ],
            TackServices::new().with_formatter(&formatter),
            Cursor::new(b"module app;\n".to_vec()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert_eq!(result.stdout(), "formatted\n");

        let request = formatter
            .request
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .unwrap_or_else(|| panic!("formatter should receive one request"));

        assert_eq!(request.mode(), TackFormatMode::Check);

        assert_eq!(
            request.input(),
            &TackFormatInput::StandardInput(b"module app;\n".to_vec())
        );
    }

    #[test]
    fn formatter_default_input_is_the_sorted_root_source_graph() {
        let formatter = RecordingFormatter::default();
        let workspace = ProjectWorkspace::basic();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "fmt".into(),
            ],
            TackServices::new().with_formatter(&formatter),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let request = formatter
            .request
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .unwrap_or_else(|| panic!("formatter should receive one request"));

        assert_eq!(
            request.input(),
            &TackFormatInput::Files(vec![
                workspace.path().join("app").join("src").join("main.bray")
            ])
        );
    }

    #[test]
    fn relative_workspace_is_normalized_before_child_paths_are_derived() {
        let formatter = RecordingFormatter::default();
        let unique = unique_temporary_directory();

        let name = unique
            .file_name()
            .unwrap_or_else(|| panic!("temporary path should have a file name"));

        let relative = PathBuf::from("target").join(name);

        let absolute = std::path::absolute(&relative)
            .unwrap_or_else(|error| panic!("relative workspace should become absolute: {error:?}"));

        let workspace = ProjectWorkspace::basic_at(absolute.clone());

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                relative.into_os_string(),
                "fmt".into(),
            ],
            TackServices::new().with_formatter(&formatter),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        let request = formatter
            .request
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .unwrap_or_else(|| panic!("formatter should receive one request"));

        assert_eq!(request.workspace_root(), absolute);

        assert_eq!(
            request.input(),
            &TackFormatInput::Files(vec![
                workspace.path().join("app/src/main.bray")
            ])
        );
    }

    #[test]
    fn inspection_and_help_publish_command_owned_output() {
        let workspace = ProjectWorkspace::basic();

        let project = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "inspect".into(),
                "project".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        let help = run_tack_result_with_input(
            ["bray".into(), "--help".into()],
            TackServices::new(),
            Cursor::new(Vec::new()),
        );

        assert_eq!(project.exit_code(), ExitCode::SUCCESS);
        assert!(project.stdout().contains("example.application"));
        assert_eq!(help.exit_code(), ExitCode::SUCCESS);
        assert!(help.stdout().contains("Bray Tack"));
        assert!(help.stderr().is_empty());
    }

    #[test]
    fn language_server_receives_the_validated_project_graph() {
        let workspace = ProjectWorkspace::with_vendor();
        let language_server = RecordingLanguageServer::default();

        let result = run_tack_result_with_input(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.path().as_os_str().to_os_string(),
                "language-server".into(),
                "--target".into(),
                "native".into(),
            ],
            TackServices::new()
                .with_language_server(&language_server),
            Cursor::new(Vec::new()),
        );

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);
        assert_eq!(result.stdout(), "language-server-protocol");

        assert_eq!(
            *language_server
                .package_count
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
            Some(2)
        );

        assert_eq!(
            language_server
                .target
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_deref(),
            Some("x86_64-unknown-linux-gnu")
        );
    }

    #[test]
    fn json_failures_are_written_to_stdout() {
        let workspace = unique_temporary_directory();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit_code = run_tack_with_io(
            [
                "bray".into(),
                "--workspace".into(),
                workspace.as_os_str().to_os_string(),
                "--format".into(),
                "json".into(),
                "check".into(),
            ],
            TackServices::new(),
            Cursor::new(Vec::new()),
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit_code, ExitCode::FAILURE);
        assert!(stderr.is_empty());

        let output: serde_json::Value = serde_json::from_slice(&stdout)
            .unwrap_or_else(|error| panic!("diagnostics should be JSON: {error:?}"));

        assert_eq!(
            output["diagnostics"][0]["kind"],
            DiagnosticKind::ProjectManifestReadFailed.as_str()
        );
    }
}
