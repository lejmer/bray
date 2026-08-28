use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use bray_base::NonEmptySharedStr;
use bray_base::lowercase_hex;
use sha2::{Digest, Sha256};

use super::model::{Description, LinkKind, Manifest, Source};
use super::render::{RenderedTarget, render_all};
use super::validation::validate;
use crate::workspace;

const INPUT_PATH: &str = "standard-library/targets/os-bindings.json";
const BRAY_OUTPUT_ROOT: &str = "standard-library/std/src/os/generated";
const PROBE_OUTPUT_ROOT: &str = "standard-library/targets/probes";

pub(crate) fn native_links(
    target: &bray_target::TargetIdentity,
) -> Result<Vec<bray_symbols::NativeLinkRequirement>, String> {
    let loaded = read_description()?;

    let target = loaded
        .description
        .targets
        .iter()
        .find(|description| description.target == target.as_str())
        .ok_or_else(|| format!("no OS binding description exists for {}", target.as_str()))?;

    target
        .links
        .iter()
        .map(|link| {
            let name = NonEmptySharedStr::try_new(link.name.clone())
                .ok_or_else(|| format!("native link name {} is empty", link.name))?;

            let kind = match link.kind {
                LinkKind::Dynamic => bray_symbols::NativeLinkKind::Dynamic,
                LinkKind::Framework => bray_symbols::NativeLinkKind::Framework,
                LinkKind::Static => bray_symbols::NativeLinkKind::Static,
                LinkKind::System => bray_symbols::NativeLinkKind::System,
            };

            Ok(bray_symbols::NativeLinkRequirement::new(name, kind))
        })
        .collect()
}

pub(super) fn load() -> Result<LoadedDescription, String> {
    let loaded = read_description()?;
    let rendered = render_all(&loaded.description, &loaded.digest)?;

    Ok(LoadedDescription {
        root: loaded.root,
        description: loaded.description,
        rendered,
    })
}

fn read_description() -> Result<DescriptionFile, String> {
    let root = workspace::root()?;
    let input = root.join(INPUT_PATH);

    let (description, digest) = read_input(&input)?;

    Ok(DescriptionFile {
        root,
        description,
        digest,
    })
}

fn read_input(input: &Path) -> Result<(Description, String), String> {
    let manifest_bytes =
        std::fs::read(input).map_err(|error| workspace::io_error("read", input, error))?;

    let manifest = serde_json::from_slice::<Manifest>(&manifest_bytes)
        .map_err(|error| format!("could not parse {}: {error}", input.display()))?;

    if manifest.sources.is_empty() {
        return Err(format!(
            "OS binding manifest {} has no sources",
            input.display()
        ));
    }

    let parent = input
        .parent()
        .ok_or_else(|| format!("{} has no parent", input.display()))?;

    let mut names = BTreeSet::new();
    let mut sources = Vec::with_capacity(manifest.sources.len());
    let mut digest = Sha256::new();

    digest.update(canonical_text(&manifest_bytes));

    for name in &manifest.sources {
        let relative = Path::new(name);

        if relative
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("json")
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!(
                "OS binding source path {name} must be a relative JSON path"
            ));
        }

        if !names.insert(relative.to_path_buf()) {
            return Err(format!("OS binding source path {name} is repeated"));
        }

        let path = parent.join(relative);

        let bytes =
            std::fs::read(&path).map_err(|error| workspace::io_error("read", &path, error))?;

        let source = serde_json::from_slice::<Source>(&bytes)
            .map_err(|error| format!("could not parse {}: {error}", path.display()))?;

        if source.groups.is_empty() && source.targets.is_empty() {
            return Err(format!("OS binding source {} is empty", path.display()));
        }

        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        let canonical = canonical_text(&bytes);

        digest.update((canonical.len() as u64).to_le_bytes());
        digest.update(&canonical);
        sources.push(source);
    }

    let description = Description::from_sources(manifest.format, sources).expand_groups()?;

    validate(&description)?;

    Ok((description, lowercase_hex(&digest.finalize())))
}

pub(super) fn generate(check: bool) -> Result<(), String> {
    let loaded = load()?;
    let files = loaded.files();

    if check {
        check_files(&files)
    } else {
        synchronize_files(&files)
    }
}

pub(super) struct LoadedDescription {
    pub(super) root: PathBuf,
    pub(super) description: Description,
    rendered: Vec<RenderedTarget>,
}

struct DescriptionFile {
    root: PathBuf,
    description: Description,
    digest: String,
}

impl LoadedDescription {
    pub(super) fn probe(&self, target: &str) -> Option<ProbeFile<'_>> {
        let rendered = self
            .rendered
            .iter()
            .find(|rendered| rendered.target == target)?;

        let target = self
            .description
            .targets
            .iter()
            .find(|description| description.target == target)?;

        Some(ProbeFile {
            source: self
                .root
                .join(PROBE_OUTPUT_ROOT)
                .join(format!("{}.c", rendered.file_stem)),
            target,
        })
    }

    fn files(&self) -> Vec<GeneratedFile> {
        self.rendered
            .iter()
            .flat_map(|target| {
                [
                    GeneratedFile {
                        path: self
                            .root
                            .join(BRAY_OUTPUT_ROOT)
                            .join(format!("{}.bray", target.file_stem)),
                        contents: target.bray.clone(),
                    },
                    GeneratedFile {
                        path: self
                            .root
                            .join(PROBE_OUTPUT_ROOT)
                            .join(format!("{}.c", target.file_stem)),
                        contents: target.probe.clone(),
                    },
                ]
            })
            .collect()
    }
}

pub(super) struct ProbeFile<'a> {
    pub(super) source: PathBuf,
    pub(super) target: &'a super::model::TargetDescription,
}

