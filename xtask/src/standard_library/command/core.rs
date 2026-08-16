use std::fs;
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
use bray_runtime_interface::PlatformServiceRole;
use bray_standard_library::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY, STANDARD_LIBRARY_MANIFEST_FILE_NAME,
    StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
    StandardLibraryTargetArtifacts, decode_standard_library_manifest,
    encode_standard_library_manifest, standard_library_target_artifact_directory,
};
use bray_symbols::{PackageIdentity, PackageVersion, ProductIdentity, ProductKind};
use bray_target::{
    NativeTarget, TargetIdentity, TargetOutputDescription, TargetOutputKind, TargetOutputName,
};
use bray_tooling::{load_llvm_compilation, native_linker, source_inputs_from_file_arguments};

use super::error::BuildError;
use crate::bundle::{DirectoryPublication, NativeBuildOptions, NativeBuildOptionsBuilder};
use crate::workspace;

const USAGE: &str = "usage: cargo xtask standard-library \
    <build --output <directory> [--source <directory>] [--target <triple>] | \
    os-constants generate [--check] | \
    test [--profile-output <directory>] | verify>";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("build") => BuildOptions::parse(arguments).and_then(BuildOptions::build),
        Some("os-constants") => crate::standard_library::os_constants::run(arguments)
            .map(|()| PathBuf::new())
            .map_err(BuildError::OsConstants),
        Some("test") => native_test(arguments).map(|()| PathBuf::new()),
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

fn native_test(mut arguments: impl Iterator<Item = String>) -> Result<(), BuildError> {
    let profile_output = native_profile_output(&mut arguments)?;

    crate::standard_library::native::test(profile_output.as_deref())
}

fn native_profile_output(
    arguments: &mut impl Iterator<Item = String>,
) -> Result<Option<PathBuf>, BuildError> {
    let profile_output = match arguments.next().as_deref() {
        None => None,
        Some("--profile-output") => Some(path_argument(arguments, "--profile-output")?),
        Some(argument) => return Err(BuildError::UnexpectedArgument(argument.to_owned())),
    };

    if let Some(argument) = arguments.next() {
        return Err(BuildError::UnexpectedArgument(argument));
    }

    profile_output
        .map(|path| {
            std::path::absolute(&path).map_err(|error| {
                BuildError::conformance(
                    "native profile",
                    format!("could not resolve output: {error}"),
                )
            })
        })
        .transpose()
}

struct BuildOptions {
    native: NativeBuildOptions,
    source: PathBuf,
}

impl BuildOptions {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, BuildError> {
        let mut source = None;
        let mut native = NativeBuildOptionsBuilder::default();

        while let Some(argument) = arguments.next() {
            if native
                .parse_option(&argument, &mut arguments)
                .map_err(BuildError::BuildOptions)?
            {
                continue;
            }

            match argument.as_str() {
                "--source" if source.is_none() => {
                    source = Some(path_argument(&mut arguments, "--source")?);
                }
                _ => return Err(BuildError::UnexpectedArgument(argument)),
            }
        }

        let native = native.finish().map_err(BuildError::BuildOptions)?;

        let source = match source {
            Some(source) => source,
            None => workspace::root()
                .map_err(BuildError::Workspace)?
                .join("standard-library"),
        };

        Ok(Self { native, source })
    }

