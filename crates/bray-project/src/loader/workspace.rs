use std::path::Path;
use std::sync::Arc;

use bray_standard_library::{PackageSourceAuthority, is_public_standard_library_package};
use bray_symbols::{PackageVersion, ProductKind};
use bray_target::TargetIdentity;

use crate::manifest::{TargetManifest, WorkspaceManifest, WorkspacePackageManifest};
use crate::{
    ProjectDependency, ProjectGraph, ProjectLoadError, ProjectManifestProblem, ProjectPackage,
    ProjectPath, ProjectTarget, WORKSPACE_MANIFEST_FILE_NAME,
};

use super::package::{PendingPackage, load_package};
use super::validation::{local_name, paths_overlap, project_path, read_manifest, require_format};

/// Loads one explicit Bray workspace into an immutable dependency-first graph.
///
/// The loader reads only the fixed workspace manifest, package manifests in the
/// workspace's explicit package inventory, and `.bray` files beneath declared
/// source roots. It performs no search, acquisition, registry access, or network I/O.
pub fn load_project_graph(workspace_root: &Path) -> Result<ProjectGraph, ProjectLoadError> {
    load_project_graph_with_authority(workspace_root, PackageSourceAuthority::Ordinary)
}

/// Loads one toolchain-owned standard library workspace.
///
/// Every package in the workspace must use the reserved `std` namespace. This
/// authority does not change source-language trust or semantic checking rules.
pub fn load_standard_library_project_graph(
    workspace_root: &Path,
) -> Result<ProjectGraph, ProjectLoadError> {
    load_project_graph_with_authority(workspace_root, PackageSourceAuthority::StandardLibrary)
}

fn load_project_graph_with_authority(
    workspace_root: &Path,
    source_authority: PackageSourceAuthority,
) -> Result<ProjectGraph, ProjectLoadError> {
    let workspace_manifest_path = workspace_root.join(WORKSPACE_MANIFEST_FILE_NAME);
    let manifest: WorkspaceManifest = read_manifest(&workspace_manifest_path)?;

    require_format(manifest.format, &workspace_manifest_path)?;

    let output_root = project_path(manifest.output_root, false, &workspace_manifest_path)?;

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

    Ok(ProjectGraph::new(output_root, targets, packages))
}

