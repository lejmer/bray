use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use bray_base::{lowercase_hex, sha256_file};
use bray_diagnostics::DiagnosticLlvmToolRole;

use super::model::{
    ArtifactDependencies, ArtifactKind, ArtifactReport, BoundedList, LinkerMapReport, RetainedInput,
    SectionSize, MAX_DYNAMIC_LIBRARY_COUNT, MAX_RETAINED_INPUT_COUNT, MAX_SECTION_COUNT,
};

pub(super) fn inspect(
    kind: ArtifactKind,
    artifact: &Path,
    map: Option<&Path>,
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
    let linker_map = map.map(inspect_map).transpose()?;

    let static_inputs = linker_map
        .as_ref()
        .map(|inspection| inspection.retained_inputs.clone())
        .unwrap_or_else(empty_list);

    Ok(ArtifactReport {
        kind,
        path: crate::path::slash_separated(artifact),
        bytes: metadata.len(),
        sections: bounded(parse_sections(&inspection), MAX_SECTION_COUNT),
        dependencies: ArtifactDependencies {
            static_inputs,
            dynamic_libraries: bounded(
                parse_needed_libraries(&inspection),
                MAX_DYNAMIC_LIBRARY_COUNT,
            ),
        },
        linker_map: linker_map.map(|inspection| inspection.report),
    })
}

struct InspectedLinkerMap {
    report: LinkerMapReport,
    retained_inputs: BoundedList<RetainedInput>,
}

fn inspect_map(path: &Path) -> Result<InspectedLinkerMap, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("could not read linker map {}: {error}", path.display()))?;

    let text = String::from_utf8_lossy(&bytes);

    Ok(InspectedLinkerMap {
        report: LinkerMapReport {
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: lowercase_hex(&sha256_file(path).map_err(|error| {
                format!("could not hash linker map {}: {error}", path.display())
            })?),
        },
        retained_inputs: bounded(parse_retained_inputs(&text), MAX_RETAINED_INPUT_COUNT),
    })
}

fn parse_sections(report: &str) -> Vec<SectionSize> {
    let mut sections = Vec::new();
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
                sections.push(SectionSize { name, bytes });
            }

            inside_section = false;
        }
    }

    sections.sort_by(|left, right| left.name.cmp(&right.name));

    sections
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

    for token in map.split_whitespace() {
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
        } else if is_native_input(token) {
            inputs.insert(RetainedInput {
                artifact: token.to_owned(),
                member: None,
            });
        }
    }

    inputs.into_iter().collect()
}

pub(super) fn contains_retained_provenance(map: &str, expected: &str) -> bool {
    let expected = expected.to_ascii_lowercase();

    map.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| matches!(character, ',' | ';' | '"'));

        if let Some((artifact, member)) = archive_member(token)
            .or_else(|| microsoft_archive_member(token))
        {
            return artifact.to_ascii_lowercase().contains(&expected)
                || member.to_ascii_lowercase().contains(&expected);
        }

        !is_native_input(token) && token.to_ascii_lowercase().contains(&expected)
    })
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

fn is_archive(value: &str) -> bool {
    let value = value.to_ascii_lowercase();

    value.ends_with(".a") || value.ends_with(".lib")
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