    fn build(self) -> Result<PathBuf, BuildError> {
        match self.native.target() {
            Some(target) => build_target_bundle_inner(&self.source, self.native.output(), target),
            None => build(&self.source, self.native.output()),
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

    crate::progress::run("Checking standard library OS constants", || {
        crate::standard_library::os_constants::verify()
    })
    .map_err(BuildError::OsConstants)?;

    let directory = tempfile::Builder::new()
        .prefix("bray-standard-library-verification-")
        .tempdir()
        .map_err(BuildError::TemporaryDirectory)?;

    crate::progress::run("Verifying the standard library bundle", || {
        crate::standard_library::conformance::verify(directory.path())
    })
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
    let publication = DirectoryPublication::begin(output, "bray-standard-library-")
        .map_err(BuildError::Publication)?;

    let manifest = build_bundle(
        product,
        version,
        source,
        targets,
        publication.work(),
        publication.contents(),
    )?;

    let bundle = publication.contents();
    let manifest_path = bundle.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let manifest_bytes = encode_standard_library_manifest(&manifest)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

    fs::write(&manifest_path, manifest_bytes)
        .map_err(|error| BuildError::write(&manifest_path, error))?;

    let published = publication.publish().map_err(BuildError::Publication)?;

    Ok(published.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME))
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

    let mut built_targets = Vec::new();
    let root = workspace::root().map_err(BuildError::Workspace)?;
    let temporal_provenance_path = root.join("third-party/temporal/provenance.json");

    let temporal_provenance = fs::read(&temporal_provenance_path)
        .map_err(|error| BuildError::read(&temporal_provenance_path, error))?;

    for (index, target) in targets.iter().enumerate() {
        crate::progress::item(
            index.saturating_add(1),
            targets.len(),
            &format!("Building the {} standard library", target.as_str()),
        );

        let built = crate::progress::run(
            &format!("Compiling the {} standard library", target.as_str()),
            || build_target(product, version, &source_paths, target, work),
        )?;

        let BuiltTarget {
            selected,
            native,
            interface_bytes,
            implementation_bytes,
            archive_bytes,
            platform_archives,
        } = built;

        let abi = selected.runtime_abi();
        let target_path = standard_library_target_artifact_directory(target, abi);

        let interface_path = format!("{target_path}/std.brayi");

        write_bundle_artifact(bundle, &interface_path, &interface_bytes)?;

        let interface = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageInterface,
            interface_path,
            &interface_bytes,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let implementation_path = format!("{target_path}/std.brayimpl");

        write_bundle_artifact(bundle, &implementation_path, &implementation_bytes)?;

        let implementation = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageImplementation,
            implementation_path,
            &implementation_bytes,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let file_name = standard_library_archive_name(native)?;

        let portable_path = format!("{target_path}/{file_name}");

        write_bundle_artifact(bundle, &portable_path, &archive_bytes)?;

        let archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            portable_path,
            &archive_bytes,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let mut artifacts = vec![interface, implementation, archive];

        for platform in platform_archives {
            let platform_file_name = platform_abi_archive_name(native, platform.name)?;
            let platform_path = format!("{target_path}/{platform_file_name}");

            write_bundle_artifact(bundle, &platform_path, &platform.bytes)?;

            let platform_archive = StandardLibraryArtifact::try_for_bytes(
                StandardLibraryArtifactKind::PlatformServiceLibrary,
                platform_path,
                &platform.bytes,
            )
            .map(|artifact| artifact.with_platform_services(platform.roles.iter().copied()))
            .map(|artifact| artifact.with_native_links(platform.native_links))
            .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

            artifacts.push(platform_archive);
        }

        let provenance_path = format!("{target_path}/temporal-provider.json");

        write_bundle_artifact(bundle, &provenance_path, &temporal_provenance)?;

        let provenance = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::DependencyMetadata,
            provenance_path,
            &temporal_provenance,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        artifacts.push(provenance);

        let target = StandardLibraryTargetArtifacts::try_new(target.clone(), abi, artifacts)
            .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        built_targets.push(target);
    }

    StandardLibraryBundleManifest::try_new(built_targets)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))
}

struct BuiltTarget {
    selected: SelectedTarget,
    native: NativeTarget,
    interface_bytes: Vec<u8>,
    implementation_bytes: Vec<u8>,
    archive_bytes: Vec<u8>,
    platform_archives: Vec<BuiltPlatformArchive>,
}

struct BuiltPlatformArchive {
    name: &'static str,
    roles: &'static [PlatformServiceRole],
    bytes: Vec<u8>,
    native_links: Vec<bray_symbols::NativeLinkRequirement>,
}

struct BuiltPlatformArtifacts {
    archives: Vec<BuiltPlatformArchive>,
}

fn build_platform_archives(
    root: &Path,
    native: NativeTarget,
) -> Result<BuiltPlatformArtifacts, BuildError> {
    let mut archives = Vec::new();

    for partition in super::platform::PARTITIONS {
        let build = if partition.uses_rust_standard_library {
            crate::native_archive::build_rust_static_library
        } else {
            crate::native_archive::build_no_std_rust_static_library
        };

        let built = build(
            root,
            native,
            "bray-platform-abi",
            "release",
            &[partition.feature],
        )
        .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

        let bytes =
            fs::read(built.archive()).map_err(|error| BuildError::read(built.archive(), error))?;

        archives.push(BuiltPlatformArchive {
            name: partition.name,
            roles: partition.roles,
            bytes,
            native_links: built.native_links().to_vec(),
        });
    }

    Ok(BuiltPlatformArtifacts { archives })
}

