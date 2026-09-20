use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_codegen::{BackendIdentity, CodegenTarget};
use bray_compilation::{
    BuildConfiguration, CompilationProfileReport, ProductEmissionInputs, SelectedTarget,
    WorkerBudget,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus,
    ManagedFilesystemDestination, ManagedOutputDirectory, OutputSink, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_project::{ProjectGraph, ProjectProduct, load_standard_library_project_graph};
use bray_standard_library::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    STANDARD_LIBRARY_MANIFEST_FILE_NAME, StandardLibraryArtifact, StandardLibraryArtifactKind,
    StandardLibraryBundleManifest, StandardLibraryTargetArtifacts,
    decode_standard_library_manifest, encode_standard_library_manifest,
    standard_library_target_artifact_directory,
};
use bray_symbols::{NativeLinkRequirement, PackageIdentity, PackageVersion, ProductKind};
use bray_target::{
    NativeTarget, TargetIdentity, TargetOutputDescription, TargetOutputKind, TargetOutputName,
};
use bray_tooling::{load_llvm_compilation, native_linker};

use super::error::BuildError;
use super::options::{BuildOptions, BuildProfileOptions, path_argument, required_argument};
use super::profile::write_compiler_profiles;
use crate::bundle::DirectoryPublication;
use crate::workspace;

const USAGE: &str = "usage: cargo xtask standard-library \
    <build --output <directory> [--source <directory>] [--target <triple>] \
        [--profile <summary|trace> --profile-output <directory>] | \
    os-bindings <generate [--check] | probe [--target <triple>] --sdk-root <path> [--compiler-root <path>]> | \
    unicode <generate [--check]> | \
    test [--part <provider-retention|interoperability|api|outcomes>] [--profile-output <directory>] | verify>";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("build") => BuildOptions::parse(arguments).and_then(BuildOptions::build),
        Some("os-bindings") => crate::standard_library::os_bindings::run(arguments)
            .map(|()| PathBuf::new())
            .map_err(BuildError::OsBindings),
        Some("unicode") => crate::standard_library::unicode_data::run(arguments)
            .map(|()| PathBuf::new())
            .map_err(BuildError::UnicodeData),
        Some("test") => native_test(arguments).map(|()| PathBuf::new()),
        Some("verify") => super::verification::verify(arguments).map(|()| PathBuf::new()),
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
    let (parts, profile_output) = native_test_options(&mut arguments)?;

    crate::standard_library::native::test(&parts, profile_output.as_deref())
}

fn native_test_options(
    arguments: &mut impl Iterator<Item = String>,
) -> Result<(Vec<super::super::native::TestPart>, Option<PathBuf>), BuildError> {
    let mut parts = Vec::new();
    let mut profile_output = None;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--part" => {
                let value = required_argument(arguments, "--part")?;

                let value = super::super::native::TestPart::parse(&value)
                    .ok_or_else(|| BuildError::UnexpectedArgument(value))?;

                if !parts.contains(&value) {
                    parts.push(value);
                }
            }
            "--profile-output" if profile_output.is_none() => {
                profile_output = Some(path_argument(arguments, "--profile-output")?);
            }
            _ => return Err(BuildError::UnexpectedArgument(argument)),
        }
    }

    let profile_output = profile_output
        .map(|path| {
            std::path::absolute(&path).map_err(|error| {
                BuildError::conformance(
                    "native profile",
                    format!("could not resolve output: {error}"),
                )
            })
        })
        .transpose()?;

    Ok((parts, profile_output))
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
    build_selected_targets(source, output, None, None)
}

pub(super) fn build_selected_targets(
    source: &Path,
    output: &Path,
    target: Option<NativeTarget>,
    profile: Option<&BuildProfileOptions>,
) -> Result<PathBuf, BuildError> {
    crate::standard_library::unicode_data::verify().map_err(BuildError::UnicodeData)?;

    let graph = load_standard_library_project_graph(source)
        .map_err(|error| BuildError::Project(format!("{error:?}")))?;

    let product = standard_library_product(&graph)?;
    let version = standard_library_version(&graph)?;

    match target {
        Some(target) => {
            let target =
                TargetIdentity::try_new(target.as_str()).ok_or(BuildError::InvalidIdentity)?;

            build_product_bundle(
                product,
                version,
                source,
                output,
                std::slice::from_ref(&target),
                profile,
            )
        }
        None => build_product_bundle(product, version, source, output, product.targets(), profile),
    }
}

pub(crate) fn build_target_bundle(
    source: &Path,
    output: &Path,
    target: NativeTarget,
) -> Result<PathBuf, String> {
    build_selected_targets(source, output, Some(target), None).map_err(|error| error.to_string())
}

