use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_target::NativeTarget;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

const CACHE_FORMAT_REVISION: u8 = 1;
pub(crate) const INPUT_IDENTITY_FILE_NAME: &str = "input.sha256";
const ROOT_INPUTS: &[&str] = &[".cargo/config.toml", "Cargo.toml", "xtask/Cargo.toml"];
const COMMON_SOURCE_INPUTS: &[&str] = &[
    "toolchains",
    "xtask/src/bundle.rs",
    "xtask/src/native_archive.rs",
    "xtask/src/native_symbols.rs",
    "xtask/src/path.rs",
    "xtask/src/input_identity.rs",
    "xtask/src/progress.rs",
    "xtask/src/workspace.rs",
];
const RUNTIME_SOURCE_INPUTS: &[&str] = &[
    "runtime",
    "xtask/src/dependency_audit.rs",
    "xtask/src/link_map.rs",
    "xtask/src/native_product.rs",
    "xtask/src/runtime_artifact",
];
const STANDARD_LIBRARY_SOURCE_INPUTS: &[&str] =
    &["third-party/temporal", "xtask/src/standard_library"];
const RUNTIME_PACKAGES: &[&str] = &[
    "bray-driver",
    "bray-llvm-toolchain",
    "bray-runtime-adapter",
];
const STANDARD_LIBRARY_PACKAGES: &[&str] = &[
    "bray-codegen",
    "bray-compilation",
    "bray-emitter",
    "bray-linker",
    "bray-package-interface",
    "bray-platform-abi",
    "bray-project",
    "bray-runtime-interface",
    "bray-standard-library",
    "bray-symbols",
    "bray-target",
    "bray-tooling",
];
const BUILD_ENVIRONMENT_NAMES: &[&str] = &[
    "AR",
    "BRAY_LLVM_PREFIX",
    "CC",
    "CFLAGS",
    "CXX",
    "CXXFLAGS",
    "DEVELOPER_DIR",
    "INCLUDE",
    "LIB",
    "LLVM_SYS_221_PREFIX",
    "MACOSX_DEPLOYMENT_TARGET",
    "PATH",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "RUSTUP_TOOLCHAIN",
    "SDKROOT",
];
const BUILD_ENVIRONMENT_PREFIXES: &[&str] = &[
    "AR_",
    "CARGO_BUILD_",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_PROFILE_",
    "CARGO_TARGET_",
    "CC_",
    "CFLAGS_",
    "CXX_",
    "CXXFLAGS_",
];

#[derive(Clone, Copy)]
pub(crate) enum Component {
    Runtime,
    StandardLibrary,
}

impl Component {
    const fn identity(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::StandardLibrary => "standard-library",
        }
    }

    const fn packages(self) -> &'static [&'static str] {
        match self {
            Self::Runtime => RUNTIME_PACKAGES,
            Self::StandardLibrary => STANDARD_LIBRARY_PACKAGES,
        }
    }

    const fn source_inputs(self) -> &'static [&'static str] {
        match self {
            Self::Runtime => RUNTIME_SOURCE_INPUTS,
            Self::StandardLibrary => STANDARD_LIBRARY_SOURCE_INPUTS,
        }
    }
}

pub(crate) struct WorkspaceSources {
    packages: BTreeMap<String, CargoPackage>,
    dependencies: BTreeMap<String, Vec<String>>,
}

