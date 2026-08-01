use std::ffi::OsString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_compilation::{
    CompilationOptions, CompilationRequest, ProductEmissionInputs, SelectedTarget, WorkerBudget,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, OutputSink,
    ReplacementPolicy, RequestedArtifact, RequestedArtifactDestination,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    PackageInterfaceIdentity,
};
use bray_project::{ProjectGraph, ProjectProduct, load_standard_library_project_graph};
use bray_standard_library::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY, STANDARD_LIBRARY_MANIFEST_FILE_NAME,
    StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
    StandardLibraryTargetArtifacts, decode_standard_library_manifest,
    encode_standard_library_manifest,
};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::{TargetIdentity, TargetOutputDescription, TargetOutputKind};
use bray_tooling::{load_llvm_compilation, native_linker, source_inputs_from_file_arguments};

use crate::workspace;

const USAGE: &str = "usage: cargo xtask standard-library <build --output <directory> [--source <directory>] | verify>";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("build") => BuildOptions::parse(arguments).and_then(|options| build(&options)),
        Some("verify") => verify(arguments).map(|()| PathBuf::new()),
        _ => Err(BuildError::Usage),
    };

    match result {
        Ok(manifest) if !manifest.as_os_str().is_empty() => {
            println!("{}", manifest.display());

            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{USAGE}");

            ExitCode::FAILURE
        }
    }
}

struct BuildOptions {
    source: PathBuf,
    output: PathBuf,
}

impl BuildOptions {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, BuildError> {
        let mut source = None;
        let mut output = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--source" if source.is_none() => {
                    source = Some(path_argument(&mut arguments, "--source")?);
                }
                "--output" if output.is_none() => {
                    output = Some(path_argument(&mut arguments, "--output")?);
                }
                _ => return Err(BuildError::UnexpectedArgument(argument)),
            }
        }

        let output = output.ok_or(BuildError::MissingOutput)?;

        let source = match source {
            Some(source) => source,
            None => workspace::root()
                .map_err(BuildError::Workspace)?
                .join("standard-library"),
        };

        Ok(Self { source, output })
    }
}

fn path_argument(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<PathBuf, BuildError> {
    arguments
        .next()
        .map(PathBuf::from)
        .ok_or(BuildError::MissingValue(option))
}

fn verify(mut arguments: impl Iterator<Item = String>) -> Result<(), BuildError> {
    if let Some(argument) = arguments.next() {
        return Err(BuildError::UnexpectedArgument(argument));
    }

    let directory = tempfile::Builder::new()
        .prefix("bray-standard-library-verification-")
        .tempdir()
        .map_err(BuildError::TemporaryDirectory)?;

    super::conformance::verify(directory.path())
}

pub(super) fn compare_bundles(first: &Path, second: &Path) -> Result<(), BuildError> {
    let first_manifest = read_manifest(first)?;
    let second_manifest = read_manifest(second)?;

    if first_manifest != second_manifest {
        return Err(BuildError::NonReproducibleManifest);
    }

    compare_artifact(first, second, first_manifest.interface())?;

    for artifact in first_manifest
        .targets()
        .iter()
        .flat_map(StandardLibraryTargetArtifacts::artifacts)
    {
        compare_artifact(first, second, artifact)?;
    }

    Ok(())
}

pub(super) fn read_manifest(root: &Path) -> Result<StandardLibraryBundleManifest, BuildError> {
    let path = root.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);
    let bytes = fs::read(&path).map_err(|error| BuildError::read(&path, error))?;

    decode_standard_library_manifest(&bytes)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))
}

fn compare_artifact(
    first: &Path,
    second: &Path,
    artifact: &StandardLibraryArtifact,
) -> Result<(), BuildError> {
    let first_path = artifact.beneath(first);
    let second_path = artifact.beneath(second);

    let first_bytes =
        fs::read(&first_path).map_err(|error| BuildError::read(&first_path, error))?;

    let second_bytes =
        fs::read(&second_path).map_err(|error| BuildError::read(&second_path, error))?;

    if first_bytes != second_bytes {
        return Err(BuildError::NonReproducibleArtifact(
            artifact.path().to_owned(),
        ));
    }

    Ok(())
}

