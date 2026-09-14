use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use bray_base::{lowercase_hex, sha256_file};
use serde::{Deserialize, Serialize};

use crate::process::output_detail;
use crate::workspace;

const USAGE: &str = "usage: cargo llvm <fetch | validate [--root <directory>] | host>";
const MANIFEST: &str = include_str!("../../../toolchains/llvm.json");
const TOOLCHAIN_DIRECTORY: &str = "toolchains/llvm";
const ACTIVE_DIRECTORY: &str = "active";
const DOWNLOAD_DIRECTORY: &str = "downloads";
const MARKER_FILE: &str = "bray-llvm-toolchain.json";
const BRAY_LLVM_PREFIX: &str = "BRAY_LLVM_PREFIX";
const LLVM_SYS_PREFIX: &str = "LLVM_SYS_221_PREFIX";

/// Runs one LLVM toolchain provisioning command.
pub fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("fetch") => reject_trailing(arguments).and_then(|()| fetch()),
        Some("validate") => validate_command(arguments),
        Some("host") => reject_trailing(arguments).and_then(|()| print_host()),
        _ => Err(ToolchainError::Usage),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn fetch() -> Result<(), ToolchainError> {
    let manifest = manifest()?;
    let host = rustc_host()?;
    let package = manifest.package(&host)?;
    let workspace_root = workspace::root().map_err(ToolchainError::Workspace)?;
    let root = workspace_root.join("target").join(TOOLCHAIN_DIRECTORY);
    let active = root.join(ACTIVE_DIRECTORY);

    if validate_root(&active, &manifest.version)
        .and_then(|()| validate_marker(&active, &manifest.version, package, &manifest.source))
        .is_ok()
    {
        println!("{}", active.display());

        return Ok(());
    }

    fs::create_dir_all(root.join(DOWNLOAD_DIRECTORY))
        .map_err(|error| ToolchainError::io("create", &root, error))?;

    let archive = root.join(DOWNLOAD_DIRECTORY).join(&package.archive);

    let source_archive = root.join(DOWNLOAD_DIRECTORY).join(&manifest.source.archive);

    acquire_archive(&package.url, package.size, &package.sha256, &archive)?;

    acquire_archive(
        &manifest.source.url,
        manifest.source.size,
        &manifest.source.sha256,
        &source_archive,
    )?;

    install_archive(
        &root,
        &archive,
        &source_archive,
        &manifest.version,
        package,
        &manifest.source,
    )?;

    println!("{}", active.display());

    Ok(())
}

fn validate_command(mut arguments: impl Iterator<Item = String>) -> Result<(), ToolchainError> {
    let root = match arguments.next().as_deref() {
        None => toolchain_directory()?.join(ACTIVE_DIRECTORY),
        Some("--root") => {
            let path = arguments.next().ok_or(ToolchainError::Usage)?;

            reject_trailing(arguments)?;

            PathBuf::from(path)
        }
        Some(_) => return Err(ToolchainError::Usage),
    };

    let manifest = manifest()?;

    validate_root(&root, &manifest.version)?;

    println!("{}", root.display());

    Ok(())
}

fn print_host() -> Result<(), ToolchainError> {
    let manifest = manifest()?;
    let host = rustc_host()?;

    let _ = manifest.package(&host)?;

    println!("{host}");

    Ok(())
}

fn manifest() -> Result<ToolchainManifest, ToolchainError> {
    let manifest: ToolchainManifest =
        serde_json::from_str(MANIFEST).map_err(ToolchainError::Manifest)?;

    manifest.validate()?;

    Ok(manifest)
}

fn toolchain_directory() -> Result<PathBuf, ToolchainError> {
    let root = workspace::root().map_err(ToolchainError::Workspace)?;

    Ok(root.join("target").join(TOOLCHAIN_DIRECTORY))
}