fn standard_library_archive_name(target: NativeTarget) -> Result<String, BuildError> {
    TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
        .file_name(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
        .ok_or(BuildError::InvalidIdentity)
}

fn platform_abi_archive_name(target: NativeTarget, partition: &str) -> Result<String, BuildError> {
    TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
        .file_name(partition)
        .ok_or(BuildError::InvalidIdentity)
}

fn build_target(
    product: &ProjectProduct,
    version: &PackageVersion,
    source_paths: &[PathBuf],
    target: &TargetIdentity,
    work: &Path,
) -> Result<BuiltTarget, BuildError> {
    if !super::platform::inventory_matches(product.platform_services()) {
        return Err(BuildError::Manifest(
            "platform archive role inventory does not match the standard library".to_owned(),
        ));
    }

    let selected = SelectedTarget::for_identity(target)
        .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))?;

    let native = selected
        .native_target()
        .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))?;

    let root = workspace::root().map_err(BuildError::Workspace)?;

    let platform = if product.platform_services().is_empty() {
        BuiltPlatformArtifacts {
            archives: Vec::new(),
        }
    } else {
        crate::progress::run("Building standard library platform providers", || {
            build_platform_archives(&root, native)
        })?
    };

    let output = work.join(target.as_str());

    fs::create_dir_all(&output).map_err(|error| BuildError::write(&output, error))?;

    let request = standard_library_source_request(
        product,
        version,
        source_paths,
        &selected,
        WorkerBudget::default(),
    )?;

    let compilation = load_llvm_compilation(request).map_err(BuildError::CompilerUnavailable)?;

    let linker = native_linker(native, None).map_err(|error| BuildError::LinkerUnavailable {
        target: target.clone(),
        detail: format!("{error:?}"),
    })?;

    let native_plan = crate::progress::run("Planning the standard library native product", || {
        compilation
            .native_product_plan(
                product.identity().clone(),
                BuildConfiguration::Release,
                None,
                [],
                Some(&linker),
            )
            .map_err(|error| BuildError::CompilationFailed {
                target: target.clone(),
                detail: format!(
                    "{error:?}. diagnostics={:?}",
                    compilation.check_diagnostics()
                ),
            })
    })?;

    let output_description = TargetOutputDescription::for_native(
        native,
        [
            TargetOutputKind::PackageInterface,
            TargetOutputKind::PackageImplementation,
            TargetOutputKind::StaticLibrary,
        ],
    );

    let request = EmissionRequest::try_new(
        product.identity().clone(),
        ProductKind::Library,
        None,
        target.clone(),
        RequestedArtifactDestination::FilesystemDirectory(output.into()),
        [
            RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            ),
            RequestedArtifact::new(
                ArtifactKind::PackageImplementation,
                ArtifactRequirement::Required,
            ),
            RequestedArtifact::new(ArtifactKind::StaticLibrary, ArtifactRequirement::Required),
        ],
        ReplacementPolicy::RequireAbsent,
    )
    .map_err(|error| BuildError::EmissionRequest(format!("{error:?}")))?;

    let inputs =
        ProductEmissionInputs::new(&output_description).with_native_product(&native_plan, &linker);

    let outcome = crate::progress::run("Emitting standard library artifacts", || {
        compilation.emit_product(request, inputs).map_err(|error| {
            BuildError::Emission(format!(
                "{:?}. diagnostics={:?}",
                error.kind(),
                compilation.check_diagnostics()
            ))
        })
    })?;

    if !matches!(outcome.status(), EmissionStatus::Complete) {
        return Err(BuildError::CompilationFailed {
            target: target.clone(),
            detail: format!(
                "{:?}. diagnostics={:?}",
                outcome.status(),
                outcome.diagnostics()
            ),
        });
    }

    let interface_path = emitted_path(&outcome, ArtifactKind::PackageInterface)?;
    let implementation_path = emitted_path(&outcome, ArtifactKind::PackageImplementation)?;
    let archive_path = emitted_path(&outcome, ArtifactKind::StaticLibrary)?;

    let interface_bytes =
        fs::read(&interface_path).map_err(|error| BuildError::read(&interface_path, error))?;

    let implementation_bytes = fs::read(&implementation_path)
        .map_err(|error| BuildError::read(&implementation_path, error))?;

    let archive_bytes =
        fs::read(&archive_path).map_err(|error| BuildError::read(&archive_path, error))?;

    Ok(BuiltTarget {
        selected,
        native,
        interface_bytes,
        implementation_bytes,
        archive_bytes,
        platform_archives: platform.archives,
    })
}

