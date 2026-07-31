#![forbid(unsafe_code)]

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use bray_compilation::WorkerBudget;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_lsp::LanguageServer;
use bray_project::load_project_graph;
use bray_tooling::{OutputFormat, clap_styles, write_diagnostics};
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let worker_budget = match cli.cpu_count {
        Some(cpu_count) => match WorkerBudget::new(cpu_count) {
            Ok(worker_budget) => worker_budget,
            Err(error) => return write_failure(error.into_diagnostic_bag()),
        },
        None => WorkerBudget::default(),
    };

    let graph = match load_project_graph(&cli.workspace) {
        Ok(graph) => graph,
        Err(error) => {
            return write_failure(DiagnosticBag::single(
                error.into_diagnostic(DiagnosticId::new(0)),
            ));
        }
    };

    let target = graph
        .targets()
        .iter()
        .find(|target| target.name() == cli.target || target.identity().as_str() == cli.target)
        .map(|target| target.identity().clone());

    let Some(target) = target else {
        return write_failure(DiagnosticBag::single(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::ProjectCommandSelectionInvalid,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::referenced_name(cli.target)),
        ));
    };

    let server = LanguageServer::new(cli.workspace, Arc::new(graph), target, worker_budget);

    match server.run(Box::new(io::stdin()), &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn write_failure(diagnostics: DiagnosticBag) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();

    let _ = write_diagnostics(
        &diagnostics,
        None,
        OutputFormat::Text,
        &mut stdout,
        &mut stderr,
    );

    ExitCode::FAILURE
}

#[derive(Debug, Parser)]
#[command(
    name = "bray-lsp",
    version = env!("CARGO_PKG_VERSION"),
    about = "The Bray language server",
    styles = clap_styles()
)]
struct Cli {
    #[arg(long, value_name = "DIRECTORY", default_value = ".")]
    workspace: PathBuf,
    #[arg(long, value_name = "NAME")]
    target: String,
    #[arg(long = "cpu-count", value_name = "N")]
    cpu_count: Option<usize>,
}