fn build(options: &BuildOptions) -> Result<PathBuf, BuildError> {
    let graph = load_standard_library_project_graph(&options.source)
        .map_err(|error| BuildError::Project(format!("{error:?}")))?;

    let product = standard_library_product(&graph)?;
    let parent = options.output.parent().unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent).map_err(|error| BuildError::write(parent, error))?;

    let _publication = PublicationLock::acquire(&options.output)?;

    if options.output.exists() {
        return Err(BuildError::OutputExists(options.output.clone()));
    }

    let staging = tempfile::Builder::new()
        .prefix("bray-standard-library-")
        .tempdir_in(parent)
        .map_err(BuildError::TemporaryDirectory)?;

    let bundle = staging.path().join("bundle");

    fs::create_dir(&bundle).map_err(|error| BuildError::write(&bundle, error))?;

    let work = staging.path().join("work");
    let manifest = build_bundle(product, &options.source, &work, &bundle)?;
    let manifest_path = bundle.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let manifest_bytes = encode_standard_library_manifest(&manifest)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

    fs::write(&manifest_path, manifest_bytes)
        .map_err(|error| BuildError::write(&manifest_path, error))?;

    fs::rename(&bundle, &options.output)
        .map_err(|error| BuildError::publish(&bundle, &options.output, error))?;

    Ok(options.output.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME))
}

struct PublicationLock {
    path: PathBuf,
}

impl PublicationLock {
    fn acquire(output: &Path) -> Result<Self, BuildError> {
        let file_name = output
            .file_name()
            .ok_or_else(|| BuildError::InvalidArtifactPath(output.to_path_buf()))?;

        let mut lock_name = OsString::from(file_name);

        lock_name.push(".lock");

        let path = output.with_file_name(lock_name);

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(Self { path }),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                Err(BuildError::PublicationInProgress(path))
            }
            Err(error) => Err(BuildError::write(&path, error)),
        }
    }
}

impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn build_bundle(
    product: &ProjectProduct,
    workspace_root: &Path,
    work: &Path,
    bundle: &Path,
) -> Result<StandardLibraryBundleManifest, BuildError> {
    let source_paths: Vec<_> = product
        .sources()
        .iter()
        .map(|source| source.beneath(workspace_root))
        .collect();

    let mut interface = None;
    let mut targets = Vec::new();

    for target in product.targets() {
        let built = build_target(product, &source_paths, target, work)?;

        let BuiltTarget {
            selected,
            interface_bytes,
            archive_path,
            archive_bytes,
        } = built;

        match interface.as_ref() {
            Some((expected, _)) if expected != &interface_bytes => {
                return Err(BuildError::TargetDependentInterface(target.clone()));
            }
            Some(_) => {}
            None => {
                let path = "interfaces/std.brayi";

                write_bundle_artifact(bundle, path, &interface_bytes)?;

                let artifact = StandardLibraryArtifact::try_for_bytes(
                    StandardLibraryArtifactKind::PackageInterface,
                    path,
                    &interface_bytes,
                )
                .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

                interface = Some((interface_bytes, artifact));
            }
        }

        let abi = selected.runtime_abi();
        let abi_path = format!("{}.{}", abi.major(), abi.minor());

        let file_name = archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| BuildError::InvalidArtifactPath(archive_path.clone()))?;

        let portable_path = format!("targets/{}/{abi_path}/{file_name}", target.as_str());

        write_bundle_artifact(bundle, &portable_path, &archive_bytes)?;

        let archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            portable_path,
            &archive_bytes,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let target = StandardLibraryTargetArtifacts::try_new(target.clone(), abi, [archive])
            .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        targets.push(target);
    }

    let (_, interface) = interface.ok_or(BuildError::MissingInterface)?;

    StandardLibraryBundleManifest::try_new(interface, targets)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))
}

struct BuiltTarget {
    selected: SelectedTarget,
    interface_bytes: Vec<u8>,
    archive_path: PathBuf,
    archive_bytes: Vec<u8>,
}