struct GeneratedFile {
    path: PathBuf,
    contents: String,
}

fn check_files(files: &[GeneratedFile]) -> Result<(), String> {
    let mut stale = Vec::new();

    for file in files {
        match std::fs::read(&file.path) {
            Ok(actual) if canonical_text(&actual) == canonical_text(file.contents.as_bytes()) => {}
            Ok(_) => stale.push(file.path.display().to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                stale.push(file.path.display().to_string());
            }
            Err(error) => return Err(workspace::io_error("read", &file.path, error)),
        }
    }

    stale.extend(
        obsolete_managed_files(files)?
            .iter()
            .map(|path| path.display().to_string()),
    );

    if stale.is_empty() {
        return Ok(());
    }

    Err(format!("generated output is stale: {}", stale.join(", ")))
}

fn synchronize_files(files: &[GeneratedFile]) -> Result<(), String> {
    for file in files {
        let parent = file
            .path
            .parent()
            .ok_or_else(|| format!("{} has no parent", file.path.display()))?;

        std::fs::create_dir_all(parent)
            .map_err(|error| workspace::io_error("create", parent, error))?;

        if std::fs::read(&file.path)
            .ok()
            .is_some_and(|actual| canonical_text(&actual) == canonical_text(file.contents.as_bytes()))
        {
            continue;
        }

        std::fs::write(&file.path, &file.contents)
            .map_err(|error| workspace::io_error("write", &file.path, error))?;
    }

    for path in obsolete_managed_files(files)? {
        std::fs::remove_file(&path).map_err(|error| workspace::io_error("remove", &path, error))?;
    }

    Ok(())
}

fn canonical_text(bytes: &[u8]) -> Cow<'_, [u8]> {
    if !bytes.windows(2).any(|pair| pair == b"\r\n") {
        return Cow::Borrowed(bytes);
    }

    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"\r\n") {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }

    Cow::Owned(normalized)
}

fn obsolete_managed_files(files: &[GeneratedFile]) -> Result<Vec<PathBuf>, String> {
    let roots = files
        .iter()
        .filter_map(|file| file.path.parent())
        .collect::<BTreeSet<_>>();

    let expected = files
        .iter()
        .map(|file| file.path.as_path())
        .collect::<BTreeSet<_>>();

    let mut obsolete = Vec::new();

    for root in roots {
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(workspace::io_error("read", root, error)),
        };

        for entry in entries {
            let entry = entry.map_err(|error| workspace::io_error("read", root, error))?;
            let path = entry.path();

            if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("bray" | "c")
            ) && !expected.contains(path.as_path())
            {
                obsolete.push(path);
            }
        }
    }

    obsolete.sort_unstable();

    Ok(obsolete)
}

#[cfg(test)]
mod tests {
    use super::{
        GeneratedFile, canonical_text, check_files, read_description, read_input,
        synchronize_files,
    };

    #[test]
    fn checked_in_description_covers_every_native_target() {
        read_description()
            .unwrap_or_else(|error| panic!("binding description must be valid: {error}"));
    }

    #[test]
    fn manifest_sources_stay_beneath_the_manifest_directory() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must exist: {error}"));

        let manifest = directory.path().join("os-bindings.json");

        std::fs::write(&manifest, r#"{"format":1,"sources":["../outside.json"]}"#)
            .unwrap_or_else(|error| panic!("manifest must be written: {error}"));

        assert!(read_input(&manifest).is_err());
    }

    #[test]
    fn manifest_source_paths_are_unique() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must exist: {error}"));

        let manifest = directory.path().join("os-bindings.json");
        let source = directory.path().join("shared.json");

        std::fs::write(
            &manifest,
            r#"{"format":1,"sources":["shared.json","shared.json"]}"#,
        )
        .unwrap_or_else(|error| panic!("manifest must be written: {error}"));

        std::fs::write(
            &source,
            r#"{"groups":[{"name":"shared","targets":["first","second"]}]}"#,
        )
        .unwrap_or_else(|error| panic!("source must be written: {error}"));

        assert!(read_input(&manifest).is_err());
    }

    #[test]
    fn canonical_text_is_independent_of_line_endings() {
        assert_eq!(
            canonical_text(b"first\nsecond\n"),
            canonical_text(b"first\r\nsecond\r\n"),
        );

        assert_eq!(canonical_text(b"first\rsecond").as_ref(), b"first\rsecond");
    }

    #[test]
    fn generated_file_checks_accept_platform_line_endings() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must exist: {error}"));

        let path = directory.path().join("generated.bray");

        std::fs::write(&path, "first\r\nsecond\r\n")
            .unwrap_or_else(|error| panic!("generated output must be written: {error}"));

        let files = [GeneratedFile {
            path,
            contents: "first\nsecond\n".to_owned(),
        }];

        assert!(check_files(&files).is_ok());
    }

    #[test]
    fn managed_directories_reject_and_remove_obsolete_outputs() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory must exist: {error}"));

        let expected = directory.path().join("linux.bray");
        let obsolete = directory.path().join("obsolete.bray");

        let files = [GeneratedFile {
            path: expected.clone(),
            contents: "expected\n".to_owned(),
        }];

        std::fs::write(&expected, "expected\n")
            .unwrap_or_else(|error| panic!("expected output must be written: {error}"));

        std::fs::write(&obsolete, "obsolete\n")
            .unwrap_or_else(|error| panic!("obsolete output must be written: {error}"));

        assert!(check_files(&files).is_err());

        synchronize_files(&files)
            .unwrap_or_else(|error| panic!("outputs must synchronize: {error}"));

        assert!(!obsolete.exists());
        assert!(check_files(&files).is_ok());
    }
}
