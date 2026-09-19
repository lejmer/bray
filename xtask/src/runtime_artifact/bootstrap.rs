use std::fs;
use std::path::Path;

use bray_base::NonEmptySharedStr;
use bray_compilation::{BuildConfiguration, WorkerBudget};
use bray_driver::{
    DriverBackend, DriverCompilationConfiguration, DriverOptions, DriverProductConfiguration,
    OutputFormat, run_build_request,
};
use bray_emitter::ArtifactKind;
use bray_project::{PackageRole, ProjectGraph, ProjectPackage, ProjectProduct, load_project_graph};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeRoleSourceBinding};
use bray_standard_library::{PackageSourceAuthority, StandardLibraryRoot};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy)]
pub(super) enum BrayRuntimeComponent {
    Bootstrap,
    Observation,
}

impl BrayRuntimeComponent {
    const ALL: [Self; 2] = [Self::Bootstrap, Self::Observation];

    const fn package(self) -> &'static str {
        match self {
            Self::Bootstrap => "bray_runtime_bootstrap",
            Self::Observation => "bray_runtime_observation",
        }
    }

    const fn product(self) -> &'static str {
        match self {
            Self::Bootstrap => "runtime",
            Self::Observation => "observation",
        }
    }

    const fn role_artifact(self) -> bray_runtime_interface::RuntimeRoleArtifact {
        match self {
            Self::Bootstrap => bray_runtime_interface::RuntimeRoleArtifact::Bootstrap,
            Self::Observation => bray_runtime_interface::RuntimeRoleArtifact::Observation,
        }
    }
}

pub(super) fn cache_identity(
    root: &Path,
    target: NativeTarget,
    output: &Path,
    input: &str,
) -> Result<Option<String>, String> {
    let workspace = root.join("runtime");

    let graph = load_project_graph(&workspace)
        .map_err(|error| format!("could not load bootstrap project: {error:?}"))?;

    let mut identity = Sha256::new();

    identity.update(input);

    for component in BrayRuntimeComponent::ALL {
        let (_, product) = selected_product(&graph, target, component)?;

        for kind in [
            TargetOutputKind::PackageInterface,
            TargetOutputKind::PackageImplementation,
        ] {
            let name = TargetOutputName::for_native(target.object_format(), kind)
                .file_name(product.identity().name())
                .unwrap_or_else(|| unreachable!("package artifacts always have file names"));

            let path = output.join(name);

            match bray_base::sha256_file(&path) {
                Ok(artifact) => identity.update(artifact),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(format!("could not hash {}: {error}", path.display()));
                }
            }
        }
    }

    Ok(Some(bray_base::lowercase_hex(&identity.finalize())))
}

/// Compiles the manifest-selected trusted runtime bootstrap for the requested target.
pub fn build(root: &Path, target: NativeTarget, destination: &Path) -> Result<(), String> {
    build_components(root, target, &[(BrayRuntimeComponent::Bootstrap, destination)])
}

pub(super) fn build_runtime_components(
    root: &Path,
    target: NativeTarget,
    bootstrap: &Path,
    observation: &Path,
) -> Result<(), String> {
    build_components(
        root,
        target,
        &[
            (BrayRuntimeComponent::Bootstrap, bootstrap),
            (BrayRuntimeComponent::Observation, observation),
        ],
    )
}

fn build_components(
    root: &Path,
    target: NativeTarget,
    components: &[(BrayRuntimeComponent, &Path)],
) -> Result<(), String> {
    let support = tempfile::Builder::new()
        .prefix("bray-runtime-components-")
        .tempdir()
        .map_err(|error| format!("could not create bootstrap build directory: {error}"))?;

    let standard_library = crate::workspace::cargo_target(root)
        .join("runtime-bootstrap-standard-library")
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

    for (component, destination) in components {
        build_component(
            &workspace,
            &graph,
            &standard_library,
            target,
            support.path(),
            *component,
            destination,
        )?;
    }

    Ok(())
}

fn build_component(
    workspace: &Path,
    graph: &ProjectGraph,
    standard_library: &Path,
    target: NativeTarget,
    support: &Path,
    component: BrayRuntimeComponent,
    destination: &Path,
) -> Result<(), String> {
    let (package, product) = selected_product(graph, target, component)?;

    let sources = product
        .sources()
        .iter()
        .map(|source| source.beneath(&workspace))
        .collect::<Vec<_>>();

    let selected = bray_compilation::SelectedTarget::for_native(target);

    let mut native_links = crate::standard_library::native_links(selected.profile().identity())
        .map_err(|error| format!("could not load runtime native link inputs: {error}"))?;

    if matches!(component, BrayRuntimeComponent::Bootstrap) {
        native_links.extend([
            runtime_dependency("bray_runtime_host"),
            runtime_dependency("bray_runtime_callback"),
        ]);
    }

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
    .with_runtime_roles(runtime_bindings(component));

    let standard_library = StandardLibraryRoot::try_new(&standard_library)
        .ok_or_else(|| "runtime standard-library root is invalid".to_owned())?;

    let options = DriverOptions::new(
        WorkerBudget::default(),
        OutputFormat::Text,
        compilation,
        Some(standard_library),
        None,
        PackageSourceAuthority::Ordinary,
    );

    let output = support.join(component.product()).join("emission");

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

        return Err(format!("{} product build failed:\n{detail}", component.product()));
    }

    publish_outputs(product, &output, destination)
}

fn selected_product(
    graph: &ProjectGraph,
    target: NativeTarget,
    component: BrayRuntimeComponent,
) -> Result<(&ProjectPackage, &ProjectProduct), String> {
    let target = target.identity();

    let plan = graph.build_plan(&target).ok_or_else(|| {
        format!(
            "runtime project does not support target {}",
            target.as_str()
        )
    })?;

    let selected = plan.products().iter().find_map(|identity| {
        let package = graph.package(identity.package())?;

        (package.role() == PackageRole::Root
            && package.identity().as_str() == component.package()
            && identity.name() == component.product())
        .then(|| {
            package
                .products()
                .iter()
                .find(|product| product.identity() == identity)
                .map(|product| (package, product))
        })?
    });

    selected.ok_or_else(|| {
        format!(
            "runtime project has no {} root product",
            component.product()
        )
    })
}

fn runtime_dependency(name: &'static str) -> NativeLinkRequirement {
    NativeLinkRequirement::new(
        NonEmptySharedStr::try_new(name)
            .unwrap_or_else(|| unreachable!("runtime dependency names are non-empty")),
        NativeLinkKind::System,
    )
}

fn runtime_bindings(component: BrayRuntimeComponent) -> Vec<RuntimeRoleSourceBinding> {
    RuntimeAbiRole::ALL
        .into_iter()
        .filter(|role| role.source_artifact() == Some(component.role_artifact()))
        .filter_map(RuntimeAbiRole::source_binding)
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
