use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use bray_base::{lowercase_hex, sha256_file};
use bray_diagnostics::DiagnosticLlvmToolRole;

use super::model::{
    ArtifactDependencies, ArtifactKind, ArtifactReport, BoundedList, LinkerMapReport,
    MAX_DYNAMIC_LIBRARY_COUNT, MAX_RETAINED_INPUT_COUNT, MAX_SECTION_COUNT, RetainedInput,
    SectionSize,
};

pub(super) fn inspect(
    kind: ArtifactKind,
    artifact: &Path,
    map: Option<InspectedLinkerMap>,
) -> Result<ArtifactReport, String> {
    let metadata = fs::metadata(artifact)
        .map_err(|error| format!("could not inspect {}: {error}", artifact.display()))?;

    let readobj = bray_tooling::llvm_tool_path(DiagnosticLlvmToolRole::ObjectInspector)
        .map_err(|error| format!("llvm-readobj is unavailable: {error}"))?;

    let output = Command::new(readobj)
        .args(["--sections", "--needed-libs"])
        .arg(artifact)
        .output()
        .map_err(|error| format!("could not run llvm-readobj: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "llvm-readobj could not inspect {}: {}",
            artifact.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let inspection = String::from_utf8_lossy(&output.stdout);

    let (linker_map, static_archives, static_inputs) = match map {
        Some(inspection) => (
            Some(inspection.report),
            inspection.retained_archives,
            inspection.retained_inputs,
        ),
        None => (None, empty_list(), empty_list()),
    };

    Ok(ArtifactReport {
        kind,
        path: crate::path::slash_separated(artifact),
        bytes: metadata.len(),
        sections: bounded(parse_sections(&inspection), MAX_SECTION_COUNT),
        dependencies: ArtifactDependencies {
            static_archives,
            static_inputs,
            dynamic_libraries: bounded(
                parse_needed_libraries(&inspection),
                MAX_DYNAMIC_LIBRARY_COUNT,
            ),
        },
        linker_map,
    })
}

pub(super) struct InspectedLinkerMap {
    report: LinkerMapReport,
    retained_archives: BoundedList<String>,
    retained_inputs: BoundedList<RetainedInput>,
    contents: String,
}

impl InspectedLinkerMap {
    pub(super) fn contains_symbol(&self, symbol: &str) -> bool {
        crate::link_map::contains_symbol(&self.contents, symbol)
    }

    pub(super) fn contains_logical_provenance(&self, identity: &str) -> bool {
        self.report
            .logical_provenance
            .entries
            .binary_search_by(|candidate| candidate.as_str().cmp(identity))
            .is_ok()
    }
}

pub(super) fn inspect_map(
    path: &Path,
    catalog: Option<&super::optimization::OptimizationCatalog>,
) -> Result<InspectedLinkerMap, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("could not read linker map {}: {error}", path.display()))?;

    let contents = String::from_utf8_lossy(&bytes).into_owned();
    let retained_inputs = parse_retained_inputs(&contents);
    let retained_archives = retained_archives(&retained_inputs);

    let logical_provenance = catalog
        .map(|catalog| catalog.retained_provenance(&contents, &retained_inputs))
        .unwrap_or_default();

    Ok(InspectedLinkerMap {
        report: LinkerMapReport {
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: lowercase_hex(&sha256_file(path).map_err(|error| {
                format!("could not hash linker map {}: {error}", path.display())
            })?),
            logical_provenance: bounded(logical_provenance, MAX_RETAINED_INPUT_COUNT),
        },
        retained_archives: bounded(retained_archives, MAX_RETAINED_INPUT_COUNT),
        retained_inputs: bounded(retained_inputs, MAX_RETAINED_INPUT_COUNT),
        contents,
    })
}

pub(super) fn inspect_physical(
    kind: ArtifactKind,
    artifact: &Path,
    map: &Path,
) -> Result<ArtifactReport, String> {
    let map = inspect_map(map, None)?;

    inspect(kind, artifact, Some(map))
}

