use std::path::Path;
use std::sync::Arc;

use bray_diagnostics::DiagnosticProjectManifestField as Field;
use bray_standard_library::{PackageSourceAuthority, is_public_standard_library_package};
use bray_symbols::{PackageVersion, ProductKind};
use bray_target::{NativeTarget, TargetIdentity};

use crate::manifest::{TargetManifest, WorkspacePackageManifest, decode_workspace_manifest};
use crate::{
    ProjectGraph, ProjectLoadError, ProjectPackage, ProjectPath, ProjectTarget,
    WORKSPACE_MANIFEST_FILE_NAME,
};

use super::package::{PendingPackage, load_package};
use super::validation::{local_name, paths_overlap, project_path, read_manifest_source};

/// Loads one explicit Bray workspace into an immutable dependency-first graph.
///
/// The loader reads only the fixed workspace manifest, package manifests in the
/// workspace's explicit package inventory, and `.bray` files beneath declared
/// source roots. It performs no search, acquisition, registry access, or network I/O.
pub fn load_project_graph(workspace_root: &Path) -> Result<ProjectGraph, ProjectLoadError> {
    load_project_graph_with_authority(workspace_root, PackageSourceAuthority::Ordinary, None)
}

/// Loads one toolchain-owned standard library workspace.
///
/// Every package in the workspace must use the reserved `std` namespace. This
/// authority does not change source-language trust or semantic checking rules.
pub fn load_standard_library_project_graph(
    workspace_root: &Path,
) -> Result<ProjectGraph, ProjectLoadError> {
    load_project_graph_with_authority(
        workspace_root,
        PackageSourceAuthority::StandardLibrary,
        None,
    )
}

/// Loads an ordinary workspace and executes selected generators through the supplied sandbox host.
/// Failure or cancellation publishes neither a graph nor new cache entries.
/// Supplied executable artifacts must implement the declared workspace products.
pub fn load_project_graph_with_generators(
    workspace_root: &Path,
    host: &mut crate::GeneratorHost<'_>,
) -> Result<ProjectGraph, ProjectLoadError> {
    load_project_graph_with_authority(workspace_root, PackageSourceAuthority::Ordinary, Some(host))
}

/// Loads a toolchain-owned standard-library workspace with an explicit sandbox host.
/// Every package must use the reserved standard-library namespace.
pub fn load_standard_library_project_graph_with_generators(
    workspace_root: &Path,
    host: &mut crate::GeneratorHost<'_>,
) -> Result<ProjectGraph, ProjectLoadError> {
    load_project_graph_with_authority(
        workspace_root,
        PackageSourceAuthority::StandardLibrary,
        Some(host),
    )
}

fn load_project_graph_with_authority(
    workspace_root: &Path,
    source_authority: PackageSourceAuthority,
    host: Option<&mut crate::GeneratorHost<'_>>,
) -> Result<ProjectGraph, ProjectLoadError> {
    let workspace_manifest_path = workspace_root.join(WORKSPACE_MANIFEST_FILE_NAME);
    let manifest_source = read_manifest_source(&workspace_manifest_path)?;
    let manifest = decode_workspace_manifest(&manifest_source, &workspace_manifest_path)?;

    let formatter_configuration = manifest
        .formatter_configuration
        .map(|path| {
            project_path(
                path,
                false,
                &workspace_manifest_path,
                Field::WorkspaceOutputRoot,
            )
        })
        .transpose()?;

    let output_root = project_path(
        manifest.output_root,
        false,
        &workspace_manifest_path,
        Field::WorkspaceOutputRoot,
    )?;

    let workspace_package_version = manifest
        .package
        .map(|package| workspace_package_version(package.version, &workspace_manifest_path))
        .transpose()?;

    let targets = load_targets(manifest.targets, &workspace_manifest_path)?;

    let packages = load_packages(
        workspace_root,
        manifest.packages,
        &targets,
        &output_root,
        &workspace_manifest_path,
        workspace_package_version.as_ref(),
        source_authority,
    )?;

    let mut graph = ProjectGraph::new(
        source_authority,
        formatter_configuration,
        output_root,
        targets,
        packages,
    );

    let rooted_failure = |mut error: ProjectLoadError| {
        let path = match &mut error {
            ProjectLoadError::Generator { path, .. }
            | ProjectLoadError::ReadManifest { path, .. }
            | ProjectLoadError::ParseManifest { path, .. }
            | ProjectLoadError::InvalidManifest { path, .. } => path,
        };

        *path = workspace_root.join(&*path);

        error
    };

    (graph.build_order, graph.build_plans) =
        super::generation::plan_project(&graph).map_err(&rooted_failure)?;

    graph.generated_sources = super::generation::generate_sources(workspace_root, &graph, host)
        .map_err(rooted_failure)?;

    Ok(graph)
}