/// Returns a tool path inside the provisioned LLVM installation.
pub fn tool_path(root: &Path, name: &str) -> PathBuf {
    let configured =
        std::env::var_os(BRAY_LLVM_PREFIX).or_else(|| std::env::var_os(LLVM_SYS_PREFIX));

    tool_path_with_prefix(root, name, configured.as_deref())
}

fn tool_path_with_prefix(root: &Path, name: &str, configured: Option<&OsStr>) -> PathBuf {
    let prefix = match configured.map(Path::new) {
        Some(prefix) if prefix.is_absolute() => prefix.to_path_buf(),
        Some(prefix) => root.join(prefix),
        None => root
            .join("target")
            .join(TOOLCHAIN_DIRECTORY)
            .join(ACTIVE_DIRECTORY),
    };

    prefix.join("bin").join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    })
}

fn rustc_host() -> Result<String, ToolchainError> {
    let output = Command::new("rustc").arg("-vV").output().map_err(|error| {
        ToolchainError::ProcessStart {
            program: "rustc",
            error,
        }
    })?;

    if !output.status.success() {
        return Err(ToolchainError::ProcessFailed {
            program: "rustc",
            detail: output_detail(&output),
        });
    }

    parse_rustc_host(&String::from_utf8_lossy(&output.stdout))
        .ok_or(ToolchainError::MissingRustcHost)
}

fn acquire_archive(
    url: &str,
    size: u64,
    sha256: &str,
    archive: &Path,
) -> Result<(), ToolchainError> {
    if archive.is_file() && verify_archive(size, sha256, archive).is_ok() {
        return Ok(());
    }

    if archive.exists() {
        fs::remove_file(archive).map_err(|error| ToolchainError::io("remove", archive, error))?;
    }

    let Some(file_name) = archive.file_name() else {
        return Err(ToolchainError::InvalidManifest);
    };

    let partial = archive.with_file_name(format!("{}.partial", file_name.to_string_lossy()));

    if partial.exists() {
        fs::remove_file(&partial).map_err(|error| ToolchainError::io("remove", &partial, error))?;
    }

    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--retry",
            "3",
            "--proto",
            "=https",
            "--output",
        ])
        .arg(&partial)
        .arg(url)
        .output()
        .map_err(|error| ToolchainError::ProcessStart {
            program: "curl",
            error,
        })?;

    if !output.status.success() {
        return Err(ToolchainError::ProcessFailed {
            program: "curl",
            detail: output_detail(&output),
        });
    }

    verify_archive(size, sha256, &partial)?;

    fs::rename(&partial, archive).map_err(|error| ToolchainError::rename(&partial, archive, error))
}

fn verify_archive(size: u64, sha256: &str, archive: &Path) -> Result<(), ToolchainError> {
    let metadata =
        fs::metadata(archive).map_err(|error| ToolchainError::io("read", archive, error))?;

    if metadata.len() != size {
        return Err(ToolchainError::ArchiveSize {
            expected: size,
            actual: metadata.len(),
        });
    }

    let actual = lowercase_hex(
        &sha256_file(archive).map_err(|error| ToolchainError::io("read", archive, error))?,
    );

    if actual != sha256 {
        return Err(ToolchainError::ArchiveDigest {
            expected: sha256.to_owned(),
            actual,
        });
    }

    Ok(())
}

