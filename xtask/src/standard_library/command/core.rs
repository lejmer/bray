use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_compilation::{
    BuildConfiguration, CompilationOptions, CompilationRequest, ProductEmissionInputs,
    SelectedTarget, WorkerBudget,
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
use bray_symbols::{PackageIdentity, PackageVersion, ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetIdentity, TargetOutputDescription, TargetOutputKind};
use bray_tooling::{load_llvm_compilation, native_linker, source_inputs_from_file_arguments};

use super::error::BuildError;
use crate::workspace;

const USAGE: &str = "usage: cargo xtask standard-library \
    <build --output <directory> [--source <directory>] [--target <triple>] | verify>";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("build") => BuildOptions::parse(arguments).and_then(BuildOptions::build),
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
    target: Option<NativeTarget>,
}

impl BuildOptions {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, BuildError> {
        let mut source = None;
        let mut output = None;
        let mut target = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--source" if source.is_none() => {
                    source = Some(path_argument(&mut arguments, "--source")?);
                }
                "--output" if output.is_none() => {
                    output = Some(path_argument(&mut arguments, "--output")?);
                }
                "--target" if target.is_none() => {
                    let value = arguments
                        .next()
                        .ok_or(BuildError::MissingValue("--target"))?;

                    target = TargetIdentity::try_new(value.as_str())
                        .and_then(|identity| NativeTarget::for_identity(&identity))
                        .map(Some)
                        .ok_or(BuildError::InvalidTarget(value))?;
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

        Ok(Self {
            source,
            output,
            target,
        })
    }

    fn build(self) -> Result<PathBuf, BuildError> {
        match self.target {
            Some(target) => build_target_bundle_inner(&self.source, &self.output, target),
            None => build(&self.source, &self.output),
        }
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

    crate::standard_library::conformance::verify(directory.path())
}

pub(in crate::standard_library) fn compare_bundles(
    first: &Path,
    second: &Path,
) -> Result<(), BuildError> {
    let first_manifest_bytes = read_manifest_bytes(first)?;
    let second_manifest_bytes = read_manifest_bytes(second)?;

    if first_manifest_bytes != second_manifest_bytes {
        return Err(BuildError::NonReproducibleManifest);
    }

    let first_manifest = decode_manifest(&first_manifest_bytes)?;

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

pub(in crate::standard_library) fn read_manifest(
    root: &Path,
) -> Result<StandardLibraryBundleManifest, BuildError> {
    let bytes = read_manifest_bytes(root)?;

    decode_manifest(&bytes)
}

fn read_manifest_bytes(root: &Path) -> Result<Vec<u8>, BuildError> {
    let path = root.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    fs::read(&path).map_err(|error| BuildError::read(&path, error))
}

fn decode_manifest(bytes: &[u8]) -> Result<StandardLibraryBundleManifest, BuildError> {
    decode_standard_library_manifest(bytes)
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

pub(in crate::standard_library) fn build(
    source: &Path,
    output: &Path,
) -> Result<PathBuf, BuildError> {
    let graph = load_standard_library_project_graph(source)
        .map_err(|error| BuildError::Project(format!("{error:?}")))?;

    let product = standard_library_product(&graph)?;
    let version = standard_library_version(&graph)?;

    build_product_bundle(product, version, source, output, product.targets())
}

pub(crate) fn build_target_bundle(
    source: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<PathBuf, String> {
    build_target_bundle_inner(source, output, target).map_err(|error| error.to_string())
}

fn build_target_bundle_inner(
    source: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<PathBuf, BuildError> {
    let graph = load_standard_library_project_graph(source)
        .map_err(|error| BuildError::Project(format!("{error:?}")))?;

    let product = standard_library_product(&graph)?;
    let version = standard_library_version(&graph)?;
    let target = TargetIdentity::try_new(target.as_str()).ok_or(BuildError::InvalidIdentity)?;

    build_product_bundle(
        product,
        version,
        source,
        output,
        std::slice::from_ref(&target),
    )
}

fn build_product_bundle(
    product: &ProjectProduct,
    version: &PackageVersion,
    source: &Path,
    output: &Path,
    targets: &[TargetIdentity],
) -> Result<PathBuf, BuildError> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));

    fs::create_dir_all(parent).map_err(|error| BuildError::write(parent, error))?;

    let _publication = PublicationLock::acquire(output)?;

    if output.exists() {
        return Err(BuildError::OutputExists(output.to_path_buf()));
    }

    let staging = tempfile::Builder::new()
        .prefix("bray-standard-library-")
        .tempdir_in(parent)
        .map_err(BuildError::TemporaryDirectory)?;

    let bundle = staging.path().join("bundle");

    fs::create_dir(&bundle).map_err(|error| BuildError::write(&bundle, error))?;

    let work = staging.path().join("work");
    let manifest = build_bundle(product, version, source, targets, &work, &bundle)?;
    let manifest_path = bundle.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let manifest_bytes = encode_standard_library_manifest(&manifest)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

    fs::write(&manifest_path, manifest_bytes)
        .map_err(|error| BuildError::write(&manifest_path, error))?;

    fs::rename(&bundle, output).map_err(|error| BuildError::publish(&bundle, output, error))?;

    Ok(output.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME))
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
    version: &PackageVersion,
    workspace_root: &Path,
    targets: &[TargetIdentity],
    work: &Path,
    bundle: &Path,
) -> Result<StandardLibraryBundleManifest, BuildError> {
    let source_paths: Vec<_> = product
        .sources()
        .iter()
        .map(|source| source.beneath(workspace_root))
        .collect();

    let mut interface = None;
    let mut built_targets = Vec::new();

    for target in targets {
        let built = build_target(product, version, &source_paths, target, work)?;

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

        built_targets.push(target);
    }

    let (_, interface) = interface.ok_or(BuildError::MissingInterface)?;

    StandardLibraryBundleManifest::try_new(interface, built_targets)
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
    version: &PackageVersion,
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
            .with_platform_services(product.platform_services().iter().cloned())
            .with_package_interface_export(interface_export_request(product.identity(), version)?);

    let compilation = load_llvm_compilation(request).ok_or(BuildError::CompilerUnavailable)?;

    let linker =
        native_linker(native).ok_or_else(|| BuildError::LinkerUnavailable(target.clone()))?;

    let native_facts = compilation
        .native_product_facts(
            product.identity().clone(),
            BuildConfiguration::Release,
            None,
            [],
            Some(&linker),
        )
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

    let outcome = compilation.emit_product(request, inputs).map_err(|error| {
        BuildError::Emission(format!(
            "{:?}; diagnostics={:?}",
            error.kind(),
            compilation.check_diagnostics()
        ))
    })?;

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
    version: &PackageVersion,
) -> Result<bray_compilation::PackageInterfaceExportRequest, BuildError> {
    let product_identity =
        InterfaceProductIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY)
            .ok_or(BuildError::InvalidIdentity)?;