fn load_targets(
    manifests: Vec<TargetManifest>,
    workspace_manifest_path: &Path,
) -> Result<Arc<[ProjectTarget]>, ProjectLoadError> {
    if manifests.is_empty() {
        return Err(ProjectLoadError::missing_selection(
            workspace_manifest_path.to_path_buf(),
            Field::WorkspaceTargets,
        ));
    }

    let mut targets = manifests
        .into_iter()
        .map(|target| {
            let name = local_name(target.name, workspace_manifest_path, Field::TargetName)?;

            let Some(identity) =
                TargetIdentity::try_new(Arc::<str>::from(target.identity.as_str()))
            else {
                return Err(ProjectLoadError::invalid_name(
                    workspace_manifest_path.to_path_buf(),
                    Field::TargetIdentity,
                    &target.identity,
                ));
            };

            let Some(target) = NativeTarget::for_identity(&identity) else {
                return Err(ProjectLoadError::unknown_target(
                    workspace_manifest_path.to_path_buf(),
                    Field::TargetIdentity,
                    &target.identity,
                ));
            };

            Ok(ProjectTarget::new(name, target.profile()))
        })
        .collect::<Result<Vec<_>, ProjectLoadError>>()?;

    targets.sort_unstable_by(|left, right| left.name().cmp(right.name()));

    if let Some(pair) = targets
        .windows(2)
        .find(|pair| pair[0].name() == pair[1].name())
    {
        return Err(ProjectLoadError::duplicate_selection(
            workspace_manifest_path.to_path_buf(),
            Field::TargetName,
            pair[0].name(),
        ));
    }

    for (index, target) in targets.iter().enumerate() {
        if targets[index + 1..]
            .iter()
            .any(|candidate| candidate.identity() == target.identity())
        {
            return Err(ProjectLoadError::duplicate_selection(
                workspace_manifest_path.to_path_buf(),
                Field::TargetIdentity,
                target.identity().as_str(),
            ));
        }
    }

    Ok(targets.into())
}

