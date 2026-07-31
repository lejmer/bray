use std::io::{Read, Write};
use std::process::ExitCode;

use bray_lsp::LanguageServer;

use crate::{
    TackLanguageServerRequest, TackLanguageServerService, TackServiceResult,
};

pub(crate) struct BrayLanguageServerService;

impl TackLanguageServerService for BrayLanguageServerService {
    fn run(
        &self,
        request: TackLanguageServerRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> TackServiceResult {
        let (workspace_root, graph, target, worker_budget) = request.into_parts();

        let server = LanguageServer::new(workspace_root, graph, target, worker_budget);

        let exit_code = match server.run(input, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };

        TackServiceResult::new(
            exit_code,
            bray_diagnostics::DiagnosticBag::new(),
            String::new(),
            String::new(),
        )
    }
}