fn build_product_bundle(
    product: &ProjectProduct,
    version: &PackageVersion,
    source: &Path,
    output: &Path,
    targets: &[TargetIdentity],
    profile: Option<&BuildProfileOptions>,
) -> Result<PathBuf, BuildError> {
    let input = super::reuse::input_identity(source)?;

    if profile.is_none() && super::reuse::current(output, &input, targets)? {
        crate::progress::message("Reusing standard library bundle");

        return Ok(output.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME));
    }

    let publication = DirectoryPublication::begin(output, "bray-standard-library-")
        .map_err(BuildError::Publication)?;

    let (manifest, profiles) = build_bundle(
        product,
        version,
        source,
        targets,
        publication.work(),
        publication.contents(),
        profile,
    )?;

    let bundle = publication.contents();
    let manifest_path = bundle.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);

    let manifest_bytes = encode_standard_library_manifest(&manifest)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

    fs::write(&manifest_path, manifest_bytes)
        .map_err(|error| BuildError::write(&manifest_path, error))?;

    crate::input_identity::write_digest(bundle, &input).map_err(BuildError::InputIdentity)?;

    let published = publication.publish().map_err(BuildError::Publication)?;

    if let Some(profile) = profile {
        write_compiler_profiles(&profile.output, &profiles)?;
    }

    Ok(published.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME))
}

fn build_bundle(
    product: &ProjectProduct,
    version: &PackageVersion,
    workspace_root: &Path,
    targets: &[TargetIdentity],
    work: &Path,
    bundle: &Path,
    profile: Option<&BuildProfileOptions>,
) -> Result<
    (
        StandardLibraryBundleManifest,
        Vec<(TargetIdentity, CompilationProfileReport)>,
    ),
    BuildError,
> {
    let source_paths: Vec<_> = product
        .sources()
        .iter()
        .map(|source| source.beneath(workspace_root))
        .collect();

    let mut built_targets = Vec::new();
    let mut profiles = Vec::new();

    let root = workspace::root().map_err(BuildError::Workspace)?;
    let temporal_provenance_path = root.join("third-party/temporal/provenance.json");

    let temporal_provenance = fs::read(&temporal_provenance_path)
        .map_err(|error| BuildError::read(&temporal_provenance_path, error))?;

    let unicode_metadata_path = root.join("standard-library/targets/unicode/metadata.json");

    let unicode_metadata = fs::read(&unicode_metadata_path)
        .map_err(|error| BuildError::read(&unicode_metadata_path, error))?;

    for (index, target) in targets.iter().enumerate() {
        crate::progress::item(
            index.saturating_add(1),
            targets.len(),
            &format!("Building the {} standard library", target.as_str()),
        );

        let built = crate::progress::run(
            &format!("Compiling the {} standard library", target.as_str()),
            || build_target(product, version, &source_paths, target, work, profile),
        )?;

        let BuiltTarget {
            selected,
            native,
            backend,
            codegen_target,
            interface_bytes,
            implementation_bytes,
            archive_bytes,
            native_links,
            optimization,
            platform_archives,
            compiler_profile,
        } = built;

        if let Some(compiler_profile) = compiler_profile {
            profiles.push((target.clone(), compiler_profile));
        }

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
        .map(|artifact| artifact.with_native_links(native_links))
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let compiler_support_archive = crate::native_archive::target_compiler_support(native)
            .map_err(|error| BuildError::NativeArchive(error.to_string()))?;

        let compiler_support_bytes = fs::read(&compiler_support_archive)
            .map_err(|error| BuildError::read(&compiler_support_archive, error))?;

        let compiler_support_path = format!(
            "{target_path}/{}",
            platform_abi_archive_name(native, "bray_compiler_support")?
        );

        write_bundle_artifact(bundle, &compiler_support_path, &compiler_support_bytes)?;

        let compiler_support = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            compiler_support_path,
            &compiler_support_bytes,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let provenance_path = format!("{target_path}/temporal-provider.json");

        write_bundle_artifact(bundle, &provenance_path, &temporal_provenance)?;

        let provenance = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::DependencyMetadata,
            provenance_path,
            &temporal_provenance,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let unicode_path = format!("{target_path}/unicode-data.json");

        write_bundle_artifact(bundle, &unicode_path, &unicode_metadata)?;

        let unicode = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::DependencyMetadata,
            unicode_path,
            &unicode_metadata,
        )
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        let optimization_publication = super::super::optimization::OptimizationPublication::new(
            bundle,
            &target_path,
            native,
            abi,
            &backend,
            &codegen_target,
            &optimization,
        );

        let optimization_artifact =
            optimization_publication.publish_bray(&archive, optimization)?;

        let mut artifacts = vec![
            interface,
            implementation,
            archive,
            compiler_support,
            optimization_artifact,
        ];

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

            if let Some(optimization) = platform.optimization {
                let dependencies = if platform.uses_temporal_dependency_metadata {
                    std::slice::from_ref(&provenance)
                } else {
                    &[]
                };

                let artifact = optimization_publication.publish_native(
                    platform.name,
                    &platform.roles,
                    dependencies,
                    &platform_archive,
                    optimization,
                )?;

                artifacts.push(artifact);
            }

            artifacts.push(platform_archive);
        }

        artifacts.push(provenance);
        artifacts.push(unicode);

        let target = StandardLibraryTargetArtifacts::try_new(target.clone(), abi, artifacts)
            .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

        built_targets.push(target);
    }

    let manifest = StandardLibraryBundleManifest::try_new(built_targets)
        .map_err(|error| BuildError::Manifest(format!("{error:?}")))?;

    Ok((manifest, profiles))
}