impl WorkspaceSources {
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        let output = Command::new("cargo")
            .current_dir(root)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--all-features",
            ])
            .output()
            .map_err(|error| format!("could not inspect workspace packages: {error}"))?;

        if !output.status.success() {
            return Err("could not inspect workspace packages".to_owned());
        }

        let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("could not decode workspace packages: {error}"))?;

        let resolve = metadata
            .resolve
            .ok_or_else(|| "workspace package resolution is unavailable".to_owned())?;

        let packages = metadata
            .packages
            .into_iter()
            .map(|package| (package.id.clone(), package))
            .collect();

        let dependencies = resolve
            .nodes
            .into_iter()
            .map(|node| {
                let dependencies = node
                    .deps
                    .into_iter()
                    .filter(|dependency| {
                        dependency
                            .dep_kinds
                            .iter()
                            .any(|kind| kind.kind.as_deref() != Some("dev"))
                    })
                    .map(|dependency| dependency.pkg)
                    .collect();

                (node.id, dependencies)
            })
            .collect();

        Ok(Self {
            packages,
            dependencies,
        })
    }

    fn resolved_packages<'package>(
        &'package self,
        names: &[&str],
    ) -> Result<Vec<&'package CargoPackage>, String> {
        let mut pending = names
            .iter()
            .map(|name| self.local_package_id(name))
            .collect::<Result<Vec<_>, _>>()?;

        let mut visited = BTreeSet::new();

        while let Some(package) = pending.pop() {
            if !visited.insert(package.clone()) {
                continue;
            }

            if let Some(dependencies) = self.dependencies.get(&package) {
                pending.extend(dependencies.iter().cloned());
            }
        }

        let mut packages = visited
            .into_iter()
            .map(|id| {
                self.packages
                    .get(&id)
                    .ok_or_else(|| format!("resolved package {id} is unavailable"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        packages.sort_by(|left, right| left.id.cmp(&right.id));

        Ok(packages)
    }

    fn local_package_id(&self, name: &str) -> Result<String, String> {
        let matches = self
            .packages
            .values()
            .filter(|package| package.name == name && package.source.is_none())
            .map(|package| package.id.clone())
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [identity] => Ok(identity.clone()),
            [] => Err(format!("workspace package {name} is unavailable")),
            _ => Err(format!("workspace package {name} is ambiguous")),
        }
    }
}

pub(crate) fn input_digest(
    root: &Path,
    target: Option<NativeTarget>,
    component: Component,
    configuration: &[&str],
    additional_inputs: &[&Path],
    sources: &WorkspaceSources,
) -> Result<String, String> {
    let mut digest = Sha256::new();

    digest.update([CACHE_FORMAT_REVISION]);

    hash_text(&mut digest, component.identity());

    if let Some(target) = target {
        hash_text(&mut digest, target.as_str());
    }

    for value in configuration {
        hash_text(&mut digest, value);
    }

    for input in ROOT_INPUTS {
        hash_path(&mut digest, root, &root.join(input))?;
    }

    hash_command_identity(&mut digest, "rustc", "Rust compiler")?;
    hash_command_identity(&mut digest, "cargo", "Cargo")?;

    for (name, value) in build_environment() {
        hash_text(&mut digest, &name);
        hash_text(&mut digest, &value);
    }

    for input in COMMON_SOURCE_INPUTS.iter().chain(component.source_inputs()) {
        hash_path(&mut digest, root, &root.join(input))?;
    }

    for input in additional_inputs {
        hash_path(&mut digest, root, input)?;
    }

    for package in sources.resolved_packages(component.packages())? {
        if package.source.is_some() {
            hash_text(&mut digest, &package.id);
            hash_text(&mut digest, package.source.as_deref().unwrap_or_default());
            hash_text(&mut digest, package.checksum.as_deref().unwrap_or_default());

            continue;
        }

        let directory = package
            .manifest_path
            .parent()
            .ok_or_else(|| format!("package {} has no source directory", package.name))?;

        if !directory.starts_with(root) {
            return Err(format!(
                "workspace package source is outside the repository: {}",
                directory.display()
            ));
        }

        hash_path(&mut digest, root, directory)?;
    }

    Ok(bray_base::lowercase_hex(&digest.finalize()))
}

pub(crate) fn stored_digest_matches(output: &Path, expected: &str) -> Result<bool, String> {
    let path = output.join(INPUT_IDENTITY_FILE_NAME);

    match fs::read_to_string(&path) {
        Ok(actual) => Ok(actual == expected),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
    }
}

pub(crate) fn write_digest(output: &Path, digest: &str) -> Result<(), String> {
    let path = output.join(INPUT_IDENTITY_FILE_NAME);

    fs::write(&path, digest).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn hash_path(digest: &mut Sha256, root: &Path, path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;

    if metadata.is_dir() {
        return hash_directory(digest, root, path);
    }

    if metadata.is_file() {
        return hash_file(digest, root, path);
    }

    Err(format!(
        "cache input is neither a file nor a directory: {}",
        path.display()
    ))
}

fn hash_directory(digest: &mut Sha256, root: &Path, directory: &Path) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not inspect {}: {error}", directory.display()))?;

    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not inspect {}: {error}", entry.path().display()))?;

        if file_type.is_dir() {
            hash_directory(digest, root, &entry.path())?;
        } else if file_type.is_file() {
            hash_file(digest, root, &entry.path())?;
        }
    }

    Ok(())
}