fn load_packages(
    workspace_root: &Path,
    mut selections: Vec<WorkspacePackageManifest>,
    targets: &[ProjectTarget],
    output_root: &ProjectPath,
    workspace_manifest_path: &Path,
    workspace_package_version: Option<&PackageVersion>,
    source_authority: PackageSourceAuthority,
) -> Result<Arc<[ProjectPackage]>, ProjectLoadError> {
    if selections.is_empty() {
        return Err(ProjectLoadError::missing_selection(
            workspace_manifest_path.to_path_buf(),
            Field::WorkspacePackages,
        ));
    }

    if !selections
        .iter()
        .any(|selection| matches!(selection.role, crate::manifest::PackageRoleManifest::Root))
    {
        return Err(ProjectLoadError::missing_root_package(
            workspace_manifest_path.to_path_buf(),
            Field::WorkspaceRootPackage,
        ));
    }

    selections.sort_unstable_by(|left, right| left.path.cmp(&right.path));

    if let Some(pair) = selections
        .windows(2)
        .find(|pair| pair[0].path == pair[1].path)
    {
        return Err(ProjectLoadError::duplicate_selection(
            workspace_manifest_path.to_path_buf(),
            Field::WorkspacePackagePath,
            &pair[0].path,
        ));
    }

    let mut packages = selections
        .into_iter()
        .map(|selection| {
            load_package(
                workspace_root,
                selection,
                targets,
                output_root,
                workspace_manifest_path,
                workspace_package_version,
                source_authority,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    packages.sort_unstable_by(|left, right| left.identity.cmp(&right.identity));

    if let Some(pair) = packages
        .windows(2)
        .find(|pair| pair[0].identity == pair[1].identity)
    {
        return Err(ProjectLoadError::duplicate_selection(
            pair[1].manifest_path.to_path_buf(),
            Field::PackageIdentity,
            pair[0].identity.as_str().to_owned(),
        ));
    }

    if source_authority.is_standard_library()
        && !packages.iter().any(|package| {
            is_public_standard_library_package(&package.identity)
                && package.role == crate::PackageRole::Root
        })
    {
        return Err(ProjectLoadError::standard_library_root_package_required(
            workspace_manifest_path.to_path_buf(),
            Field::WorkspaceRootPackage,
            "std",
        ));
    }

    validate_source_ownership(&packages)?;
    validate_dependencies(&packages)?;

    Ok(packages.into_iter().map(PendingPackage::finish).collect())
}

fn workspace_package_version(
    value: String,
    manifest_path: &Path,
) -> Result<PackageVersion, ProjectLoadError> {
    PackageVersion::try_new(&value).ok_or_else(|| {
        ProjectLoadError::invalid_package_version(
            manifest_path.to_path_buf(),
            Field::WorkspacePackageVersion,
            value,
        )
    })
}

fn validate_source_ownership(packages: &[PendingPackage]) -> Result<(), ProjectLoadError> {
    let roots = packages
        .iter()
        .flat_map(|package| {
            package.source_roots.iter().map(|root| {
                // Ownership validation compares canonical workspace-relative root paths.
                (package.path.joined(root.path()), &package.manifest_path)
            })
        })
        .collect::<Vec<_>>();

    for (index, (path, _)) in roots.iter().enumerate() {
        if let Some((_, manifest_path)) = roots[index + 1..]
            .iter()
            .find(|(candidate, _)| paths_overlap(path, candidate))
        {
            return Err(ProjectLoadError::invalid_source_root(
                (*manifest_path).to_path_buf(),
                Field::SourceRootPath,
                path.as_str().into(),
            ));
        }
    }

    Ok(())
}

fn validate_dependencies(packages: &[PendingPackage]) -> Result<(), ProjectLoadError> {
    for package in packages {
        for product in package.products.iter() {
            for dependency in product.dependencies() {
                let Some(target_package) = packages
                    .iter()
                    .find(|candidate| candidate.identity == *dependency.product().package())
                else {
                    return Err(ProjectLoadError::unknown_dependency_package(
                        package.manifest_path.to_path_buf(),
                        Field::DependencyPackage,
                        dependency.product().package().clone(),
                    ));
                };

                let Some(target_product) = target_package
                    .products
                    .iter()
                    .find(|candidate| candidate.identity() == dependency.product())
                else {
                    return Err(ProjectLoadError::unknown_dependency_product(
                        package.manifest_path.to_path_buf(),
                        Field::DependencyProduct,
                        dependency.product().clone(),
                    ));
                };

                if target_product.kind() != ProductKind::Library {
                    return Err(ProjectLoadError::dependency_product_not_library(
                        package.manifest_path.to_path_buf(),
                        Field::DependencyProduct,
                        dependency.product().clone(),
                    ));
                }

                if let Some(target) = dependency
                    .active_targets()
                    .iter()
                    .find(|target| !target_product.targets().contains(target))
                {
                    return Err(ProjectLoadError::dependency_target_unavailable(
                        package.manifest_path.to_path_buf(),
                        Field::DependencyProduct,
                        dependency.product().clone(),
                        (*target).clone(),
                    ));
                }
            }

            if let Some(tested_library) = product.tested_library() {
                let Some(library) = package
                    .products
                    .iter()
                    .find(|candidate| candidate.identity() == tested_library)
                else {
                    continue;
                };

                if let Some(target) = product
                    .targets()
                    .iter()
                    .find(|target| !library.targets().contains(target))
                {
                    return Err(ProjectLoadError::dependency_target_unavailable(
                        package.manifest_path.to_path_buf(),
                        Field::ProductTestedLibrary,
                        tested_library.clone(),
                        (*target).clone(),
                    ));
                }
            }
        }
    }

    Ok(())
}
