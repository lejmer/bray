use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bray_base::lowercase_hex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::text::normalize_line_endings;
use crate::workspace;

use super::tables::Tables;

const VERSION: &str = "17.0.0";
const GENERATOR_REVISION: u32 = 1;
const INPUT_ROOT: &str = "standard-library/targets/unicode/17.0.0";
const TABLE_OUTPUT: &str = "standard-library/std/src/runtime/character/unicode_tables.bray";
const METADATA_OUTPUT: &str = "standard-library/targets/unicode/metadata.json";

pub(in crate::standard_library) fn run(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), String> {
    if arguments.next().as_deref() != Some("generate") {
        return Err("expected `unicode generate [--check]`".to_owned());
    }

    let check = match arguments.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(argument) => return Err(format!("unexpected argument: {argument}")),
    };

    crate::command::reject_trailing_argument(arguments)?;

    generate(check)
}

pub(in crate::standard_library) fn verify() -> Result<(), String> {
    generate(true)
}

pub(in crate::standard_library) fn verify_source(source: &Path) -> Result<(), String> {
    verify()?;

    let path = source.join("std/src/runtime/character/unicode_tables.bray");

    // Minimal source-only bundles may omit character classification entirely.
    if !path
        .try_exists()
        .map_err(|error| workspace::io_error("inspect", &path, error))?
    {
        return Ok(());
    }

    let root = workspace::root()?;
    let expected_path = root.join(TABLE_OUTPUT);

    let expected = std::fs::read(&expected_path)
        .map_err(|error| workspace::io_error("read", &expected_path, error))?;

    check_outputs(&[(path, expected)])
}

fn generate(check: bool) -> Result<(), String> {
    let root = workspace::root()?;
    let inputs = load_inputs(&root)?;
    let tables = load_tables(&inputs)?;
    let table = tables.render(VERSION, GENERATOR_REVISION);
    let metadata = render_metadata(&inputs, &tables, &table)?;

    let outputs = [
        (root.join(TABLE_OUTPUT), table.into_bytes()),
        (root.join(METADATA_OUTPUT), metadata),
    ];

    if check {
        check_outputs(&outputs)
    } else {
        synchronize_outputs(&outputs)
    }
}

fn load_tables(inputs: &[Input; 3]) -> Result<Tables, String> {
    let alphabetic = property_ranges(&inputs[0].contents, &["Alphabetic"])?;
    let whitespace = property_ranges(&inputs[1].contents, &["White_Space"])?;
    let numeric = property_ranges(&inputs[2].contents, &["Decimal", "Digit", "Numeric"])?;

    Ok(Tables::new([&alphabetic, &numeric, &whitespace]))
}

pub(in crate::standard_library) fn table_payloads() -> Result<[Vec<u8>; 5], String> {
    let root = workspace::root()?;

    Ok(load_tables(&load_inputs(&root)?)?.payloads())
}

pub(super) struct Input {
    name: &'static str,
    pub(super) contents: String,
    digest: String,
}

pub(super) fn load_inputs(root: &Path) -> Result<[Input; 3], String> {
    [
        (
            "DerivedCoreProperties.txt",
            "24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08",
        ),
        (
            "PropList.txt",
            "130dcddcaadaf071008bdfce1e7743e04fdfbc910886f017d9f9ac931d8c64dd",
        ),
        (
            "DerivedNumericType.txt",
            "7c83684d3336b698381745b78a971c3e1242cb3fcac58604469086c19b6edcee",
        ),
    ]
    .map(|(name, digest)| load_input(root, name, digest))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?
    .try_into()
    .map_err(|_| "Unicode input inventory is incomplete".to_owned())
}

fn load_input(root: &Path, name: &'static str, expected_digest: &str) -> Result<Input, String> {
    let path = root.join(INPUT_ROOT).join(name);
    let bytes = std::fs::read(&path).map_err(|error| workspace::io_error("read", &path, error))?;

    let contents = String::from_utf8(normalize_line_endings(&bytes).into_owned())
        .map_err(|error| format!("{} is not UTF-8: {error}", path.display()))?;

    let digest = lowercase_hex(&Sha256::digest(contents.as_bytes()));

    if digest != expected_digest {
        return Err(format!("pinned Unicode input digest mismatch: {name}"));
    }

    Ok(Input {
        name,
        contents,
        digest,
    })
}

pub(super) fn property_ranges(
    contents: &str,
    properties: &[&str],
) -> Result<Vec<(u32, u32)>, String> {
    let mut ranges = Vec::new();

    for line in contents.lines() {
        let definition = line.split('#').next().unwrap_or_default().trim();

        if definition.is_empty() {
            continue;
        }

        let Some((range, property)) = definition.split_once(';') else {
            return Err(format!("invalid Unicode data line: {line}"));
        };

        if !properties.contains(&property.trim()) {
            continue;
        }

        ranges.push(parse_range(range.trim())?);
    }

    ranges.sort_unstable();

    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());

    for (start, end) in ranges {
        if let Some((_, previous_end)) = merged.last_mut()
            && start <= previous_end.saturating_add(1)
        {
            *previous_end = (*previous_end).max(end);
        } else {
            merged.push((start, end));
        }
    }

    Ok(merged)
}