    // The interface shares the immutable Arc-backed package version from the project graph.
    let identity = PackageInterfaceIdentity::try_new(
        product.package().clone(),
        version.clone(),
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

fn standard_library_version(graph: &ProjectGraph) -> Result<&PackageVersion, BuildError> {
    let package = PackageIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
        .ok_or(BuildError::InvalidIdentity)?;

    graph
        .package(&package)
        .map(bray_project::ProjectPackage::version)
        .ok_or(BuildError::MissingProduct)
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

pub(in crate::standard_library) fn write_bundle_artifact(
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME;
    use bray_target::NativeTarget;

    use super::{BuildError, BuildOptions, build, compare_bundles, read_manifest};

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
        assert_eq!(options.target, None);
    }

    #[test]
    fn build_options_accept_one_exact_native_target() {
        let options = BuildOptions::parse(
            [
                "--output".to_owned(),
                "output".to_owned(),
                "--target".to_owned(),
                "x86_64-pc-windows-msvc".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap_or_else(|error| panic!("build options must parse: {error}"));

        assert_eq!(options.target, Some(NativeTarget::X86_64WindowsMsvc));
    }

    #[test]
    fn bundle_comparison_rejects_different_manifest_bytes_before_decoding() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        let first = directory.path().join("first");
        let second = directory.path().join("second");

        fs::create_dir(&first)
            .unwrap_or_else(|error| panic!("first bundle root must exist: {error}"));

        fs::create_dir(&second)
            .unwrap_or_else(|error| panic!("second bundle root must exist: {error}"));

        fs::write(first.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME), [0])
            .unwrap_or_else(|error| panic!("first manifest must exist: {error}"));

        fs::write(second.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME), [1])
            .unwrap_or_else(|error| panic!("second manifest must exist: {error}"));

        assert!(matches!(
            compare_bundles(&first, &second),
            Err(BuildError::NonReproducibleManifest)
        ));
    }

    #[test]
    #[ignore = "builds every production standard-library target"]
    fn production_standard_library_build_emits_a_complete_bundle() {
        let workspace = crate::workspace::root()
            .unwrap_or_else(|error| panic!("workspace root must be available: {error}"));

        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary output root must exist: {error}"));

        let source = workspace.join("standard-library");
        let output = directory.path().join("bundle");

        build(&source, &output)
            .unwrap_or_else(|error| panic!("production standard library must build: {error}"));

        let manifest = read_manifest(&output)
            .unwrap_or_else(|error| panic!("built manifest must decode: {error}"));

        assert!(manifest.interface().beneath(&output).is_file());
        assert!(!manifest.targets().is_empty());

        for artifact in manifest
            .targets()
            .iter()
            .flat_map(bray_standard_library::StandardLibraryTargetArtifacts::artifacts)
        {
            assert!(artifact.beneath(&output).is_file());
        }
    }
}