struct BuiltTarget {
    selected: SelectedTarget,
    native: NativeTarget,
    backend: BackendIdentity,
    codegen_target: CodegenTarget,
    interface_bytes: Vec<u8>,
    implementation_bytes: Vec<u8>,
    archive_bytes: Vec<u8>,
    native_links: Vec<NativeLinkRequirement>,
    optimization: super::super::optimization::BuiltOptimizationArchive,
    platform_archives: Vec<super::platform::BuiltPlatformArchive>,
    compiler_profile: Option<CompilationProfileReport>,
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
    profile: Option<&BuildProfileOptions>,
) -> Result<BuiltTarget, BuildError> {
    if !super::platform::inventory_matches(product.platform_services()) {
        return Err(BuildError::Manifest(
            "platform provider roles are missing or duplicated in the standard library".to_owned(),
        ));
    }

    let selected = SelectedTarget::for_identity(target)
        .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))?;

    let native = selected
        .native_target()
        .ok_or_else(|| BuildError::UnsupportedTarget(target.clone()))?;

    let native_links =
        crate::standard_library::os_bindings::native_links(selected.profile().identity())
            .map_err(BuildError::OsBindings)?;

    let root = workspace::root().map_err(BuildError::Workspace)?;
    let output = work.join(target.as_str());

    let output_directory =
        ManagedOutputDirectory::try_new(target.as_str()).ok_or(BuildError::InvalidIdentity)?;

    let destination = ManagedFilesystemDestination::new(work, output_directory);

    fs::create_dir_all(&output).map_err(|error| BuildError::write(&output, error))?;

    let platform = crate::progress::run("Building platform providers", || {
        super::platform::build_archives(&root, native, &output, product.platform_services())
    })?;

    let request = super::source::request_with_native_links(
        product,
        version,
        source_paths,
        &selected,
        WorkerBudget::default(),
        native_links.clone(),
    )?;

    let request = match profile {
        Some(profile) => request
            .with_profile(profile.configuration)
            .with_profile_product(product.identity().clone()),
        None => request,
    };

    let compilation = load_llvm_compilation(request).map_err(BuildError::CompilerUnavailable)?;

    let linker =
        native_linker(native, None, work).map_err(|error| BuildError::LinkerUnavailable {
            target: target.clone(),
            detail: format!("{error:?}"),
        })?;

    let native_plan = crate::progress::run("Planning the standard library native product", || {
        compilation
            .native_product_plan(
                product.identity().clone(),
                BuildConfiguration::ObjectRelease,
                None,
                [],
                Some(linker.linker()),
            )
            .map_err(|error| {
                let diagnostics = compilation.check_diagnostics();

                BuildError::compilation_failed(
                    target.clone(),
                    format!("{error:?}"),
                    diagnostics,
                    compilation.sources(),
                )
            })
    })?;

    let output_description = TargetOutputDescription::for_native(
        native,
        [
            TargetOutputKind::PackageInterface,
            TargetOutputKind::PackageImplementation,
            TargetOutputKind::BackendBitcode,
            TargetOutputKind::StaticLibrary,
        ],
    );

    let request = EmissionRequest::try_new(
        product.identity().clone(),
        ProductKind::Library,
        None,
        target.clone(),
        RequestedArtifactDestination::FilesystemDirectory(destination),
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
            RequestedArtifact::new(ArtifactKind::BackendBitcode, ArtifactRequirement::Required),
        ],
        ReplacementPolicy::RequireAbsent,
    )
    .map_err(|error| BuildError::EmissionRequest(format!("{error:?}")))?
    .with_storage_profile(BuildConfiguration::ObjectRelease.as_str());

    let inputs = ProductEmissionInputs::new(&output_description)
        .with_native_product(&native_plan, linker.linker());

    let outcome = crate::progress::run("Emitting standard library artifacts", || {
        compilation.emit_product(request, inputs).map_err(|error| {
            let diagnostics = compilation.check_diagnostics();

            BuildError::emission(
                format!("{:?}", error.kind()),
                diagnostics,
                compilation.sources(),
            )
        })
    })?;

    if !matches!(outcome.status(), EmissionStatus::Complete) {
        return Err(BuildError::compilation_failed(
            target.clone(),
            format!("{:?}", outcome.status()),
            outcome.diagnostics(),
            compilation.sources(),
        ));
    }

    let compiler_profile = profile
        .map(|_| {
            compilation
                .profile_report()
                .ok_or_else(|| BuildError::MissingCompilerProfile(target.clone()))
        })
        .transpose()?;

    let required_path = |kind| {
        emitted_paths(&outcome, kind)
            .into_iter()
            .next()
            .ok_or(BuildError::MissingEmittedArtifact(kind))
    };

    let interface_path = required_path(ArtifactKind::PackageInterface)?;

    let implementation_path = required_path(ArtifactKind::PackageImplementation)?;

    let archive_path = required_path(ArtifactKind::StaticLibrary)?;
    let bitcode_paths = emitted_paths(&outcome, ArtifactKind::BackendBitcode);

    let interface_bytes =
        fs::read(&interface_path).map_err(|error| BuildError::read(&interface_path, error))?;

    let implementation_bytes = fs::read(&implementation_path)
        .map_err(|error| BuildError::read(&implementation_path, error))?;

    let archive_bytes =
        fs::read(&archive_path).map_err(|error| BuildError::read(&archive_path, error))?;

    let bitcode_modules = bitcode_paths
        .iter()
        .map(|path| fs::read(path).map_err(|error| BuildError::read(path, error)))
        .collect::<Result<Vec<_>, _>>()?;

    let optimization = super::super::optimization::from_bray_modules(
        &root,
        work,
        bitcode_modules,
        native_plan.preservation_roots().cloned(),
    )?;

    let backend = native_plan.backend().identity().clone();
    let codegen_target = native_plan.target().clone();

    Ok(BuiltTarget {
        selected,
        native,
        backend,
        codegen_target,
        interface_bytes,
        implementation_bytes,
        archive_bytes,
        native_links,
        optimization,
        platform_archives: platform,
        compiler_profile,
    })
}