fn load_targets(
    manifests: Vec<TargetManifest>,
    workspace_manifest_path: &Path,
) -> Result<Arc<[ProjectTarget]>, ProjectLoadError> {
    if manifests.is_empty() {
        return Err(invalid_workspace(
            workspace_manifest_path,
            ProjectManifestProblem::MissingSelection,
            "targets",
        ));
    }

    let mut targets = manifests
        .into_iter()
        .map(|target| {
            let name = local_name(target.name, workspace_manifest_path)?;

            let Some(identity) =
                TargetIdentity::try_new(Arc::<str>::from(target.identity.as_str()))
            else {
                return Err(invalid_workspace(
                    workspace_manifest_path,
                    ProjectManifestProblem::InvalidName,
                    &target.identity,
                ));
            };

            Ok(ProjectTarget::new(name, identity))
        })
        .collect::<Result<Vec<_>, ProjectLoadError>>()?;

    targets.sort_unstable_by(|left, right| left.name().cmp(right.name()));

    if let Some(pair) = targets
        .windows(2)
        .find(|pair| pair[0].name() == pair[1].name())
    {
        return Err(invalid_workspace(
            workspace_manifest_path,
            ProjectManifestProblem::DuplicateSelection,
            pair[0].name(),
        ));
    }

    for (index, target) in targets.iter().enumerate() {
        if targets[index + 1..]
            .iter()
            .any(|candidate| candidate.identity() == target.identity())
        {
            return Err(invalid_workspace(
                workspace_manifest_path,
                ProjectManifestProblem::DuplicateSelection,
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
        return Err(invalid_workspace(
            workspace_manifest_path,
            ProjectManifestProblem::MissingSelection,
            "packages",
        ));
    }

    if !selections
        .iter()
        .any(|selection| matches!(selection.role, crate::manifest::PackageRoleManifest::Root))
    {
        return Err(invalid_workspace(
            workspace_manifest_path,
            ProjectManifestProblem::MissingRootPackage,
            "packages",
        ));
    }

    selections.sort_unstable_by(|left, right| left.path.cmp(&right.path));

    if let Some(pair) = selections
        .windows(2)
        .find(|pair| pair[0].path == pair[1].path)
    {
        return Err(invalid_workspace(
            workspace_manifest_path,
            ProjectManifestProblem::DuplicateSelection,
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
        return Err(ProjectLoadError::invalid(
            pair[1].manifest_path.to_path_buf(),
            ProjectManifestProblem::DuplicateSelection,
            pair[0].identity.as_str().to_owned(),
        ));
    }

    if source_authority.is_standard_library()
        && !packages.iter().any(|package| {
            is_public_standard_library_package(&package.identity)
                && package.role == crate::PackageRole::Root
        })
    {
        return Err(ProjectLoadError::invalid(
            workspace_manifest_path.to_path_buf(),
            ProjectManifestProblem::StandardLibraryRootPackageRequired,
            "std",
        ));
    }

    validate_source_ownership(&packages)?;

    let dependency_indices = resolve_dependencies(&packages)?;
    let build_order = dependency_first_order(&packages, &dependency_indices)?;
    let dependencies = materialize_dependencies(&packages, &dependency_indices)?;

    let mut packages: Vec<_> = packages
        .into_iter()
        .zip(dependencies)
        .map(|(package, dependencies)| Some(package.finish(dependencies)))
        .collect();

    let mut ordered = Vec::with_capacity(packages.len());

    for index in build_order {
        let Some(package) = packages.get_mut(index).and_then(Option::take) else {
            return Err(invalid_workspace(
                workspace_manifest_path,
                ProjectManifestProblem::DependencyCycle,
                "packages",
            ));
        };

        ordered.push(package);
    }

    Ok(ordered.into())
}

fn workspace_package_version(
    value: String,
    manifest_path: &Path,
) -> Result<PackageVersion, ProjectLoadError> {
    PackageVersion::try_new(&value).ok_or_else(|| {
        ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::InvalidPackageVersion,
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
            return Err(ProjectLoadError::invalid(
                (*manifest_path).to_path_buf(),
                ProjectManifestProblem::InvalidSourceRoot,
                path.as_str().to_owned(),
            ));
        }
    }

    Ok(())
}

fn resolve_dependencies(
    packages: &[PendingPackage],
) -> Result<Vec<Box<[usize]>>, ProjectLoadError> {
    packages
        .iter()
        .map(|package| {
            package
                .dependencies
                .iter()
                .map(|dependency| {
                    packages
                        .binary_search_by(|candidate| candidate.identity.cmp(&dependency.package))
                        .map_err(|_| {
                            ProjectLoadError::invalid(
                                package.manifest_path.to_path_buf(),
                                ProjectManifestProblem::UnknownDependencyPackage,
                                dependency.package.as_str().to_owned(),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Vec::into_boxed_slice)
        })
        .collect()
}

fn materialize_dependencies(
    packages: &[PendingPackage],
    dependency_indices: &[Box<[usize]>],
) -> Result<Vec<Arc<[ProjectDependency]>>, ProjectLoadError> {
    packages
        .iter()
        .zip(dependency_indices)
        .map(|(package, indices)| {
            package
                .dependencies
                .iter()
                .zip(indices)
                .map(|(dependency, &index)| {
                    let target = &packages[index];

                    let Some(product) = target
                        .products
                        .iter()
                        .find(|product| product.identity().name() == dependency.product.as_ref())
                    else {
                        return Err(ProjectLoadError::invalid(
                            package.manifest_path.to_path_buf(),
                            ProjectManifestProblem::UnknownDependencyProduct,
                            format!("{}/{}", dependency.package.as_str(), dependency.product),
                        ));
                    };

                    if product.kind() != ProductKind::Library {
                        return Err(ProjectLoadError::invalid(
                            package.manifest_path.to_path_buf(),
                            ProjectManifestProblem::DependencyProductNotLibrary,
                            format!("{}/{}", dependency.package.as_str(), dependency.product),
                        ));
                    }

                    // Dependency edges retain immutable Arc-backed product identities.
                    Ok(ProjectDependency::new(product.identity().clone()))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Arc::from)
        })
        .collect()
}

fn dependency_first_order(
    packages: &[PendingPackage],
    dependency_indices: &[Box<[usize]>],
) -> Result<Box<[usize]>, ProjectLoadError> {
    let mut emitted = vec![false; packages.len()];
    let mut order = Vec::with_capacity(packages.len());

    while order.len() < packages.len() {
        let candidate = packages
            .iter()
            .enumerate()
            .filter(|(index, _)| !emitted[*index])
            .filter(|(index, _)| {
                dependency_indices[*index]
                    .iter()
                    .all(|dependency| emitted[*dependency])
            })
            .min_by(|left, right| left.1.identity.cmp(&right.1.identity))
            .map(|(index, _)| index);

        let Some(index) = candidate else {
            let Some((_, package)) = packages
                .iter()
                .enumerate()
                .filter(|(index, _)| !emitted[*index])
                .min_by(|left, right| left.1.identity.cmp(&right.1.identity))
            else {
                return Ok(order.into_boxed_slice());
            };

            return Err(ProjectLoadError::invalid(
                package.manifest_path.to_path_buf(),
                ProjectManifestProblem::DependencyCycle,
                package.identity.as_str().to_owned(),
            ));
        };

        emitted[index] = true;
        order.push(index);
    }

    Ok(order.into_boxed_slice())
}

fn invalid_workspace(
    path: &Path,
    problem: ProjectManifestProblem,
    value: &str,
) -> ProjectLoadError {
    ProjectLoadError::invalid(path.to_path_buf(), problem, value.to_owned())
}
