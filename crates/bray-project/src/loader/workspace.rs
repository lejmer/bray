use std::path::Path;
use std::sync::Arc;

use bray_standard_library::{PackageSourceAuthority, is_public_standard_library_package};
use bray_symbols::{PackageIdentity, PackageVersion, ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetIdentity};

use crate::manifest::{TargetManifest, WorkspacePackageManifest, decode_workspace_manifest};
use crate::{
    ProjectGraph, ProjectLoadError, ProjectManifestProblem, ProjectPackage, ProjectPath,
    ProjectProduct, ProjectTarget, ProjectTargetBuildPlan, WORKSPACE_MANIFEST_FILE_NAME,
};

use super::package::{PendingPackage, load_package};
use super::validation::{local_name, paths_overlap, project_path, read_manifest_source};

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
    let manifest_source = read_manifest_source(&workspace_manifest_path)?;
    let manifest = decode_workspace_manifest(&manifest_source, &workspace_manifest_path)?;

    let formatter_configuration = manifest
        .formatter_configuration
        .map(|path| project_path(path, false, &workspace_manifest_path))
        .transpose()?;

    let output_root = project_path(manifest.output_root, false, &workspace_manifest_path)?;

    let workspace_package_version = manifest
        .package
        .map(|package| workspace_package_version(package.version, &workspace_manifest_path))
        .transpose()?;

    let targets = load_targets(manifest.targets, &workspace_manifest_path)?;

    let (packages, build_plans) = load_packages(
        workspace_root,
        manifest.packages,
        &targets,
        &output_root,
        &workspace_manifest_path,
        workspace_package_version.as_ref(),
        source_authority,
    )?;

    Ok(ProjectGraph::new(
        source_authority,
        formatter_configuration,
        output_root,
        targets,
        packages,
        build_plans,
    ))
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

            let Some(target) = NativeTarget::for_identity(&identity) else {
                return Err(invalid_workspace(
                    workspace_manifest_path,
                    ProjectManifestProblem::UnknownTarget,
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
) -> Result<(Arc<[ProjectPackage]>, Arc<[ProjectTargetBuildPlan]>), ProjectLoadError> {
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
    validate_dependencies(&packages)?;

    let build_plans = build_plans(&packages, targets)?;
    let packages = packages.into_iter().map(PendingPackage::finish).collect();

    Ok((packages, build_plans))
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

fn validate_dependencies(packages: &[PendingPackage]) -> Result<(), ProjectLoadError> {
    for package in packages {
        for product in package.products.iter() {
            for dependency in product.dependencies() {
                let Some(target_package) = packages
                    .iter()
                    .find(|candidate| candidate.identity == *dependency.product().package())
                else {
                    return Err(ProjectLoadError::invalid(
                        package.manifest_path.to_path_buf(),
                        ProjectManifestProblem::UnknownDependencyPackage,
                        dependency.product().package().as_str(),
                    ));
                };

                let Some(target_product) = target_package
                    .products
                    .iter()
                    .find(|candidate| candidate.identity() == dependency.product())
                else {
                    return Err(ProjectLoadError::invalid(
                        package.manifest_path.to_path_buf(),
                        ProjectManifestProblem::UnknownDependencyProduct,
                        product_name(dependency.product()),
                    ));
                };

                if target_product.kind() != ProductKind::Library {
                    return Err(ProjectLoadError::invalid(
                        package.manifest_path.to_path_buf(),
                        ProjectManifestProblem::DependencyProductNotLibrary,
                        product_name(dependency.product()),
                    ));
                }

                if let Some(target) = dependency
                    .active_targets()
                    .iter()
                    .find(|target| !target_product.targets().contains(target))
                {
                    return Err(ProjectLoadError::invalid(
                        package.manifest_path.to_path_buf(),
                        ProjectManifestProblem::DependencyTargetUnavailable,
                        format!("{}/{}", product_name(dependency.product()), target.as_str()),
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
                    return Err(ProjectLoadError::invalid(
                        package.manifest_path.to_path_buf(),
                        ProjectManifestProblem::DependencyTargetUnavailable,
                        format!("{}/{}", product_name(tested_library), target.as_str()),
                    ));
                }
            }
        }
    }

    Ok(())
}

fn build_plans(
    packages: &[PendingPackage],
    targets: &[ProjectTarget],
) -> Result<Arc<[ProjectTargetBuildPlan]>, ProjectLoadError> {
    targets
        .iter()
        .map(|target| build_plan(packages, target.identity()))
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::from)
}

fn build_plan(
    packages: &[PendingPackage],
    target: &TargetIdentity,
) -> Result<ProjectTargetBuildPlan, ProjectLoadError> {
    let active_packages = packages
        .iter()
        .filter(|package| {
            package
                .products
                .iter()
                .any(|product| product.targets().contains(target))
        })
        .collect::<Vec<_>>();

    let active_products = packages
        .iter()
        .flat_map(|package| {
            package
                .products
                .iter()
                .map(move |product| (package, product))
        })
        .filter(|(_, product)| product.targets().contains(target))
        .collect::<Vec<_>>();

    let package_order = dependency_first_package_order(&active_packages, target)?;
    let product_order = dependency_first_product_order(&active_products, target)?;

    Ok(ProjectTargetBuildPlan::new(
        target.clone(),
        package_order.into(),
        product_order.into(),
    ))
}

fn dependency_first_package_order(
    packages: &[&PendingPackage],
    target: &TargetIdentity,
) -> Result<Vec<PackageIdentity>, ProjectLoadError> {
    let mut emitted = vec![false; packages.len()];
    let mut order = Vec::with_capacity(packages.len());

    while order.len() < packages.len() {
        let candidate = packages
            .iter()
            .enumerate()
            .filter(|(index, _)| !emitted[*index])
            .find(|(_, package)| {
                active_package_dependencies(package, target).all(|dependency| {
                    packages
                        .iter()
                        .position(|candidate| candidate.identity == *dependency)
                        .is_some_and(|index| emitted[index])
                })
            })
            .map(|(index, _)| index);

        let Some(index) = candidate else {
            let Some(package) = packages
                .iter()
                .enumerate()
                .find(|(index, _)| !emitted[*index])
                .map(|(_, package)| *package)
            else {
                return Ok(order);
            };

            return Err(ProjectLoadError::invalid(
                package.manifest_path.to_path_buf(),
                ProjectManifestProblem::DependencyCycle,
                package.identity.as_str(),
            ));
        };

        emitted[index] = true;
        order.push(packages[index].identity.clone());
    }

    Ok(order)
}

fn active_package_dependencies<'package>(
    package: &'package PendingPackage,
    target: &'package TargetIdentity,
) -> impl Iterator<Item = &'package PackageIdentity> {
    package
        .products
        .iter()
        .filter(move |product| product.targets().contains(target))
        .flat_map(ProjectProduct::dependencies)
        .filter(move |dependency| dependency.is_active_for(target))
        .map(|dependency| dependency.product().package())
}

fn dependency_first_product_order(
    products: &[(&PendingPackage, &ProjectProduct)],
    target: &TargetIdentity,
) -> Result<Vec<ProductIdentity>, ProjectLoadError> {
    let mut emitted = vec![false; products.len()];
    let mut order = Vec::with_capacity(products.len());

    while order.len() < products.len() {
        let candidate = products
            .iter()
            .enumerate()
            .filter(|(index, _)| !emitted[*index])
            .find(|(_, (_, product))| {
                active_product_dependencies(product, target).all(|dependency| {
                    products
                        .iter()
                        .position(|(_, candidate)| candidate.identity() == dependency)
                        .is_some_and(|index| emitted[index])
                })
            })
            .map(|(index, _)| index);

        let Some(index) = candidate else {
            let Some((package, product)) = products
                .iter()
                .enumerate()
                .find(|(index, _)| !emitted[*index])
                .map(|(_, product)| *product)
            else {
                return Ok(order);
            };

            return Err(ProjectLoadError::invalid(
                package.manifest_path.to_path_buf(),
                ProjectManifestProblem::DependencyCycle,
                product_name(product.identity()),
            ));
        };

        emitted[index] = true;
        order.push(products[index].1.identity().clone());
    }

    Ok(order)
}

fn active_product_dependencies<'product>(
    product: &'product ProjectProduct,
    target: &'product TargetIdentity,
) -> impl Iterator<Item = &'product ProductIdentity> {
    product
        .dependencies()
        .iter()
        .filter(move |dependency| dependency.is_active_for(target))
        .map(|dependency| dependency.product())
        .chain(product.tested_library())
}

fn product_name(product: &ProductIdentity) -> String {
    format!("{}/{}", product.package().as_str(), product.name())
}

fn invalid_workspace(
    path: &Path,
    problem: ProjectManifestProblem,
    value: &str,
) -> ProjectLoadError {
    ProjectLoadError::invalid(path.to_path_buf(), problem, value.to_owned())
}