fn retained_archives(inputs: &[RetainedInput]) -> Vec<String> {
    inputs
        .iter()
        .filter(|input| input.member.is_some() || is_archive(&input.artifact))
        .map(|input| input.artifact.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn parse_sections(report: &str) -> Vec<SectionSize> {
    let mut sections: BTreeMap<String, u64> = BTreeMap::new();
    let mut inside_section = false;
    let mut name = None;
    let mut raw_size = None;
    let mut virtual_size = None;

    for line in report.lines().map(str::trim) {
        if line == "Section {" {
            inside_section = true;
            name = None;
            raw_size = None;
            virtual_size = None;
        } else if inside_section && let Some(value) = line.strip_prefix("Name: ") {
            name = Some(value.split_whitespace().next().unwrap_or(value).to_owned());
        } else if inside_section && let Some(value) = line.strip_prefix("RawDataSize: ") {
            raw_size = parse_number(value.split_whitespace().next().unwrap_or(value));
        } else if inside_section && let Some(value) = line.strip_prefix("Size: ") {
            virtual_size = parse_number(value.split_whitespace().next().unwrap_or(value));
        } else if inside_section && line == "}" {
            if let (Some(name), Some(bytes)) = (name.take(), raw_size.or(virtual_size)) {
                sections
                    .entry(name)
                    .and_modify(|total| *total = total.saturating_add(bytes))
                    .or_insert(bytes);
            }

            inside_section = false;
        }
    }

    sections
        .into_iter()
        .map(|(name, bytes)| SectionSize { name, bytes })
        .collect()
}

fn parse_needed_libraries(report: &str) -> Vec<String> {
    let mut libraries = Vec::new();
    let mut inside = false;

    for line in report.lines().map(str::trim) {
        match line {
            "NeededLibraries [" => inside = true,
            "]" if inside => inside = false,
            value if inside && !value.is_empty() => libraries.push(value.to_owned()),
            _ => {}
        }
    }

    libraries.sort();
    libraries.dedup();

    libraries
}

fn parse_retained_inputs(map: &str) -> Vec<RetainedInput> {
    let mut inputs = BTreeSet::new();

    for token in retained_map_tokens(map) {
        let token = token.trim_matches(|character: char| matches!(character, ',' | ';' | '"'));

        if let Some((artifact, member)) = archive_member(token) {
            inputs.insert(RetainedInput {
                artifact: artifact.to_owned(),
                member: Some(member.to_owned()),
            });
        } else if let Some((artifact, member)) = microsoft_archive_member(token) {
            inputs.insert(RetainedInput {
                artifact: artifact.to_owned(),
                member: Some(member.to_owned()),
            });
        } else if is_standalone_native_input(token) {
            inputs.insert(RetainedInput {
                artifact: token.to_owned(),
                member: None,
            });
        }
    }

    inputs.into_iter().collect()
}

fn retained_map_tokens(map: &str) -> impl Iterator<Item = &str> {
    map.lines()
        .filter(|line| !line.trim_start().starts_with("LOAD "))
        .flat_map(str::split_whitespace)
}

fn microsoft_archive_member(token: &str) -> Option<(&str, &str)> {
    let (artifact, member) = token.rsplit_once(':')?;

    if artifact.len() > 1 && [".o", ".obj"].iter().any(|suffix| member.ends_with(suffix)) {
        Some((artifact, member))
    } else {
        None
    }
}

fn archive_member(token: &str) -> Option<(&str, &str)> {
    let open = token.rfind('(')?;
    let member = token.get(open + 1..)?.strip_suffix(')')?;
    let artifact = token.get(..open)?;

    if is_archive(artifact) && !member.is_empty() {
        Some((artifact, member))
    } else {
        None
    }
}

fn is_native_input(value: &str) -> bool {
    is_archive(value)
        || [".o", ".obj", ".so", ".dylib", ".dll"]
            .iter()
            .any(|suffix| value.to_ascii_lowercase().ends_with(suffix))
}

fn is_standalone_native_input(value: &str) -> bool {
    let colon = value.find(':');

    let has_windows_drive = colon == Some(1)
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic());

    is_native_input(value) && (colon.is_none() || has_windows_drive)
}

fn is_archive(value: &str) -> bool {
    let value = value.to_ascii_lowercase();

    value.ends_with(".a") || value.ends_with(".lib") || value.ends_with(".rlib")
}

fn parse_number(value: &str) -> Option<u64> {
    value
        .strip_prefix("0x")
        .and_then(|value| u64::from_str_radix(value, 16).ok())
        .or_else(|| value.parse().ok())
}

#[cfg(test)]
pub(super) fn retained_inputs_for_test(map: &str) -> Vec<RetainedInput> {
    parse_retained_inputs(map)
}

#[cfg(test)]
pub(super) fn bounded_retained_inputs_for_test(map: &str) -> BoundedList<RetainedInput> {
    bounded(parse_retained_inputs(map), MAX_RETAINED_INPUT_COUNT)
}

#[cfg(test)]
pub(super) fn sections_for_test(report: &str) -> Vec<SectionSize> {
    parse_sections(report)
}

fn bounded<T>(mut entries: Vec<T>, limit: usize) -> BoundedList<T> {
    let omitted_count = u64::try_from(entries.len().saturating_sub(limit)).unwrap_or(u64::MAX);

    entries.truncate(limit);

    BoundedList {
        entries,
        omitted_count,
    }
}

fn empty_list<T>() -> BoundedList<T> {
    BoundedList {
        entries: Vec::new(),
        omitted_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_retained_inputs, retained_archives};

    #[test]
    fn retained_archive_inventory_is_unique_and_sorted() {
        let inputs = parse_retained_inputs(
            "std.lib:first.obj bray_platform_standard_streams:output.obj std.lib:second.obj",
        );

        assert_eq!(
            retained_archives(&inputs),
            [
                "bray_platform_standard_streams".to_owned(),
                "std.lib".to_owned()
            ]
        );
    }
}