fn parse_range(value: &str) -> Result<(u32, u32), String> {
    let (start, end) = value.split_once("..").unwrap_or((value, value));

    let start = u32::from_str_radix(start, 16)
        .map_err(|error| format!("invalid Unicode range start {start}: {error}"))?;

    let end = u32::from_str_radix(end, 16)
        .map_err(|error| format!("invalid Unicode range end {end}: {error}"))?;

    if start > end || end > 0x10_ffff {
        return Err(format!("invalid Unicode range {value}"));
    }

    Ok((start, end))
}

#[derive(Serialize)]
struct Metadata<'a> {
    format: u32,
    unicode_version: &'a str,
    generator_revision: u32,
    generator_sha256: String,
    tables_sha256: String,
    table_bytes: BTreeMap<&'a str, usize>,
    inputs: Vec<MetadataInput<'a>>,
}

#[derive(Serialize)]
struct MetadataInput<'a> {
    name: &'a str,
    sha256: &'a str,
}

fn render_metadata(inputs: &[Input], tables: &Tables, source: &str) -> Result<Vec<u8>, String> {
    let metadata = Metadata {
        format: 1,
        unicode_version: VERSION,
        generator_revision: GENERATOR_REVISION,
        generator_sha256: generator_digest(),
        tables_sha256: lowercase_hex(&Sha256::digest(source.as_bytes())),
        table_bytes: ["root", "branches", "alphabetic", "numeric", "whitespace"]
            .into_iter()
            .zip(tables.sizes())
            .collect(),
        inputs: inputs
            .iter()
            .map(|input| MetadataInput {
                name: input.name,
                sha256: &input.digest,
            })
            .collect(),
    };

    let mut output = serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?;

    output.push(b'\n');

    Ok(output)
}

fn check_outputs(outputs: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    let stale = outputs
        .iter()
        .filter_map(|(path, expected)| match std::fs::read(path) {
            Ok(actual) if normalize_line_endings(&actual) == normalize_line_endings(expected) => {
                None
            }
            _ => Some(path.display().to_string()),
        })
        .collect::<Vec<_>>();

    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "generated Unicode output is stale: {}",
            stale.join(", ")
        ))
    }
}

fn synchronize_outputs(outputs: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    for (path, contents) in outputs {
        let parent = path
            .parent()
            .ok_or_else(|| format!("{} has no parent", path.display()))?;

        std::fs::create_dir_all(parent)
            .map_err(|error| workspace::io_error("create", parent, error))?;

        if std::fs::read(path).ok().as_deref() != Some(contents) {
            std::fs::write(path, contents)
                .map_err(|error| workspace::io_error("write", path, error))?;
        }
    }

    Ok(())
}

fn generator_digest() -> String {
    let mut digest = Sha256::new();

    for source in [
        include_bytes!("command.rs").as_slice(),
        include_bytes!("tables.rs").as_slice(),
    ] {
        digest.update(normalize_line_endings(source));
    }

    lowercase_hex(&digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::{
        INPUT_ROOT, check_outputs, generate, load_input, parse_range, property_ranges,
        verify_source,
    };

    #[test]
    fn alternate_source_bundles_cannot_publish_modified_unicode_tables() {
        let directory = tempfile::tempdir().expect("scratch directory must be available");

        let path = directory
            .path()
            .join("std/src/runtime/character/unicode_tables.bray");

        std::fs::create_dir_all(path.parent().expect("table has a parent"))
            .expect("scratch source root must be writable");

        std::fs::write(&path, "modified").expect("scratch table must be writable");

        assert!(verify_source(directory.path()).is_err());
    }

    #[test]
    fn changed_pinned_inputs_are_rejected_before_generation() {
        let directory = tempfile::tempdir().expect("scratch directory must be available");
        let root = directory.path().join(INPUT_ROOT);

        std::fs::create_dir_all(&root).expect("scratch input root must be writable");

        std::fs::write(root.join("PropList.txt"), "0020 ; White_Space\n")
            .expect("scratch input must be writable");

        assert!(
            load_input(
                directory.path(),
                "PropList.txt",
                "130dcddcaadaf071008bdfce1e7743e04fdfbc910886f017d9f9ac931d8c64dd"
            )
            .is_err()
        );
    }

    #[test]
    fn selected_properties_are_parsed_and_adjacent_ranges_are_merged() {
        let ranges = property_ranges(
            "0041..005A ; Alphabetic # letters\n005B ; Other\n0061..007A ; Alphabetic\n005F..0060 ; Alphabetic\n",
            &["Alphabetic"],
        ).expect("properties must parse");

        assert_eq!(ranges, [(0x41, 0x5a), (0x5f, 0x7a)]);
    }

    #[test]
    fn scalar_and_interval_ranges_are_supported() {
        assert_eq!(parse_range("0020"), Ok((0x20, 0x20)));
        assert_eq!(parse_range("0030..0039"), Ok((0x30, 0x39)));
        assert!(parse_range("110000").is_err());
        assert!(parse_range("0021..0020").is_err());
    }

    #[test]
    fn checked_in_tables_and_metadata_are_fresh() {
        generate(true).expect("Unicode outputs must match pinned inputs and generator");
    }

    #[test]
    fn missing_or_modified_outputs_are_rejected() {
        let directory = tempfile::tempdir().expect("scratch directory must be available");
        let path = directory.path().join("metadata.json");
        let outputs = [(path.clone(), b"expected".to_vec())];

        assert!(check_outputs(&outputs).is_err());
        std::fs::write(path, "modified").expect("scratch output must be writable");
        assert!(check_outputs(&outputs).is_err());
    }
}