fn hash_file(digest: &mut Sha256, root: &Path, path: &Path) -> Result<(), String> {
    let identity = path.strip_prefix(root).unwrap_or(path).to_string_lossy();

    hash_text(digest, &identity);

    let content = bray_base::sha256_file(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    digest.update(content);

    Ok(())
}

fn hash_text(digest: &mut Sha256, value: &str) {
    digest.update(value.len().to_le_bytes());
    digest.update(value.as_bytes());
}

fn hash_command_identity(digest: &mut Sha256, program: &str, owner: &str) -> Result<(), String> {
    let output = Command::new(program)
        .arg("-vV")
        .output()
        .map_err(|error| format!("could not inspect {owner}: {error}"))?;

    if !output.status.success() {
        return Err(format!("could not inspect {owner}"));
    }

    digest.update(output.stdout.len().to_le_bytes());
    digest.update(output.stdout);

    Ok(())
}

fn build_environment() -> Vec<(String, String)> {
    let mut environment = std::env::vars_os()
        .filter_map(|(name, value)| {
            let name = name.to_string_lossy().to_ascii_uppercase();

            (BUILD_ENVIRONMENT_NAMES.contains(&name.as_str())
                || BUILD_ENVIRONMENT_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix)))
            .then(|| (name, value.to_string_lossy().into_owned()))
        })
        .collect::<Vec<_>>();

    environment.sort();

    environment
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    resolve: Option<CargoResolve>,
}

#[derive(Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Deserialize)]
struct CargoResolve {
    nodes: Vec<CargoNode>,
}

#[derive(Deserialize)]
struct CargoNode {
    id: String,
    deps: Vec<CargoNodeDependency>,
}

#[derive(Deserialize)]
struct CargoNodeDependency {
    pkg: String,
    dep_kinds: Vec<CargoDependencyKind>,
}

#[derive(Deserialize)]
struct CargoDependencyKind {
    kind: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{CargoPackage, WorkspaceSources};

    #[test]
    fn package_closure_retains_local_and_registry_dependencies() {
        let root = PathBuf::from("workspace");
        let root_manifest = root.join("crates/root/Cargo.toml");
        let local_manifest = root.join("crates/local/Cargo.toml");

        let sources = WorkspaceSources {
            packages: [
                (
                    "root".to_owned(),
                    CargoPackage {
                        id: "root".to_owned(),
                        name: "root".to_owned(),
                        manifest_path: root_manifest,
                        source: None,
                        checksum: None,
                    },
                ),
                (
                    "local".to_owned(),
                    CargoPackage {
                        id: "local".to_owned(),
                        name: "local".to_owned(),
                        manifest_path: local_manifest,
                        source: None,
                        checksum: None,
                    },
                ),
                (
                    "registry".to_owned(),
                    CargoPackage {
                        id: "registry".to_owned(),
                        name: "registry".to_owned(),
                        manifest_path: PathBuf::from("registry/Cargo.toml"),
                        source: Some("registry".to_owned()),
                        checksum: Some("checksum".to_owned()),
                    },
                ),
            ]
            .into_iter()
            .collect(),
            dependencies: [
                (
                    "root".to_owned(),
                    vec!["local".to_owned(), "registry".to_owned()],
                ),
                ("local".to_owned(), Vec::new()),
                ("registry".to_owned(), Vec::new()),
            ]
            .into_iter()
            .collect(),
        };

        let packages = sources
            .resolved_packages(&["root"])
            .unwrap_or_else(|error| panic!("package closure must resolve: {error}"));

        assert_eq!(
            packages
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            ["local", "registry", "root"]
        );
    }
}
