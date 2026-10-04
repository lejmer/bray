use bray_target::{NativeTarget, ObjectFormat};

use super::super::corpus::WORKLOADS;
use super::super::model::{ArtifactKind, ArtifactReport, PerformanceReport, RuntimeLinkage};
use super::measurements::{measured_value, validate_identity};

#[cfg(test)]
pub(in crate::performance) fn validate(report: &PerformanceReport) -> Result<(), String> {
    super::measurements::validate_measurements(report)?;

    let failures = conformance_failures(report)?;

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

pub(in crate::performance) fn conformance_failures(
    report: &PerformanceReport,
) -> Result<Vec<String>, String> {
    let target = validate_identity(report)?;
    let mut failures = report.conformance_failures.clone();

    for workload in &report.workloads {
        for (owner, artifacts) in
            std::iter::once(("Bray".to_owned(), workload.artifacts.as_slice())).chain(
                workload
                    .peers
                    .iter()
                    .map(|(language, peer)| (format!("{language:?}"), peer.artifacts.as_slice())),
            )
        {
            for artifact in artifacts {
                if let Err(error) = validate_runtime_dependencies(artifact, target) {
                    failures.push(format!(
                        "workload {} {owner}: {error}. Actual dynamic libraries: {}",
                        workload.id,
                        artifact.dependencies.dynamic_libraries.entries.join(", ")
                    ));
                }
            }
        }

        if let Some(expected) = WORKLOADS
            .iter()
            .find(|canonical| canonical.id == workload.id)
            .and_then(|canonical| canonical.storage_expectation(target))
            && (measured_value(&workload.observations.allocation_count)
                != Some(expected.allocation_count)
                || measured_value(&workload.observations.allocated_bytes)
                    != Some(expected.allocated_bytes)
                || measured_value(&workload.observations.copied_bytes)
                    != Some(expected.copied_bytes))
        {
            failures.push(format!("workload {} storage work differs from the corpus contract: expected {expected:?}, measured allocations {:?}, allocated bytes {:?}, copied bytes {:?}", workload.id, measured_value(&workload.observations.allocation_count), measured_value(&workload.observations.allocated_bytes), measured_value(&workload.observations.copied_bytes)));
        }
    }

    failures.sort();
    failures.dedup();

    Ok(failures)
}

fn validate_runtime_dependencies(
    artifact: &ArtifactReport,
    target: NativeTarget,
) -> Result<(), String> {
    match super::super::peer::runtime_linkage(target)? {
        RuntimeLinkage::StaticApplicationRuntime => {
            validate_static_runtime_dependencies(artifact, target.object_format())
        }
        RuntimeLinkage::DynamicApplicationRuntime => validate_dynamic_windows_crt(artifact),
    }
}

fn validate_static_runtime_dependencies(
    artifact: &ArtifactReport,
    object_format: ObjectFormat,
) -> Result<(), String> {
    let has_dynamic_runtime =
        artifact
            .dependencies
            .dynamic_libraries
            .entries
            .iter()
            .any(|library| match object_format {
                ObjectFormat::Coff => crate::windows_crt::is_dynamic_library(library),
                ObjectFormat::Elf => {
                    let library = library.to_ascii_lowercase();

                    library.starts_with("libstdc++.") || library.starts_with("libgcc_s.")
                }
                ObjectFormat::MachO | ObjectFormat::WebAssembly | ObjectFormat::Xcoff => true,
            });

    if has_dynamic_runtime {
        return Err(format!(
            "{:?} artifact loads a dynamic application runtime",
            artifact.kind
        ));
    }

    Ok(())
}

fn validate_dynamic_windows_crt(artifact: &ArtifactReport) -> Result<(), String> {
    let static_runtime = artifact
        .dependencies
        .static_archives
        .entries
        .iter()
        .map(String::as_str)
        .chain(
            artifact
                .dependencies
                .static_inputs
                .entries
                .iter()
                .map(|input| input.artifact.as_str()),
        )
        .find(|library| crate::windows_crt::is_static_library(library));

    if let Some(library) = static_runtime {
        return Err(format!(
            "{} '{}' retains static Windows CRT input {library}",
            artifact_kind_name(artifact.kind),
            artifact.path,
        ));
    }

    if artifact.kind == ArtifactKind::Executable
        && !artifact
            .dependencies
            .dynamic_libraries
            .entries
            .iter()
            .any(|library| crate::windows_crt::is_dynamic_library(library))
    {
        return Err(format!(
            "executable '{}' does not import the dynamic Windows CRT",
            artifact.path,
        ));
    }

    Ok(())
}

const fn artifact_kind_name(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Executable => "executable",
        ArtifactKind::RelocatableObject => "relocatable object",
    }
}

#[cfg(test)]
mod tests {
    use super::{conformance_failures, validate_static_runtime_dependencies};
    use crate::performance::model::RuntimeLinkage;

    #[test]
    fn storage_conformance_preserves_actual_counts_and_target_specific_expectations() {
        let mut report = crate::performance::tests::report("corpus", 100, 1);
        let workload = &mut report.workloads[0];

        workload.id = "file_output".to_owned();

        let measured = |value| crate::performance::model::Observation::Measured {
            value,
            scope: crate::performance::model::STORAGE_OBSERVATION_SCOPE.to_owned(),
        };

        workload.observations.allocation_count = measured(5);
        workload.observations.allocated_bytes = measured(371);
        workload.observations.copied_bytes = measured(112);

        let failures = conformance_failures(&report).expect("Windows runtime identity");

        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("allocated_bytes: 486"));

        assert!(failures[0].contains(
            "measured allocations Some(5), allocated bytes Some(371), copied bytes Some(112)"
        ));

        report.workloads[0].observations.allocated_bytes = measured(486);
        report.workloads[0].observations.copied_bytes = measured(168);

        assert!(
            conformance_failures(&report)
                .expect("matching storage")
                .is_empty()
        );
    }

    #[test]
    fn linux_runtime_failure_collects_actual_dependencies_for_every_implementation() {
        let mut report = crate::performance::tests::report("corpus", 100, 1);

        report.identity.target = "x86_64-unknown-linux-gnu".to_owned();
        report.identity.runtime_linkage = RuntimeLinkage::StaticApplicationRuntime;

        let artifact = &mut report.workloads[0].artifacts[0];

        artifact.dependencies.dynamic_libraries.entries =
            vec!["libgcc_s.so.1".to_owned(), "libstdc++.so.6".to_owned()];

        assert!(
            validate_static_runtime_dependencies(artifact, bray_target::ObjectFormat::Elf)
                .expect_err("dynamic Linux runtime is rejected")
                .contains("loads a dynamic application runtime")
        );

        for peer in report.workloads[0].peers.values_mut() {
            peer.artifacts[0].dependencies.dynamic_libraries.entries =
                vec!["libgcc_s.so.1".to_owned()];
        }

        let failures = conformance_failures(&report).expect("declared identity");

        assert_eq!(failures.len(), 3);

        assert!(
            failures
                .iter()
                .all(|failure| failure.contains("libgcc_s.so.1"))
        );

        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("Bray") && failure.contains("libstdc++.so.6"))
        );

        assert!(failures.iter().any(|failure| failure.contains("Rust")));
        assert!(failures.iter().any(|failure| failure.contains("Cpp")));
    }
}