fn install_archive(
    root: &Path,
    archive: &Path,
    source_archive: &Path,
    version: &str,
    package: &ToolchainPackage,
    source: &ToolchainSource,
) -> Result<(), ToolchainError> {
    let active = root.join(ACTIVE_DIRECTORY);
    let staging = root.join(format!("active-{}.partial", std::process::id()));
    let backup = root.join("active.previous");

    remove_owned_directory(root, &staging)?;

    fs::create_dir_all(&staging).map_err(|error| ToolchainError::io("create", &staging, error))?;

    if let Err(error) = prepare_staging(&staging, archive, source_archive, version, package, source)
    {
        return cleanup_after_failure(root, &staging, error);
    }

    if let Err(error) = remove_owned_directory(root, &backup) {
        return cleanup_after_failure(root, &staging, error);
    }

    if active.exists()
        && let Err(error) = fs::rename(&active, &backup)
    {
        let error = ToolchainError::rename(&active, &backup, error);

        return cleanup_after_failure(root, &staging, error);
    }

    if let Err(error) = fs::rename(&staging, &active) {
        let publication = ToolchainError::rename(&staging, &active, error);

        if backup.exists()
            && let Err(error) = fs::rename(&backup, &active)
        {
            let rollback = ToolchainError::rename(&backup, &active, error);

            let error = ToolchainError::PublicationRollback {
                publication: Box::new(publication),
                rollback: Box::new(rollback),
            };

            return cleanup_after_failure(root, &staging, error);
        }

        return cleanup_after_failure(root, &staging, publication);
    }

    remove_owned_directory(root, &backup)
}

fn prepare_staging(
    staging: &Path,
    archive: &Path,
    source_archive: &Path,
    version: &str,
    package: &ToolchainPackage,
    source: &ToolchainSource,
) -> Result<(), ToolchainError> {
    let output = Command::new("tar")
        .arg("-xJf")
        .arg(archive)
        .arg("--directory")
        .arg(staging)
        .args(["--strip-components", "1"])
        .output()
        .map_err(|error| ToolchainError::ProcessStart {
            program: "tar",
            error,
        })?;

    if !output.status.success() {
        return Err(ToolchainError::ProcessFailed {
            program: "tar",
            detail: output_detail(&output),
        });
    }

    validate_root(staging, version)?;

    let identity = crate::instrumentation::identity(version, &source.sha256);

    crate::instrumentation::install(staging, source_archive, version, &identity)
        .map_err(ToolchainError::Instrumentation)?;

    write_marker(staging, version, package, source)
}

fn cleanup_after_failure(
    root: &Path,
    staging: &Path,
    installation: ToolchainError,
) -> Result<(), ToolchainError> {
    if let Err(cleanup) = remove_owned_directory(root, staging) {
        return Err(ToolchainError::CleanupAfterFailure {
            installation: Box::new(installation),
            cleanup: Box::new(cleanup),
        });
    }

    Err(installation)
}

fn validate_root(root: &Path, expected_version: &str) -> Result<(), ToolchainError> {
    let config = llvm_config(root);

    if !config.is_file() {
        return Err(ToolchainError::MissingLlvmConfig(config));
    }

    let version = process_stdout(Command::new(&config).arg("--version"), "llvm-config")?;

    if version.trim() != expected_version {
        return Err(ToolchainError::Version {
            expected: expected_version.to_owned(),
            actual: version.trim().to_owned(),
        });
    }

    let include = root.join("include").join("llvm-c").join("Core.h");

    if !include.is_file() {
        return Err(ToolchainError::MissingDevelopmentFile(include));
    }

    let _ = process_stdout(
        Command::new(&config).args(["--link-static", "--libs", "core", "target"]),
        "llvm-config",
    )?;

    Ok(())
}

fn llvm_config(root: &Path) -> PathBuf {
    let executable = if cfg!(windows) {
        "llvm-config.exe"
    } else {
        "llvm-config"
    };

    root.join("bin").join(executable)
}

