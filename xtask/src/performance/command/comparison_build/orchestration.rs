use std::path::Path;

use bray_target::NativeTarget;

use crate::performance::compilation::{
    MATCHED_APPLICATION_CONTRACT, MATCHED_LIBRARY_CONTRACT, comparison,
};
use crate::performance::model::{CompilationKind, CompilationLanguage};

pub(in crate::performance::command) fn build(
    kind: CompilationKind,
    root: &Path,
    compiler: &Path,
    output: &Path,
    target: NativeTarget,
    toolchain: &Path,
    runtime: &Path,
) -> Result<crate::performance::model::CompilationComparisonReport, String> {
    let (directory, contract) = match kind {
        CompilationKind::Application => ("application-compilation", MATCHED_APPLICATION_CONTRACT),
        CompilationKind::Library => ("library-compilation", MATCHED_LIBRARY_CONTRACT),
    };

    let output = output.join(directory);

    std::fs::create_dir_all(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;

    let builds = [
        (
            CompilationLanguage::Bray,
            super::bray::build(compiler, &output, kind, target, toolchain, runtime)?,
        ),
        (
            CompilationLanguage::Rust,
            super::rust::build(root, &output, kind, target)?,
        ),
        (
            CompilationLanguage::Cpp,
            super::cpp::build(root, &output, kind, target)?,
        ),
    ]
    .into_iter()
    .collect();

    Ok(comparison(kind, contract, builds))
}
