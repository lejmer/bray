use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;
use sha2::{Digest as _, Sha256};

use super::progress;
use crate::bundle::DirectoryPublication;

const CACHE_FORMAT_REVISION: u8 = 3;
const CACHE_INPUT_FILE_NAME: &str = "input.sha256";
const INPUT_FILES: &[&str] = &["Cargo.lock", "Cargo.toml", "xtask/Cargo.toml"];
const SOURCE_ROOTS: &[&str] = &[
    "crates",
    "standard-library",
    "third-party/temporal",
    "toolchains",
    "xtask/src",
];
const EXCLUDED_SOURCE_PATHS: &[&str] = &[
    "xtask/src/performance/html.rs",
    "xtask/src/performance/presentation.rs",
    "xtask/src/performance/ranking.rs",
    "xtask/src/performance/report.rs",
];

pub(super) struct PreparedToolchain {
    runtime: PathBuf,
    observation_runtime: PathBuf,
    toolchain: PathBuf,
}

impl PreparedToolchain {
    fn from_root(root: PathBuf) -> Self {
        Self {
            runtime: root.join("runtime").join("bray-runtime.brayrt"),
            observation_runtime: root.join("observation-runtime").join("bray-runtime.brayrt"),
            toolchain: root.join("toolchain"),
        }
    }

    pub(super) fn runtime(&self) -> &Path {
        &self.runtime
    }

    pub(super) fn observation_runtime(&self) -> &Path {
        &self.observation_runtime
    }

    pub(super) fn toolchain(&self) -> &Path {
        &self.toolchain
    }
}

pub(super) fn prepare(root: &Path, target: NativeTarget) -> Result<PreparedToolchain, String> {
    progress::phase("Checking performance toolchain cache");

    let input = input_digest(root, target)?;

    let cache = crate::workspace::cargo_target(root)
        .join(".bray")
        .join("performance")
        .join(target.as_str());

    if cached_input(&cache)?.as_deref() == Some(input.as_str()) && cache_is_complete(&cache) {
        progress::phase("Reusing performance toolchain");

        return Ok(PreparedToolchain::from_root(cache));
    }

    let publication =
        DirectoryPublication::begin(&cache, "p-").map_err(|error| error.to_string())?;

    progress::phase("Preparing performance runtime");

    let runtime_directory = publication.contents().join("runtime");
    let runtime = crate::runtime_artifact::build_for_readiness(target, &runtime_directory)?;
    let toolchain = publication.contents().join("toolchain");

    progress::phase("Assembling performance toolchain");

    crate::native_toolchain::assemble_in_publication(
        root,
        target,
        &runtime,
        publication.work(),
        &toolchain,
    )?;

    progress::phase("Preparing performance observations");

    crate::runtime_artifact::build_for_performance_observation(
        target,
        &publication.contents().join("observation-runtime"),
    )?;

    fs::write(publication.contents().join(CACHE_INPUT_FILE_NAME), &input).map_err(|error| {
        format!("could not write performance toolchain cache identity: {error}")
    })?;

    let root = publication.publish().map_err(|error| error.to_string())?;

    Ok(PreparedToolchain::from_root(root))
}

fn cached_input(cache: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(cache.join(CACHE_INPUT_FILE_NAME)) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "could not read performance toolchain cache identity: {error}"
        )),
    }
}

fn cache_is_complete(cache: &Path) -> bool {
    [
        cache.join("runtime").join("bray-runtime.brayrt"),
        cache
            .join("observation-runtime")
            .join("bray-runtime.brayrt"),
        cache
            .join("toolchain")
            .join("lib")
            .join("bray")
            .join("standard-library")
            .join(bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME),
    ]
    .into_iter()
    .all(|path| path.is_file())
}

fn input_digest(root: &Path, target: NativeTarget) -> Result<String, String> {
    let mut digest = Sha256::new();

    digest.update([CACHE_FORMAT_REVISION]);
    digest.update(target.as_str().as_bytes());
    digest.update([0]);

    for input in INPUT_FILES {
        hash_file(&mut digest, Path::new(input), &root.join(input))?;
    }

    let rustc = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|error| format!("could not inspect the Rust compiler: {error}"))?;

    if !rustc.status.success() {
        return Err("could not inspect the Rust compiler".to_owned());
    }

    digest.update(rustc.stdout);

    for source_root in SOURCE_ROOTS {
        hash_directory(&mut digest, root, Path::new(source_root))?;
    }

    Ok(bray_base::lowercase_hex(&digest.finalize()))
}

