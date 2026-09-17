use std::fs;
use std::path::Path;

use bray_base::NonEmptySharedStr;
use bray_compilation::{BuildConfiguration, WorkerBudget};
use bray_driver::{
    DriverBackend, DriverCompilationConfiguration, DriverOptions, DriverProductConfiguration,
    OutputFormat, run_build_request,
};
use bray_emitter::ArtifactKind;
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceValidationPolicy, PackageImplementationArtifact,
    ValidatedPackageInterface,
};
use bray_project::{PackageRole, ProjectGraph, ProjectPackage, ProjectProduct, load_project_graph};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeRoleSourceBinding};
use bray_standard_library::{PackageSourceAuthority, StandardLibraryRoot};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

pub(super) fn current(root: &Path, target: NativeTarget, output: &Path) -> Result<bool, String> {
    let workspace = root.join("runtime");

    let graph = load_project_graph(&workspace)
        .map_err(|error| format!("could not load bootstrap project: {error:?}"))?;

    let (package, product) = selected_product(&graph, target)?;

    let artifact_path = |kind| {
        let name = TargetOutputName::for_native(target.object_format(), kind)
            .file_name(product.identity().name())
            .unwrap_or_else(|| unreachable!("package artifacts always have file names"));

        output.join(name)
    };

    let Ok(interface_bytes) = fs::read(artifact_path(TargetOutputKind::PackageInterface)) else {
        return Ok(false);
    };

    let Ok(implementation_bytes) = fs::read(artifact_path(TargetOutputKind::PackageImplementation))
    else {
        return Ok(false);
    };

    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let Ok(interface) = ValidatedPackageInterface::try_new(interface_bytes, policy) else {
        return Ok(false);
    };

    let Ok(surface) = interface.decode_identity_surface() else {
        return Ok(false);
    };

    if interface.decode_semantics(&surface).is_err()
        || surface.identity().package() != product.identity().package()
        || surface.identity().version() != package.version()
        || surface.identity().product().as_str() != product.identity().name()
    {
        return Ok(false);
    }

    let Ok(implementation) =
        PackageImplementationArtifact::try_from_bytes(implementation_bytes, policy.limits())
    else {
        return Ok(false);
    };

    Ok(implementation
        .validate_interface(&interface, &surface)
        .is_ok())
}

pub(super) fn build(root: &Path, target: NativeTarget, destination: &Path) -> Result<(), String> {
    let support = tempfile::Builder::new()
        .prefix("bray-runtime-bootstrap-")
        .tempdir()
        .map_err(|error| format!("could not create bootstrap build directory: {error}"))?;

    let standard_library = root
        .join("target/runtime-bootstrap-standard-library")
        .join(target.as_str());

    crate::standard_library::build_target_bundle(
        &root.join("standard-library"),
        &standard_library,
        target,
    )
    .map_err(|error| format!("could not build bootstrap standard library: {error}"))?;

    let workspace = root.join("runtime");

    let graph = load_project_graph(&workspace)
        .map_err(|error| format!("could not load bootstrap project: {error:?}"))?;

    let (package, product) = selected_product(&graph, target)?;

    let sources = product
        .sources()
        .iter()
        .map(|source| source.beneath(&workspace))
        .collect::<Vec<_>>();

    let selected = bray_compilation::SelectedTarget::for_native(target);

    let mut native_links = crate::standard_library::native_links(selected.profile().identity())
        .map_err(|error| format!("could not load bootstrap native link inputs: {error}"))?;

    native_links.extend([
        runtime_dependency("bray_runtime_host"),
        runtime_dependency("bray_runtime_callback"),
    ]);

    let compilation = DriverCompilationConfiguration::new(
        product.identity().clone(),
        product.identity().package().clone(),
        package.version().clone(),
        product.kind(),
        target,
        Vec::new(),
        product.platform_services().to_vec(),
        native_links,
    )
    .with_runtime_roles(runtime_bindings());

    let standard_library = StandardLibraryRoot::try_new(&standard_library)
        .ok_or_else(|| "bootstrap standard-library root is invalid".to_owned())?;

    let options = DriverOptions::new(
        WorkerBudget::default(),
        OutputFormat::Text,
        compilation,
        Some(standard_library),
        None,
        PackageSourceAuthority::Ordinary,
    );

    let output = support.path().join("emission");

    let build = DriverProductConfiguration::new(
        DriverBackend::Llvm,
        BuildConfiguration::ObjectRelease,
        None,
        Vec::new(),
        output.clone(),
        false,
        product.outputs().to_vec(),
        Vec::new(),
    );

    let result = run_build_request(&options, build, sources);

    if result.exit_code() != std::process::ExitCode::SUCCESS {
        let detail = result
            .sources()
            .and_then(|sources| {
                crate::diagnostic_output::render_diagnostics(result.diagnostics(), sources)
            })
            .unwrap_or_else(|| format!("{:?}", result.diagnostics()));

        return Err(format!("bootstrap product build failed:\n{detail}"));
    }

    publish_outputs(product, &output, destination)
}

fn selected_product(
    graph: &ProjectGraph,
    target: NativeTarget,
) -> Result<(&ProjectPackage, &ProjectProduct), String> {
    let target = target.identity();

    let plan = graph.build_plan(&target).ok_or_else(|| {
        format!(
            "bootstrap project does not support target {}",
            target.as_str()
        )
    })?;

    let mut selected = plan.products().iter().filter_map(|identity| {
        let package = graph.package(identity.package())?;

        (package.role() == PackageRole::Root).then(|| {
            package
                .products()
                .iter()
                .find(|product| product.identity() == identity)
                .map(|product| (package, product))
        })?
    });

    let product = selected
        .next()
        .ok_or_else(|| "bootstrap project has no root product".to_owned())?;

    if selected.next().is_some() {
        return Err("bootstrap project selects more than one root product".to_owned());
    }

    Ok(product)
}

fn runtime_dependency(name: &'static str) -> NativeLinkRequirement {
    NativeLinkRequirement::new(
        NonEmptySharedStr::try_new(name)
            .unwrap_or_else(|| unreachable!("runtime dependency names are non-empty")),
        NativeLinkKind::System,
    )
}

fn runtime_bindings() -> Vec<RuntimeRoleSourceBinding> {
    RuntimeAbiRole::ALL
        .into_iter()
        .filter_map(RuntimeAbiRole::bootstrap_source_binding)
        .collect()
}

fn publish_outputs(
    product: &ProjectProduct,
    output: &Path,
    archive_destination: &Path,
) -> Result<(), String> {
    for kind in product.outputs() {
        let artifact = bray_emitter::resolve_published_artifact(
            output,
            product.identity(),
            ArtifactKind::from(*kind),
            0,
        )
        .map_err(|error| format!("could not resolve bootstrap output {kind:?}: {error:?}"))?;

        let destination = match kind {
            TargetOutputKind::StaticLibrary => archive_destination.to_path_buf(),
            TargetOutputKind::PackageInterface | TargetOutputKind::PackageImplementation => {
                archive_destination
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(
                        artifact
                            .path()
                            .file_name()
                            .ok_or_else(|| "bootstrap output has no file name".to_owned())?,
                    )
            }
            unsupported => {
                return Err(format!(
                    "bootstrap manifest requests unsupported output {unsupported:?}"
                ));
            }
        };

        fs::copy(artifact.path(), &destination).map_err(|error| {
            format!(
                "could not publish bootstrap output {}: {error}",
                destination.display()
            )
        })?;
    }

    Ok(())
}
