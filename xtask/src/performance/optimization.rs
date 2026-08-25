use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use bray_standard_library::{StandardLibraryArtifactKind, decode_standard_library_manifest};

use super::model::{OptimizationArtifactReport, RetainedInput, WorkloadReport};

pub(super) struct OptimizationCatalog {
    entries: Vec<OptimizationEntry>,
}

impl OptimizationCatalog {
    pub(super) fn load(
        root: &Path,
        target: bray_target::NativeTarget,
    ) -> Result<Self, String> {
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

        let mut entries = selected
            .artifacts()
            .iter()
            .filter(|artifact| artifact.kind() == StandardLibraryArtifactKind::OptimizationArchive)
            .map(|artifact| {
                let optimization = artifact.optimization().ok_or_else(|| {
                    "validated optimization archive has no selection metadata".to_owned()
                })?;

                let symbols = optimization
                    .preservation_roots()
                    .iter()
                    .map(|symbol| symbol.as_str().to_owned())
                    .chain(optimization.platform_services().iter().map(|role| {
                        bray_runtime_interface::native_platform_service_role_symbol(*role)
                            .to_owned()
                    }))
                    .collect();

                Ok(OptimizationEntry {
                    partition: optimization.partition().to_owned(),
                    path: artifact.path().to_owned(),
                    bytes: artifact.byte_len(),
                    fallback: optimization.fallback().path().to_owned(),
                    symbols,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        entries.sort_by(|left, right| left.partition.cmp(&right.partition));

        Ok(Self { entries })
    }

    pub(super) fn retained_provenance(
        &self,
        map: &str,
        retained_inputs: &[RetainedInput],
    ) -> Vec<String> {
        let symbols = crate::link_map::symbol_tokens(map).collect::<HashSet<_>>();

        let mut provenance = retained_inputs
            .iter()
            .filter_map(retained_input_identity)
            .collect::<BTreeSet<_>>();

        for entry in &self.entries {
            let mut retained_partition = retained_inputs
                .iter()
                .any(|input| entry.matches_input(input));

            for symbol in entry
                .symbols
                .iter()
                .filter(|symbol| symbols.contains(symbol.as_str()))
            {
                retained_partition = true;

                if let Some(provider) = platform_provider_identity(symbol) {
                    provenance.insert(provider.to_owned());
                }
            }

            if retained_partition {
                provenance.insert(entry.partition.clone());
            }
        }

        provenance.into_iter().collect()
    }

    pub(super) fn reports(
        &self,
        workloads: &[WorkloadReport],
    ) -> Vec<OptimizationArtifactReport> {
        self.entries
            .iter()
            .map(|entry| {
                let mut selected_by_workloads = workloads
                    .iter()
                    .filter(|workload| workload_selects_partition(workload, &entry.partition))
                    .map(|workload| workload.id.clone())
                    .collect::<Vec<_>>();

                selected_by_workloads.sort_unstable();

                OptimizationArtifactReport {
                    partition: entry.partition.clone(),
                    path: entry.path.clone(),
                    bytes: entry.bytes,
                    fallback: entry.fallback.clone(),
                    selected_by_workloads,
                }
            })
            .collect()
    }
}

fn platform_provider_identity(symbol: &str) -> Option<&'static str> {
    let service = symbol.strip_prefix("bray_platform_")?;

    if ["context_", "clock_", "entropy_"]
        .iter()
        .any(|prefix| service.starts_with(prefix))
    {
        Some("bray_platform_core")
    } else if service.starts_with("standard_") {
        Some("bray_platform_standard_streams")
    } else if ["file_", "path_", "directory_"]
        .iter()
        .any(|prefix| service.starts_with(prefix))
    {
        Some("bray_platform_filesystem")
    } else if ["process_", "child_"]
        .iter()
        .any(|prefix| service.starts_with(prefix))
    {
        Some("bray_platform_process")
    } else if service.starts_with("time_") {
        Some("bray_platform_temporal")
    } else if service.starts_with("dynamic_library_") {
        Some("bray_platform_dynamic")
    } else {
        None
    }
}

struct OptimizationEntry {
    partition: String,
    path: String,
    bytes: u64,
    fallback: String,
    symbols: BTreeSet<String>,
}

impl OptimizationEntry {
    fn matches_input(&self, input: &RetainedInput) -> bool {
        [self.path.as_str(), self.fallback.as_str()]
            .into_iter()
            .any(|expected| input_matches_path(input, expected))
    }
}

fn input_matches_path(input: &RetainedInput, expected: &str) -> bool {
    let actual = Path::new(&input.artifact).file_name();
    let expected = Path::new(expected);

    actual == expected.file_name() || actual == expected.file_stem()
}

fn retained_input_identity(input: &RetainedInput) -> Option<String> {
    input.member.as_ref()?;

    let stem = Path::new(&input.artifact).file_stem()?.to_str()?;
    let identity = stem.strip_prefix("lib").unwrap_or(stem);

    (!identity.is_empty()).then(|| identity.to_owned())
}

fn workload_selects_partition(workload: &WorkloadReport, partition: &str) -> bool {
    workload.artifacts.iter().any(|artifact| {
        artifact.linker_map.as_ref().is_some_and(|map| {
            map.logical_provenance
                .entries
                .binary_search_by(|candidate| candidate.as_str().cmp(partition))
                .is_ok()
        })
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{OptimizationCatalog, OptimizationEntry, platform_provider_identity};
    use crate::performance::retention::retained_inputs_for_test;

    #[test]
    fn logical_provenance_uses_definitions_after_thin_lto_imports() {
        let catalog = OptimizationCatalog {
            entries: vec![entry(
                "std",
                "std.lib",
                "bray_platform_standard_output_write",
            )],
        };

        let map = "LOAD bray_platform_process.lib\n\
            bray_platform_filesystem.lib(hash-filesystem.o)\n\
            0000 bray_platform_standard_output_write application.obj";

        let inputs = retained_inputs_for_test(map);

        assert_eq!(
            catalog.retained_provenance(map, &inputs),
            [
                "bray_platform_filesystem".to_owned(),
                "bray_platform_standard_streams".to_owned(),
                "std".to_owned(),
            ]
        );
    }

    #[test]
    fn platform_symbols_resolve_to_capability_families() {
        let cases = [
            ("bray_platform_context_identity", "bray_platform_core"),
            (
                "bray_platform_standard_output_write",
                "bray_platform_standard_streams",
            ),
            ("bray_platform_file_open", "bray_platform_filesystem"),
            ("bray_platform_child_spawn", "bray_platform_process"),
            ("bray_platform_time_observe", "bray_platform_temporal"),
            (
                "bray_platform_dynamic_library_symbol",
                "bray_platform_dynamic",
            ),
        ];

        for (symbol, provider) in cases {
            assert_eq!(platform_provider_identity(symbol), Some(provider));
        }

        assert_eq!(platform_provider_identity("bray_runtime_memory_copy"), None);
    }

    fn entry(partition: &str, fallback: &str, symbol: &str) -> OptimizationEntry {
        OptimizationEntry {
            partition: partition.to_owned(),
            path: format!("{partition}_optimization.lib"),
            bytes: 1,
            fallback: fallback.to_owned(),
            symbols: BTreeSet::from([symbol.to_owned()]),
        }
    }
}
