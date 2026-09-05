use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use bray_standard_library::{StandardLibraryArtifactKind, decode_standard_library_manifest};

use super::model::{OptimizationArtifactReport, RetainedInput, WorkloadReport};

pub(super) struct OptimizationCatalog {
    entries: Vec<OptimizationEntry>,
    provider_symbols: BTreeMap<&'static str, &'static str>,
}

impl OptimizationCatalog {
    pub(super) fn load(root: &Path, target: bray_target::NativeTarget) -> Result<Self, String> {
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
                    .chain(
                        optimization
                            .platform_services()
                            .iter()
                            .map(|role| role.native_symbol().to_owned()),
                    )
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

        let provider_symbols = bray_runtime_interface::PlatformServiceRole::ALL
            .iter()
            .copied()
            .map(|role| {
                (
                    role.native_symbol(),
                    platform_provider_identity(role.family()),
                )
            })
            .collect();

        Ok(Self {
            entries,
            provider_symbols,
        })
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

                if let Some(provider) = self.provider_symbols.get(symbol.as_str()) {
                    provenance.insert((*provider).to_owned());
                }
            }

            if retained_partition {
                provenance.insert(entry.partition.clone());
            }
        }

        provenance.into_iter().collect()
    }

    pub(super) fn reports(&self, workloads: &[WorkloadReport]) -> Vec<OptimizationArtifactReport> {
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

const fn platform_provider_identity(
    family: bray_runtime_interface::PlatformServiceFamily,
) -> &'static str {
    use bray_runtime_interface::PlatformServiceFamily as Family;

    match family {
        Family::Core => "bray_platform_core",
        Family::StandardStreams => "bray_platform_standard_streams",
        Family::Filesystem => "bray_platform_filesystem",
        Family::Process => "bray_platform_process",
        Family::Thread => "bray_platform_thread",
        Family::Temporal => "bray_platform_temporal",
        Family::DynamicLibrary => "bray_platform_dynamic",
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
    use std::collections::{BTreeMap, BTreeSet};

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
            provider_symbols: BTreeMap::from([(
                "bray_platform_standard_output_write",
                "bray_platform_standard_streams",
            )]),
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
        use bray_runtime_interface::PlatformServiceFamily as Family;

        let cases = [
            (Family::Core, "bray_platform_core"),
            (Family::StandardStreams, "bray_platform_standard_streams"),
            (Family::Filesystem, "bray_platform_filesystem"),
            (Family::Process, "bray_platform_process"),
            (Family::Thread, "bray_platform_thread"),
            (Family::Temporal, "bray_platform_temporal"),
            (Family::DynamicLibrary, "bray_platform_dynamic"),
        ];

        for (family, provider) in cases {
            assert_eq!(platform_provider_identity(family), provider);
        }
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
