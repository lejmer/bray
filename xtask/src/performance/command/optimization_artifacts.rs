use std::path::Path;

use bray_standard_library::{StandardLibraryArtifactKind, decode_standard_library_manifest};

use super::super::model::{OptimizationArtifactReport, WorkloadReport};

pub(super) fn inspect(
    root: &Path,
    target: bray_target::NativeTarget,
    workloads: &[WorkloadReport],
) -> Result<Vec<OptimizationArtifactReport>, String> {
    let manifest_path = root.join(bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let bytes = std::fs::read(&manifest_path)
        .map_err(|error| format!("could not read {}: {error}", manifest_path.display()))?;

    let manifest = decode_standard_library_manifest(&bytes)
        .map_err(|error| format!("performance standard library is invalid: {error:?}"))?;

    let selected = manifest
        .targets()
        .iter()
        .find(|candidate| candidate.target().as_str() == target.as_str())
        .ok_or_else(|| "performance standard library does not contain the target".to_owned())?;

    selected
        .artifacts()
        .iter()
        .filter(|artifact| artifact.kind() == StandardLibraryArtifactKind::OptimizationArchive)
        .map(|artifact| {
            let optimization = artifact.optimization().ok_or_else(|| {
                "validated optimization archive has no selection metadata".to_owned()
            })?;

            let fallback = optimization.fallback().path();

            let mut selected_by_workloads = workloads
                .iter()
                .filter(|workload| {
                    workload_selects_optimization(
                        workload,
                        optimization.partition(),
                        fallback,
                    )
                })
                .map(|workload| workload.id.clone())
                .collect::<Vec<_>>();

            selected_by_workloads.sort();

            Ok(OptimizationArtifactReport {
                partition: optimization.partition().to_owned(),
                path: artifact.path().to_owned(),
                bytes: artifact.byte_len(),
                fallback: fallback.to_owned(),
                selected_by_workloads,
            })
        })
        .collect()
}

fn workload_selects_optimization(
    workload: &WorkloadReport,
    partition: &str,
    fallback: &str,
) -> bool {
    workload.artifacts.iter().any(|artifact| {
        artifact
            .dependencies
            .static_archives
            .entries
            .iter()
            .any(|archive| archive_matches_fallback(archive, fallback))
            || artifact
                .dependencies
                .static_inputs
                .entries
                .iter()
                .any(|input| {
                    input.artifact.contains(partition)
                        || input
                            .member
                            .as_deref()
                            .is_some_and(|member| member.contains(partition))
                })
    })
}

fn archive_matches_fallback(archive: &str, fallback: &str) -> bool {
    let archive = Path::new(archive).file_name();
    let fallback = Path::new(fallback);

    archive == fallback.file_name() || archive == fallback.file_stem()
}