pub(in crate::standard_library) fn standard_library_source_request(
    product: &ProjectProduct,
    version: &PackageVersion,
    source_paths: &[PathBuf],
    selected: &SelectedTarget,
    worker_budget: WorkerBudget,
) -> Result<CompilationRequest, BuildError> {
    let sources = source_inputs_from_file_arguments(source_paths.iter().cloned())
        .map_err(|error| BuildError::Source(format!("{error:?}")))?;

    let options = CompilationOptions::new(worker_budget, ProductKind::Library, selected.clone());

    Ok(
        CompilationRequest::with_options(product.identity().package().clone(), sources, options)
            .with_standard_library_source_authority()
            .with_platform_services(product.platform_services().iter().cloned())
            .with_package_interface_export(interface_export_request(product.identity(), version)?),
    )
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
            OutputSink::ManagedFilesystem { .. } => outcome
                .generation()
                .and_then(|generation| generation.artifact_path(artifact.id())),
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

pub(in crate::standard_library) fn standard_library_version(
    graph: &ProjectGraph,
) -> Result<&PackageVersion, BuildError> {
    let package = PackageIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
        .ok_or(BuildError::InvalidIdentity)?;

    graph
        .package(&package)
        .map(bray_project::ProjectPackage::version)
        .ok_or(BuildError::MissingProduct)
}

pub(in crate::standard_library) fn standard_library_product(
    graph: &ProjectGraph,
) -> Result<&ProjectProduct, BuildError> {
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

    use super::{
        BuildError, BuildOptions, build, compare_bundles, native_profile_output,
        platform_abi_archive_name, read_manifest, standard_library_archive_name,
    };

    #[test]
    fn build_options_require_an_output_and_reject_unknown_arguments() {
        assert!(matches!(
            BuildOptions::parse(std::iter::empty()),
            Err(BuildError::BuildOptions(
                crate::bundle::NativeBuildOptionsError::MissingOutput
            ))
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
        assert_eq!(options.native.output(), PathBuf::from("output"));
        assert_eq!(options.native.target(), None);
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

        assert_eq!(
            options.native.target(),
            Some(NativeTarget::X86_64WindowsMsvc)
        );
    }

    #[test]
    fn native_test_profile_output_is_explicit_and_absolute() {
        let mut arguments =
            ["--profile-output".to_owned(), "profiles/native".to_owned()].into_iter();

        let output = native_profile_output(&mut arguments)
            .unwrap_or_else(|error| panic!("native profile output must parse: {error}"))
            .unwrap_or_else(|| panic!("native profile output must be retained"));

        assert!(output.is_absolute());
        assert!(output.ends_with("profiles/native"));
        assert!(arguments.next().is_none());

        assert!(matches!(
            native_profile_output(&mut ["--unknown".to_owned()].into_iter()),
            Err(BuildError::UnexpectedArgument(argument)) if argument == "--unknown"
        ));
    }

    #[test]
    fn standard_library_archives_follow_native_library_naming() {
        assert_eq!(
            standard_library_archive_name(NativeTarget::X86_64WindowsMsvc)
                .unwrap_or_else(|error| panic!("Windows archive name must be valid: {error}")),
            "std.lib"
        );

        assert_eq!(
            standard_library_archive_name(NativeTarget::X86_64LinuxGnu)
                .unwrap_or_else(|error| panic!("Linux archive name must be valid: {error}")),
            "libstd.a"
        );

        assert_eq!(
            standard_library_archive_name(NativeTarget::Aarch64MacOs)
                .unwrap_or_else(|error| panic!("macOS archive name must be valid: {error}")),
            "libstd.a"
        );

        assert_eq!(
            platform_abi_archive_name(
                NativeTarget::X86_64WindowsMsvc,
                "bray_platform_standard_streams",
            )
            .unwrap_or_else(|error| panic!("Windows platform archive must be valid: {error}")),
            "bray_platform_standard_streams.lib"
        );

        assert_eq!(
            platform_abi_archive_name(NativeTarget::X86_64LinuxGnu, "bray_platform_filesystem")
                .unwrap_or_else(|error| panic!("Linux platform archive must be valid: {error}")),
            "libbray_platform_filesystem.a"
        );
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
