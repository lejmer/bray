use std::io::ErrorKind;
use std::path::PathBuf;

use bray_diagnostics::{
    DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticId, DiagnosticKind,
};

use super::ProjectLoadError;

#[test]
fn project_load_errors_preserve_their_diagnostic_categories() {
    let manifest = PathBuf::from("bray-workspace.json");

    let read = diagnostic_bag(ProjectLoadError::ReadManifest {
        path: manifest.clone(),
        kind: ErrorKind::NotFound,
    });

    bray_testing::assert_goal_state_diagnostic_kind(
        &read,
        DiagnosticKind::ProjectManifestReadFailed,
    );

    let parse = diagnostic_bag(ProjectLoadError::ParseManifest {
        path: manifest.clone(),
        kind: DiagnosticDocumentParseKind::Syntax,
        line: Some(1),
        column: Some(2),
    });

    bray_testing::assert_goal_state_diagnostic_kind(
        &parse,
        DiagnosticKind::ProjectManifestParseFailed,
    );
}

fn diagnostic_bag(error: ProjectLoadError) -> DiagnosticBag {
    DiagnosticBag::single(error.into_diagnostic(DiagnosticId::new(0)))
}