fn hash_directory(digest: &mut Sha256, root: &Path, relative: &Path) -> Result<(), String> {
    let directory = root.join(relative);

    let mut entries = fs::read_dir(&directory)
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?;

    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let entry_relative = relative.join(entry.file_name());

        if source_path_is_excluded(&entry_relative) {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", entry.path().display()))?;

        if file_type.is_dir() {
            hash_directory(digest, root, &entry_relative)?;
        } else if file_type.is_file() {
            hash_file(digest, &entry_relative, &entry.path())?;
        }
    }

    Ok(())
}

fn source_path_is_excluded(path: &Path) -> bool {
    EXCLUDED_SOURCE_PATHS
        .iter()
        .any(|excluded| path == Path::new(excluded))
}

fn hash_file(digest: &mut Sha256, identity: &Path, path: &Path) -> Result<(), String> {
    let identity = identity.to_string_lossy();

    digest.update(identity.len().to_le_bytes());
    digest.update(identity.as_bytes());

    let content = bray_base::sha256_file(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    digest.update(content);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        CACHE_INPUT_FILE_NAME, PreparedToolchain, cache_is_complete, cached_input,
        source_path_is_excluded,
    };

    #[test]
    fn report_rendering_does_not_invalidate_the_compiled_toolchain() {
        assert!(source_path_is_excluded(Path::new(
            "xtask/src/performance/report.rs"
        )));

        assert!(source_path_is_excluded(Path::new(
            "xtask/src/performance/html.rs"
        )));

        assert!(source_path_is_excluded(Path::new(
            "xtask/src/performance/ranking.rs"
        )));

        assert!(source_path_is_excluded(Path::new(
            "xtask/src/performance/presentation.rs"
        )));

        assert!(!source_path_is_excluded(Path::new(
            "xtask/src/performance/model.rs"
        )));

        assert!(!source_path_is_excluded(Path::new(
            "xtask/src/performance/command/toolchain.rs"
        )));
    }

    #[test]
    fn complete_cache_requires_both_runtimes_and_the_standard_library() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary cache must be created: {error}"));

        let prepared = PreparedToolchain::from_root(directory.path().to_path_buf());

        fs::create_dir_all(
            prepared
                .runtime()
                .parent()
                .unwrap_or_else(|| directory.path()),
        )
        .unwrap_or_else(|error| panic!("runtime directory must be created: {error}"));

        fs::write(prepared.runtime(), b"runtime")
            .unwrap_or_else(|error| panic!("runtime metadata must be written: {error}"));

        assert!(!cache_is_complete(directory.path()));

        fs::create_dir_all(
            prepared
                .observation_runtime()
                .parent()
                .unwrap_or_else(|| directory.path()),
        )
        .unwrap_or_else(|error| panic!("observation runtime directory must be created: {error}"));

        fs::write(prepared.observation_runtime(), b"observation")
            .unwrap_or_else(|error| panic!("observation metadata must be written: {error}"));

        let manifest = prepared
            .toolchain()
            .join("lib")
            .join("bray")
            .join("standard-library")
            .join(bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME);

        fs::create_dir_all(manifest.parent().unwrap_or_else(|| directory.path()))
            .unwrap_or_else(|error| panic!("manifest directory must be created: {error}"));

        fs::write(manifest, b"manifest")
            .unwrap_or_else(|error| panic!("manifest must be written: {error}"));

        assert!(cache_is_complete(directory.path()));
    }

    #[test]
    fn missing_cache_identity_is_a_cache_miss() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary cache must be created: {error}"));

        assert_eq!(
            cached_input(directory.path())
                .unwrap_or_else(|error| panic!("missing cache identity must be readable: {error}")),
            None
        );

        fs::write(directory.path().join(CACHE_INPUT_FILE_NAME), "digest")
            .unwrap_or_else(|error| panic!("cache identity must be written: {error}"));

        assert_eq!(
            cached_input(directory.path())
                .unwrap_or_else(|error| panic!("cache identity must be readable: {error}")),
            Some("digest".to_owned())
        );
    }
}