fn emitted_paths(outcome: &bray_emitter::EmissionOutcome, kind: ArtifactKind) -> Vec<PathBuf> {
    outcome
        .artifacts()
        .artifacts()
        .iter()
        .filter(|artifact| artifact.id().kind() == kind)
        .filter_map(|artifact| match artifact.sink() {
            OutputSink::ManagedFilesystem { .. } => outcome
                .generation()
                .and_then(|generation| generation.artifact_path(artifact.id())),
            OutputSink::Filesystem(path) => Some(path.clone()),
            OutputSink::Memory { .. } | OutputSink::Stream(_) => None,
        })
        .collect()
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

    use bray_standard_library::STANDARD_LIBRARY_MANIFEST_FILE_NAME;
    use bray_target::NativeTarget;

    use super::{
        BuildError, build, compare_bundles, native_test_options, platform_abi_archive_name,
        read_manifest, standard_library_archive_name,
    };

    #[test]
    fn native_test_parts_and_profile_output_are_explicit() {
        let mut arguments = [
            "--part".to_owned(),
            "api".to_owned(),
            "--profile-output".to_owned(),
            "profiles/native".to_owned(),
            "--part".to_owned(),
            "outcomes".to_owned(),
        ]
        .into_iter();

        let (parts, output) = native_test_options(&mut arguments)
            .unwrap_or_else(|error| panic!("native test options must parse: {error}"));

        let output = output.unwrap_or_else(|| panic!("native profile output must be retained"));

        assert_eq!(
            parts,
            [
                super::super::super::native::TestPart::Api,
                super::super::super::native::TestPart::Outcomes,
            ]
        );

        assert!(output.is_absolute());
        assert!(output.ends_with("profiles/native"));
        assert!(arguments.next().is_none());

        assert!(matches!(
            native_test_options(&mut ["--unknown".to_owned()].into_iter()),
            Err(BuildError::UnexpectedArgument(argument)) if argument == "--unknown"
        ));

        assert!(matches!(
            native_test_options(
                &mut ["--part".to_owned(), "unknown".to_owned()].into_iter()
            ),
            Err(BuildError::UnexpectedArgument(argument)) if argument == "unknown"
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