fn build_target(
    product: &ProjectProduct,
    source_paths: &[PathBuf],
    target: &TargetIdentity,
    work: &Path,
) -> Result<BuiltTarget, BuildError> {
    let selected = SelectedTarget::for_identity(target)
        .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))?;

    let native = selected
        .native_target()
        .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))?;

    let output = work.join(target.as_str());

    fs::create_dir_all(&output).map_err(|error| BuildError::write(&output, error))?;

    let sources = source_inputs_from_file_arguments(source_paths.iter().cloned())
        .map_err(|error| BuildError::Source(format!("{error:?}")))?;

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Library,
        selected.clone(),
    );

    let request =
        CompilationRequest::with_options(product.identity().package().clone(), sources, options)
            .with_standard_library_source_authority()
            .with_package_interface_export(interface_export_request(product.identity())?);

    let compilation = load_llvm_compilation(request).ok_or(BuildError::CompilerUnavailable)?;

    let linker =
        native_linker(native).ok_or_else(|| BuildError::LinkerUnavailable(target.clone()))?;

    let native_facts = compilation
        .native_product_facts(product.identity().clone(), None, [], Some(&linker))
        .map_err(|error| BuildError::CompilationFailed {
            target: target.clone(),
            detail: format!(
                "{error:?}; diagnostics={:?}",
                compilation.check_diagnostics()
            ),
        })?;

    let output_description = TargetOutputDescription::for_native(
        native,
        [
            TargetOutputKind::PackageInterface,
            TargetOutputKind::StaticLibrary,
        ],
    );

    let request = EmissionRequest::try_new(
        product.identity().clone(),
        ProductKind::Library,
        None,
        target.clone(),
        RequestedArtifactDestination::FilesystemDirectory(output),
        [
            RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            ),
            RequestedArtifact::new(ArtifactKind::StaticLibrary, ArtifactRequirement::Required),
        ],
        ReplacementPolicy::RequireAbsent,
    )
    .map_err(|error| BuildError::EmissionRequest(format!("{error:?}")))?;

    let inputs =
        ProductEmissionInputs::new(&output_description).with_native_product(&native_facts, &linker);

    let outcome = compilation
        .emit_product(request, inputs)
        .map_err(|error| BuildError::Emission(format!("{:?}", error.kind())))?;

    if !matches!(outcome.status(), EmissionStatus::Complete) {
        return Err(BuildError::CompilationFailed {
            target: target.clone(),
            detail: format!(
                "{:?}; diagnostics={:?}",
                outcome.status(),
                outcome.diagnostics()
            ),
        });
    }

    let interface_path = emitted_path(&outcome, ArtifactKind::PackageInterface)?;
    let archive_path = emitted_path(&outcome, ArtifactKind::StaticLibrary)?;

    let interface_bytes =
        fs::read(&interface_path).map_err(|error| BuildError::read(&interface_path, error))?;

    let archive_bytes =
        fs::read(&archive_path).map_err(|error| BuildError::read(&archive_path, error))?;

    Ok(BuiltTarget {
        selected,
        interface_bytes,
        archive_path,
        archive_bytes,
    })
}

fn emitted_path(
    outcome: &bray_emitter::EmissionOutcome,
    kind: ArtifactKind,
) -> Result<PathBuf, BuildError> {
    outcome
        .artifacts()
        .artifacts()
        .iter()
        .find(|artifact| artifact.id().kind() == kind)
        .and_then(|artifact| match artifact.sink() {
            OutputSink::Filesystem(path) => Some(path.clone()),
            OutputSink::Memory { .. } | OutputSink::Stream(_) => None,
        })
        .ok_or(BuildError::MissingEmittedArtifact(kind))
}

fn interface_export_request(
    product: &ProductIdentity,
) -> Result<bray_compilation::PackageInterfaceExportRequest, BuildError> {
    let product_identity =
        InterfaceProductIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY)
            .ok_or(BuildError::InvalidIdentity)?;

    let identity = PackageInterfaceIdentity::try_new(
        product.package().clone(),
        product_identity,
        InterfaceProductKind::Library,
        PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
    )
    .ok_or(BuildError::InvalidIdentity)?;

    Ok(bray_compilation::PackageInterfaceExportRequest::new(
        identity,
        InterfaceLanguageRevision::new(0),
    ))
}

fn standard_library_product(graph: &ProjectGraph) -> Result<&ProjectProduct, BuildError> {
    let package = PackageIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
        .ok_or(BuildError::InvalidIdentity)?;

    graph
        .package(&package)
        .and_then(|package| {
            package.products().iter().find(|product| {
                product.identity().name() == PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY
            })
        })
        .ok_or(BuildError::MissingProduct)
}

pub(super) fn write_bundle_artifact(
    bundle: &Path,
    path: &str,
    bytes: &[u8],
) -> Result<(), BuildError> {
    let destination = path
        .split('/')
        .fold(bundle.to_path_buf(), |path, component| path.join(component));

    let parent = destination
        .parent()
        .ok_or_else(|| BuildError::InvalidArtifactPath(destination.clone()))?;

    fs::create_dir_all(parent).map_err(|error| BuildError::write(parent, error))?;

    fs::write(&destination, bytes).map_err(|error| BuildError::write(&destination, error))
}

#[derive(Debug)]
pub(super) enum BuildError {
    Usage,
    UnexpectedArgument(String),
    MissingValue(&'static str),
    MissingOutput,
    Workspace(String),
    OutputExists(PathBuf),
    PublicationInProgress(PathBuf),
    Project(String),
    Source(String),
    TemporaryDirectory(std::io::Error),
    UnsupportedTarget(TargetIdentity),
    CompilerUnavailable,
    LinkerUnavailable(TargetIdentity),
    CompilationFailed {
        target: TargetIdentity,
        detail: String,
    },
    EmissionRequest(String),
    Emission(String),
    MissingEmittedArtifact(ArtifactKind),
    TargetDependentInterface(TargetIdentity),
    InvalidArtifactPath(PathBuf),
    InvalidIdentity,
    MissingProduct,
    MissingInterface,
    NonReproducibleManifest,
    NonReproducibleArtifact(String),
    Conformance {
        check: &'static str,
        detail: String,
    },
    Manifest(String),
    Io {
        action: &'static str,
        path: PathBuf,
        destination: Option<PathBuf>,
        source: std::io::Error,
    },
}

impl BuildError {
    pub(super) fn conformance(check: &'static str, detail: impl Into<String>) -> Self {
        Self::Conformance {
            check,
            detail: detail.into(),
        }
    }