fn process_stdout(command: &mut Command, program: &'static str) -> Result<String, ToolchainError> {
    let output = command
        .output()
        .map_err(|error| ToolchainError::ProcessStart { program, error })?;

    if !output.status.success() {
        return Err(ToolchainError::ProcessFailed {
            program,
            detail: output_detail(&output),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn write_marker(
    root: &Path,
    version: &str,
    package: &ToolchainPackage,
    source: &ToolchainSource,
) -> Result<(), ToolchainError> {
    let marker = ToolchainMarker {
        version: version.to_owned(),
        identity: crate::instrumentation::identity(version, &source.sha256),
        host: package.host.clone(),
        archive: package.archive.clone(),
        sha256: package.sha256.clone(),
        source_sha256: source.sha256.clone(),
        instrumentation_digest: crate::instrumentation::digest(),
    };

    let bytes = serde_json::to_vec_pretty(&marker).map_err(ToolchainError::Marker)?;
    let path = root.join(MARKER_FILE);

    fs::write(&path, bytes).map_err(|error| ToolchainError::io("write", &path, error))
}

fn validate_marker(
    root: &Path,
    version: &str,
    package: &ToolchainPackage,
    source: &ToolchainSource,
) -> Result<(), ToolchainError> {
    let path = root.join(MARKER_FILE);
    let bytes = fs::read(&path).map_err(|error| ToolchainError::io("read", &path, error))?;

    let marker: ToolchainMarker = serde_json::from_slice(&bytes).map_err(ToolchainError::Marker)?;

    if marker.version != version
        || marker.identity != crate::instrumentation::identity(version, &source.sha256)
        || marker.host != package.host
        || marker.archive != package.archive
        || marker.sha256 != package.sha256
        || marker.source_sha256 != source.sha256
        || marker.instrumentation_digest != crate::instrumentation::digest()
    {
        return Err(ToolchainError::MarkerMismatch);
    }

    Ok(())
}

fn remove_owned_directory(root: &Path, path: &Path) -> Result<(), ToolchainError> {
    if path.parent() != Some(root) {
        return Err(ToolchainError::UnsafeCleanup(path.to_path_buf()));
    }

    if !path.exists() {
        return Ok(());
    }

    fs::remove_dir_all(path).map_err(|error| ToolchainError::io("remove", path, error))
}

fn reject_trailing(mut arguments: impl Iterator<Item = String>) -> Result<(), ToolchainError> {
    match arguments.next() {
        Some(argument) => Err(ToolchainError::UnexpectedArgument(argument)),
        None => Ok(()),
    }
}

fn parse_rustc_host(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
}

#[derive(Deserialize)]
struct ToolchainManifest {
    version: String,
    source: ToolchainSource,
    hosts: Vec<ToolchainPackage>,
}

impl ToolchainManifest {
    fn validate(&self) -> Result<(), ToolchainError> {
        if self.version.is_empty() || self.hosts.is_empty() {
            return Err(ToolchainError::InvalidManifest);
        }

        validate_archive(
            &self.source.archive,
            &self.source.url,
            self.source.size,
            &self.source.sha256,
            &self.version,
        )?;

        let mut hosts = BTreeSet::new();

        for package in &self.hosts {
            if package.host.is_empty() || !hosts.insert(&package.host) {
                return Err(ToolchainError::InvalidManifest);
            }

            validate_archive(
                &package.archive,
                &package.url,
                package.size,
                &package.sha256,
                &self.version,
            )?;
        }

        Ok(())
    }

    fn package(&self, host: &str) -> Result<&ToolchainPackage, ToolchainError> {
        self.hosts
            .iter()
            .find(|package| package.host == host)
            .ok_or_else(|| ToolchainError::UnsupportedHost(host.to_owned()))
    }
}

fn validate_archive(
    archive: &str,
    url: &str,
    size: u64,
    sha256: &str,
    version: &str,
) -> Result<(), ToolchainError> {
    let unsafe_archive =
        archive == "." || archive == ".." || archive.contains('/') || archive.contains('\\');

    if archive.is_empty()
        || unsafe_archive
        || url.is_empty()
        || size == 0
        || sha256.len() != 64
        || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !archive.contains(version)
        || !url.contains(version)
    {
        return Err(ToolchainError::InvalidManifest);
    }

    Ok(())
}

#[derive(Deserialize)]
struct ToolchainSource {
    archive: String,
    url: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct ToolchainPackage {
    host: String,
    archive: String,
    url: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
struct ToolchainMarker {
    version: String,
    identity: String,
    host: String,
    archive: String,
    sha256: String,
    source_sha256: String,
    instrumentation_digest: String,
}

#[derive(Debug)]
enum ToolchainError {
    Usage,
    UnexpectedArgument(String),
    Workspace(String),
    Manifest(serde_json::Error),
    Marker(serde_json::Error),
    MarkerMismatch,
    InvalidManifest,
    UnsupportedHost(String),
    MissingRustcHost,
    ProcessStart {
        program: &'static str,
        error: io::Error,
    },
    ProcessFailed {
        program: &'static str,
        detail: String,
    },
    MissingLlvmConfig(PathBuf),
    MissingDevelopmentFile(PathBuf),
    Version {
        expected: String,
        actual: String,
    },
    ArchiveSize {
        expected: u64,
        actual: u64,
    },
    ArchiveDigest {
        expected: String,
        actual: String,
    },
    Io {
        action: &'static str,
        path: PathBuf,
        error: io::Error,
    },
    Rename {
        source: PathBuf,
        destination: PathBuf,
        error: io::Error,
    },
    UnsafeCleanup(PathBuf),
    CleanupAfterFailure {
        installation: Box<ToolchainError>,
        cleanup: Box<ToolchainError>,
    },
    PublicationRollback {
        publication: Box<ToolchainError>,
        rollback: Box<ToolchainError>,
    },
    Instrumentation(crate::instrumentation::InstrumentationError),
}

impl ToolchainError {
    fn io(action: &'static str, path: &Path, error: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_path_buf(),
            error,
        }
    }

    fn rename(source: &Path, destination: &Path, error: io::Error) -> Self {
        Self::Rename {
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            error,
        }
    }
}

impl fmt::Display for ToolchainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str(USAGE),
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "unexpected argument: {argument}")
            }
            Self::Workspace(error) => write!(formatter, "failed to locate workspace: {error}"),
            Self::Manifest(error) => write!(formatter, "invalid LLVM manifest JSON: {error}"),
            Self::Marker(error) => write!(formatter, "invalid LLVM marker JSON: {error}"),
            Self::MarkerMismatch => {
                formatter.write_str(
                    "active LLVM toolchain does not match the pinned package and linker instrumentation contract",
                )
            }
            Self::InvalidManifest => formatter.write_str("LLVM manifest is incomplete or invalid"),
            Self::UnsupportedHost(host) => {
                write!(formatter, "LLVM {host} package is not provisioned by Bray")
            }
            Self::MissingRustcHost => formatter.write_str("rustc did not report its host"),
            Self::ProcessStart { program, error } => {
                write!(formatter, "failed to start {program}: {error}")
            }
            Self::ProcessFailed { program, detail } => {
                write!(formatter, "{program} failed: {detail}")
            }
            Self::MissingLlvmConfig(path) => {
                write!(
                    formatter,
                    "LLVM development tool is missing: {}",
                    path.display()
                )
            }
            Self::MissingDevelopmentFile(path) => {
                write!(
                    formatter,
                    "LLVM development file is missing: {}",
                    path.display()
                )
            }
            Self::Version { expected, actual } => {
                write!(formatter, "expected LLVM {expected}, found {actual}")
            }
            Self::ArchiveSize { expected, actual } => {
                write!(
                    formatter,
                    "expected LLVM archive size {expected}, found {actual}"
                )
            }
            Self::ArchiveDigest { expected, actual } => {
                write!(
                    formatter,
                    "expected LLVM SHA-256 {expected}, found {actual}"
                )
            }
            Self::Io {
                action,
                path,
                error,
            } => write!(formatter, "failed to {action} {}: {error}", path.display()),
            Self::Rename {
                source,
                destination,
                error,
            } => write!(
                formatter,
                "failed to rename {} to {}: {error}",
                source.display(),
                destination.display()
            ),
            Self::UnsafeCleanup(path) => {
                write!(
                    formatter,
                    "refusing to remove unowned path {}",
                    path.display()
                )
            }
            Self::CleanupAfterFailure {
                installation,
                cleanup,
            } => write!(
                formatter,
                "LLVM installation failed ({installation}) and staging cleanup failed ({cleanup})"
            ),
            Self::PublicationRollback {
                publication,
                rollback,
            } => write!(
                formatter,
                "LLVM publication failed ({publication}) and rollback failed ({rollback})"
            ),
            Self::Instrumentation(error) => write!(formatter, "{error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        ToolchainError, cleanup_after_failure, manifest, parse_rustc_host, tool_path_with_prefix,
        validate_marker, write_marker,
    };

    #[test]
    fn tool_paths_use_default_relative_and_absolute_installation_prefixes() {
        let root = Path::new("workspace");

        let executable = if cfg!(windows) {
            "lld-link.exe"
        } else {
            "lld-link"
        };

        assert_eq!(
            tool_path_with_prefix(root, "lld-link", None),
            root.join("target/toolchains/llvm/active/bin")
                .join(executable)
        );

        assert_eq!(
            tool_path_with_prefix(
                root,
                "lld-link",
                Some(std::ffi::OsStr::new("configured/llvm")),
            ),
            root.join("configured/llvm/bin").join(executable)
        );

        let absolute = if cfg!(windows) {
            Path::new(r"C:\llvm")
        } else {
            Path::new("/llvm")
        };

        assert_eq!(
            tool_path_with_prefix(root, "lld-link", Some(absolute.as_os_str())),
            absolute.join("bin").join(executable)
        );
    }

    #[test]
    fn manifest_covers_supported_release_hosts() {
        let Ok(manifest) = manifest() else {
            panic!("checked-in LLVM manifest must validate");
        };

        assert_eq!(manifest.version, "22.1.8");

        assert_eq!(
            manifest
                .hosts
                .iter()
                .map(|package| package.host.as_str())
                .collect::<Vec<_>>(),
            [
                "aarch64-apple-darwin",
                "aarch64-pc-windows-msvc",
                "aarch64-unknown-linux-gnu",
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
            ]
        );
    }

    #[test]
    fn rustc_host_parser_requires_the_named_field() {
        assert_eq!(
            parse_rustc_host("rustc 1.97.0\nhost: x86_64-pc-windows-msvc\n"),
            Some("x86_64-pc-windows-msvc".to_owned())
        );

        assert_eq!(parse_rustc_host("rustc 1.97.0\n"), None);
    }

    #[test]
    fn manifest_rejects_archive_path_components() {
        let Ok(mut manifest) = manifest() else {
            panic!("checked-in LLVM manifest must validate");
        };

        manifest.hosts[0].archive = "../LLVM-22.1.8.tar.xz".to_owned();

        assert!(matches!(
            manifest.validate(),
            Err(ToolchainError::InvalidManifest)
        ));
    }

    #[test]
    fn active_markers_must_match_the_selected_package() {
        let Ok(manifest) = manifest() else {
            panic!("checked-in LLVM manifest must validate");
        };

        let Some(package) = manifest.hosts.first() else {
            panic!("manifest must contain a test package");
        };

        let Ok(directory) = tempfile::tempdir() else {
            panic!("temporary directory must be available");
        };

        if let Err(error) = write_marker(directory.path(), "0.0.0", package, &manifest.source) {
            panic!("test marker must be written: {error}");
        }

        assert!(matches!(
            validate_marker(
                directory.path(),
                &manifest.version,
                package,
                &manifest.source,
            ),
            Err(ToolchainError::MarkerMismatch)
        ));
    }

    #[test]
    fn failed_installations_remove_owned_staging_directories() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("temporary directory must be available");
        };

        let staging = directory.path().join("active-1.partial");

        if let Err(error) = fs::create_dir(&staging) {
            panic!("test staging directory must be created: {error}");
        }

        let result =
            cleanup_after_failure(directory.path(), &staging, ToolchainError::InvalidManifest);

        assert!(matches!(result, Err(ToolchainError::InvalidManifest)));
        assert!(!staging.exists());
    }
}
