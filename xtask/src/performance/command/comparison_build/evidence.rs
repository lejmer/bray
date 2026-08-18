use std::collections::BTreeSet;
use std::path::Path;

use bray_target::NativeTarget;

use crate::performance::compilation::{CompilationReuseRole, reuse_evidence};
use crate::performance::model::{ArtifactReport, CompilationReuseEvidence, RetainedInput};

pub(super) fn bray_reuse_evidence(
    artifact: &ArtifactReport,
    target: NativeTarget,
    standard_library: &Path,
    runtime: &Path,
) -> Result<CompilationReuseEvidence, String> {
    let selected = bray_compilation::SelectedTarget::for_native(target);

    let standard_library = bray_standard_library::StandardLibraryRoot::try_new(standard_library)
        .ok_or_else(|| "matched standard-library root is invalid".to_owned())?;

    let resolver = bray_standard_library::StandardLibraryResolver::new(standard_library);

    let packaged_library_names = resolver
        .target_artifacts(selected.profile().identity(), selected.runtime_abi())
        .map_err(|error| {
            format!("could not resolve matched standard-library artifacts: {error:?}")
        })?
        .iter()
        .filter_map(|artifact| artifact.path().file_name())
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    let runtime_names = bray_tooling::load_runtime_artifact(
        runtime,
        selected.profile().identity(),
        selected.runtime_abi(),
    )
    .map_err(|error| format!("could not resolve matched runtime artifacts: {error:?}"))?
    .components()
    .iter()
    .filter_map(|component| component.archive().file_name())
    .map(|name| name.to_string_lossy().to_ascii_lowercase())
    .collect::<BTreeSet<_>>();

    Ok(reuse_evidence(std::slice::from_ref(artifact), |input| {
        let name = retained_artifact_name(input);

        if packaged_library_names.contains(&name) {
            Some(CompilationReuseRole::PackagedLibrary)
        } else if runtime_names.contains(&name) {
            Some(CompilationReuseRole::Runtime)
        } else {
            None
        }
    }))
}

pub(super) fn rust_reuse_role(input: &RetainedInput) -> Option<CompilationReuseRole> {
    let name = retained_artifact_name(input);

    if name.starts_with("libstd-") || name.starts_with("std-") {
        Some(CompilationReuseRole::PackagedLibrary)
    } else {
        Some(CompilationReuseRole::Runtime)
    }
}

pub(super) fn cpp_reuse_role(input: &RetainedInput) -> Option<CompilationReuseRole> {
    let name = retained_artifact_name(input);

    let packaged = ["libc++", "libstdc++", "libcpmt", "libcmt", "msvcprt"]
        .iter()
        .any(|identity| name.contains(identity));

    Some(if packaged {
        CompilationReuseRole::PackagedLibrary
    } else {
        CompilationReuseRole::Runtime
    })
}

fn retained_artifact_name(input: &RetainedInput) -> String {
    input
        .artifact
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&input.artifact)
        .to_ascii_lowercase()
}