    pub(super) fn read(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "read",
            path: path.to_path_buf(),
            destination: None,
            source,
        }
    }

    fn write(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "write",
            path: path.to_path_buf(),
            destination: None,
            source,
        }
    }

    fn publish(path: &Path, destination: &Path, source: std::io::Error) -> Self {
        Self::Io {
            action: "publish",
            path: path.to_path_buf(),
            destination: Some(destination.to_path_buf()),
            source,
        }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str("invalid standard-library command"),
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "unexpected argument: {argument}")
            }
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::MissingOutput => formatter.write_str("missing --output"),
            Self::Workspace(error) => formatter.write_str(error),
            Self::OutputExists(path) => {
                write!(formatter, "output already exists: {}", path.display())
            }
            Self::PublicationInProgress(path) => {
                write!(
                    formatter,
                    "standard library publication is already in progress: {}",
                    path.display()
                )
            }
            Self::Project(error) => {
                write!(formatter, "standard library project is invalid: {error}")
            }
            Self::Source(error) => {
                write!(
                    formatter,
                    "standard library source could not be read: {error}"
                )
            }
            Self::TemporaryDirectory(error) => {
                write!(formatter, "could not create staging directory: {error}")
            }
            Self::UnsupportedTarget(target) => {
                write!(
                    formatter,
                    "unsupported standard library target: {}",
                    target.as_str()
                )
            }
            Self::CompilerUnavailable => {
                formatter.write_str("LLVM compiler backend is unavailable")
            }
            Self::LinkerUnavailable(target) => {
                write!(formatter, "archiver is unavailable for {}", target.as_str())
            }
            Self::CompilationFailed { target, detail } => {
                write!(
                    formatter,
                    "standard library compilation failed for {}: {detail}",
                    target.as_str(),
                )
            }
            Self::EmissionRequest(error) => {
                write!(
                    formatter,
                    "standard library emission request is invalid: {error}"
                )
            }
            Self::Emission(error) => {
                write!(formatter, "standard library emission failed: {error}")
            }
            Self::MissingEmittedArtifact(kind) => {
                write!(formatter, "standard library emission omitted {kind:?}")
            }
            Self::TargetDependentInterface(target) => {
                write!(
                    formatter,
                    "package interface differs for target {}",
                    target.as_str()
                )
            }
            Self::InvalidArtifactPath(path) => {
                write!(formatter, "artifact path is invalid: {}", path.display())
            }
            Self::InvalidIdentity => {
                formatter.write_str("standard library identity contract is invalid")
            }
            Self::MissingProduct => {
                formatter.write_str("standard library product std:library is missing")
            }
            Self::MissingInterface => formatter
                .write_str("standard library has no target from which to build its interface"),
            Self::NonReproducibleManifest => {
                formatter.write_str("repeated standard library builds produced different manifests")
            }
            Self::NonReproducibleArtifact(path) => {
                write!(
                    formatter,
                    "repeated standard library builds produced different bytes for {path}"
                )
            }
            Self::Conformance { check, detail } => {
                write!(
                    formatter,
                    "standard library {check} conformance failed: {detail}"
                )
            }
            Self::Manifest(error) => {
                write!(formatter, "standard library manifest is invalid: {error}")
            }
            Self::Io {
                action,
                path,
                destination,
                source,
            } => match destination {
                Some(destination) => write!(
                    formatter,
                    "failed to {action} {} to {}: {source}",
                    path.display(),
                    destination.display()
                ),
                None => write!(formatter, "failed to {action} {}: {source}", path.display()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{BuildError, BuildOptions};

    #[test]
    fn build_options_require_an_output_and_reject_unknown_arguments() {
        assert!(matches!(
            BuildOptions::parse(std::iter::empty()),
            Err(BuildError::MissingOutput)
        ));

        assert!(matches!(
            BuildOptions::parse(["--unknown".to_owned()].into_iter()),
            Err(BuildError::UnexpectedArgument(argument)) if argument == "--unknown"
        ));
    }

    #[test]
    fn build_options_accept_explicit_source_and_output_roots() {
        let options = BuildOptions::parse(
            [
                "--source".to_owned(),
                "source".to_owned(),
                "--output".to_owned(),
                "output".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap_or_else(|error| panic!("build options must parse: {error}"));

        assert_eq!(options.source, PathBuf::from("source"));
        assert_eq!(options.output, PathBuf::from("output"));
    }
}
