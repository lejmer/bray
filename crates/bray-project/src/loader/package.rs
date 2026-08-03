use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_standard_library::PackageSourceAuthority;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::{TargetIdentity, TargetOutputKind};

use crate::manifest::{
    DependencyManifest, OutputKindManifest, PackageManifest, PackageRoleManifest,
    ProductKindManifest, ProductManifest, SourceRootManifest, WorkspacePackageManifest,
};
use crate::{
    FeatureName, PackageRole, ProjectLoadError, ProjectManifestProblem, ProjectPackage,
    ProjectPath, ProjectProduct, ProjectSourceRoot, ProjectTarget,
};

use super::source::collect_sources;
use super::validation::{
    local_name, manifest_path, package_identity, paths_overlap, project_path, read_manifest,
    require_format, require_owned_package_path, sorted_unique_names,
};

pub(super) struct PendingDependency {
    pub package: PackageIdentity,
    pub product: Arc<str>,
}

pub(super) struct PendingPackage {
    pub identity: PackageIdentity,
    pub role: PackageRole,
    pub path: ProjectPath,
    pub manifest_path: PathBuf,
    pub declared_features: Arc<[FeatureName]>,
    pub enabled_features: Arc<[FeatureName]>,
    pub source_roots: Arc<[ProjectSourceRoot]>,
    pub dependencies: Box<[PendingDependency]>,
    pub products: Arc<[ProjectProduct]>,
}

impl PendingPackage {
    pub fn finish(self, dependencies: Arc<[crate::ProjectDependency]>) -> ProjectPackage {
        ProjectPackage::new(
            self.identity,
            self.role,
            self.path,
            self.declared_features,
            self.enabled_features,
            self.source_roots,
            dependencies,
            self.products,
        )
    }
}

pub(super) fn load_package(
    workspace_root: &Path,
    selection: WorkspacePackageManifest,
    targets: &[ProjectTarget],
    output_root: &ProjectPath,
    workspace_manifest_path: &Path,
    source_authority: PackageSourceAuthority,
) -> Result<PendingPackage, ProjectLoadError> {
    let package_path = project_path(selection.path, true, workspace_manifest_path)?;

    require_owned_package_path(workspace_root, &package_path, workspace_manifest_path)?;

    let package_root = package_path.beneath(workspace_root);
    let manifest_path = manifest_path(&package_root);
    let manifest: PackageManifest = read_manifest(&manifest_path)?;

    require_format(manifest.format, &manifest_path)?;

    let identity = package_identity(manifest.identity, &manifest_path, source_authority)?;
    let declared_feature_names = sorted_unique_names(manifest.features, &manifest_path)?;
    let enabled_feature_names = sorted_unique_names(selection.features, workspace_manifest_path)?;

    if let Some(feature) = enabled_feature_names
        .iter()
        .find(|feature| declared_feature_names.binary_search(feature).is_err())
    {
        return Err(ProjectLoadError::invalid(
            workspace_manifest_path.to_path_buf(),
            ProjectManifestProblem::UndeclaredFeature,
            feature.to_string(),
        ));
    }

    // Published feature identities retain validated immutable name storage.
    let declared_features = declared_feature_names
        .iter()
        .map(|feature| FeatureName::new(Arc::clone(feature)))
        .collect::<Vec<_>>()
        .into();

    let enabled_features = enabled_feature_names
        .iter()
        .map(|feature| FeatureName::new(Arc::clone(feature)))
        .collect::<Vec<_>>()
        .into();

    let source_roots = load_source_roots(
        workspace_root,
        &package_path,
        manifest.source_roots,
        output_root,
        &manifest_path,
    )?;

    let products = load_products(
        &identity,
        manifest.products,
        &source_roots,
        targets,
        &manifest_path,
    )?;

    let dependencies = load_dependencies(manifest.dependencies, &manifest_path, source_authority)?;

    let role = match selection.role {
        PackageRoleManifest::Root => PackageRole::Root,
        PackageRoleManifest::Vendored => PackageRole::Vendored,
    };

    Ok(PendingPackage {
        identity,
        role,
        path: package_path,
        manifest_path,
        declared_features,
        enabled_features,
        source_roots,
        dependencies,
        products,
    })
}

fn load_source_roots(
    workspace_root: &Path,
    package_path: &ProjectPath,
    manifests: Vec<SourceRootManifest>,
    output_root: &ProjectPath,
    manifest_path: &Path,
) -> Result<Arc<[ProjectSourceRoot]>, ProjectLoadError> {
    if manifests.is_empty() {
        return Err(missing_selection(manifest_path, "source_roots"));
    }

    let mut roots = manifests
        .into_iter()
        .map(|root| {
            let name = local_name(root.name, manifest_path)?;
            let path = project_path(root.path, false, manifest_path)?;
            let workspace_source_path = package_path.joined(&path);

            if paths_overlap(&workspace_source_path, output_root) {
                return Err(ProjectLoadError::invalid(
                    manifest_path.to_path_buf(),
                    ProjectManifestProblem::InvalidSourceRoot,
                    path.as_str().to_owned(),
                ));
            }

            let sources = Arc::from(collect_sources(
                workspace_root,
                package_path,
                &path,
                manifest_path,
            )?);

            Ok(ProjectSourceRoot::new(name, path, sources))
        })
        .collect::<Result<Vec<_>, ProjectLoadError>>()?;

    roots.sort_unstable_by(|left, right| left.name().cmp(right.name()));

    reject_duplicate(
        roots
            .windows(2)
            .find(|pair| pair[0].name() == pair[1].name()),
        manifest_path,
        |pair| pair[0].name(),
    )?;

    Ok(roots.into())
}

fn load_products(
    package: &PackageIdentity,
    manifests: Vec<ProductManifest>,
    source_roots: &[ProjectSourceRoot],
    targets: &[ProjectTarget],
    manifest_path: &Path,
) -> Result<Arc<[ProjectProduct]>, ProjectLoadError> {
    if manifests.is_empty() {
        return Err(missing_selection(manifest_path, "products"));
    }

    let mut products = manifests
        .into_iter()
        .map(|manifest| load_product(package, manifest, source_roots, targets, manifest_path))
        .collect::<Result<Vec<_>, _>>()?;

    products.sort_unstable_by(|left, right| left.identity().name().cmp(right.identity().name()));

    reject_duplicate(
        products
            .windows(2)
            .find(|pair| pair[0].identity().name() == pair[1].identity().name()),
        manifest_path,
        |pair| pair[0].identity().name(),
    )?;

    Ok(products.into())
}

fn load_product(
    package: &PackageIdentity,
    manifest: ProductManifest,
    source_roots: &[ProjectSourceRoot],
    targets: &[ProjectTarget],
    manifest_path: &Path,
) -> Result<ProjectProduct, ProjectLoadError> {
    let name = local_name(manifest.name, manifest_path)?;

    // Product identities retain the package's immutable Arc-backed canonical identity.
    let Some(identity) = ProductIdentity::try_new(package.clone(), Arc::clone(&name)) else {
        return Err(ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::InvalidName,
            name.to_string(),
        ));
    };

    let kind = product_kind(manifest.kind);
    let sources = select_sources(manifest.source_roots, source_roots, manifest_path)?;
    let targets = select_targets(manifest.targets, targets, manifest_path)?;
    let outputs = select_outputs(manifest.outputs, manifest_path)?;
    let platform_services = select_platform_services(manifest.platform_services, manifest_path)?;

    Ok(ProjectProduct::new(
        identity,
        kind,
        sources,
        targets,
        outputs,
        platform_services,
    ))
}

fn select_platform_services(
    manifests: Vec<crate::manifest::PlatformServiceManifest>,
    manifest_path: &Path,
) -> Result<Arc<[PlatformServiceBinding]>, ProjectLoadError> {
    let mut bindings = manifests
        .into_iter()
        .map(|manifest| {
            let Some(role) = PlatformServiceRole::from_name(&manifest.role) else {
                return Err(ProjectLoadError::invalid(
                    manifest_path.to_path_buf(),
                    ProjectManifestProblem::InvalidName,
                    manifest.role,
                ));
            };

            PlatformServiceBinding::try_new(role, &manifest.declaration).ok_or_else(|| {
                ProjectLoadError::invalid(
                    manifest_path.to_path_buf(),
                    ProjectManifestProblem::InvalidName,
                    manifest.declaration,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    bindings.sort_unstable();

    if let Some(duplicate) = bindings.windows(2).find(|pair| {
        pair[0].role() == pair[1].role() || pair[0].dotted_path() == pair[1].dotted_path()
    }) {
        return Err(ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::DuplicateSelection,
            duplicate[1].dotted_path(),
        ));
    }

    Ok(bindings.into())
}

fn select_sources(
    selections: Vec<String>,
    roots: &[ProjectSourceRoot],
    manifest_path: &Path,
) -> Result<Arc<[ProjectPath]>, ProjectLoadError> {
    let names = require_names(selections, manifest_path, "product.source_roots")?;
    let mut sources = Vec::new();

    for name in names.iter() {
        let Some(root) = roots.iter().find(|root| root.name() == name.as_ref()) else {
            return Err(ProjectLoadError::invalid(
                manifest_path.to_path_buf(),
                ProjectManifestProblem::UnknownSourceRoot,
                name.to_string(),
            ));
        };

        // Product source lists retain the roots' immutable, Arc-backed portable paths.
        sources.extend(root.sources().iter().cloned());
    }

    Ok(sorted_unique_shared_slice(sources))
}

fn select_targets(
    selections: Vec<String>,
    targets: &[ProjectTarget],
    manifest_path: &Path,
) -> Result<Arc<[TargetIdentity]>, ProjectLoadError> {
    let names = require_names(selections, manifest_path, "product.targets")?;
    let mut identities = Vec::with_capacity(names.len());

    for name in names.iter() {
        let Some(target) = targets.iter().find(|target| target.name() == name.as_ref()) else {
            return Err(ProjectLoadError::invalid(
                manifest_path.to_path_buf(),
                ProjectManifestProblem::UnknownTarget,
                name.to_string(),
            ));
        };

        // Product selections retain immutable Arc-backed target identities.
        identities.push(target.identity().clone());
    }

    identities.sort_unstable();

    Ok(identities.into())
}

fn select_outputs(
    selections: Vec<OutputKindManifest>,
    manifest_path: &Path,
) -> Result<Arc<[TargetOutputKind]>, ProjectLoadError> {
    if selections.is_empty() {
        return Err(missing_selection(manifest_path, "product.outputs"));
    }

    let mut outputs: Vec<_> = selections.into_iter().map(output_kind).collect();
    outputs.sort_unstable();

    if let Some(output) = outputs.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::DuplicateSelection,
            output_kind_name(output[0]).to_owned(),
        ));
    }

    Ok(outputs.into())
}

fn load_dependencies(
    manifests: Vec<DependencyManifest>,
    manifest_path: &Path,
    source_authority: PackageSourceAuthority,
) -> Result<Box<[PendingDependency]>, ProjectLoadError> {
    let mut dependencies = manifests
        .into_iter()
        .map(|dependency| {
            Ok(PendingDependency {
                package: package_identity(dependency.package, manifest_path, source_authority)?,
                product: local_name(dependency.product, manifest_path)?,
            })
        })
        .collect::<Result<Vec<_>, ProjectLoadError>>()?;

    dependencies.sort_unstable_by(|left, right| {
        (&left.package, &left.product).cmp(&(&right.package, &right.product))
    });

    if let Some(pair) = dependencies
        .windows(2)
        .find(|pair| pair[0].package == pair[1].package && pair[0].product == pair[1].product)
    {
        return Err(ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::DuplicateSelection,
            format!("{}/{}", pair[0].package.as_str(), pair[0].product),
        ));
    }

    Ok(dependencies.into_boxed_slice())
}

fn require_names(
    names: Vec<String>,
    manifest_path: &Path,
    field: &str,
) -> Result<Arc<[Arc<str>]>, ProjectLoadError> {
    if names.is_empty() {
        return Err(missing_selection(manifest_path, field));
    }

    sorted_unique_names(names, manifest_path)
}

fn missing_selection(manifest_path: &Path, field: &str) -> ProjectLoadError {
    ProjectLoadError::invalid(
        manifest_path.to_path_buf(),
        ProjectManifestProblem::MissingSelection,
        field.to_owned(),
    )
}

fn reject_duplicate<T>(
    duplicate: Option<&[T]>,
    manifest_path: &Path,
    value: impl FnOnce(&[T]) -> &str,
) -> Result<(), ProjectLoadError> {
    let Some(duplicate) = duplicate else {
        return Ok(());
    };

    Err(ProjectLoadError::invalid(
        manifest_path.to_path_buf(),
        ProjectManifestProblem::DuplicateSelection,
        value(duplicate).to_owned(),
    ))
}

const fn product_kind(kind: ProductKindManifest) -> ProductKind {
    match kind {
        ProductKindManifest::Executable => ProductKind::Executable,
        ProductKindManifest::Library => ProductKind::Library,
        ProductKindManifest::Test => ProductKind::Test,
    }
}

const fn output_kind(kind: OutputKindManifest) -> TargetOutputKind {
    match kind {
        OutputKindManifest::Assembly => TargetOutputKind::Assembly,
        OutputKindManifest::BackendIr => TargetOutputKind::BackendIr,
        OutputKindManifest::BackendBitcode => TargetOutputKind::BackendBitcode,
        OutputKindManifest::RelocatableObject => TargetOutputKind::RelocatableObject,
        OutputKindManifest::ExecutableModule => TargetOutputKind::ExecutableModule,
        OutputKindManifest::DebugCompanion => TargetOutputKind::DebugCompanion,
        OutputKindManifest::PackageInterface => TargetOutputKind::PackageInterface,
        OutputKindManifest::DependencyMetadata => TargetOutputKind::DependencyMetadata,
        OutputKindManifest::Executable => TargetOutputKind::Executable,
        OutputKindManifest::StaticLibrary => TargetOutputKind::StaticLibrary,
        OutputKindManifest::SharedLibrary => TargetOutputKind::SharedLibrary,
        OutputKindManifest::LinkedCompanion => TargetOutputKind::LinkedCompanion,
    }
}

const fn output_kind_name(kind: TargetOutputKind) -> &'static str {
    match kind {
        TargetOutputKind::Assembly => "assembly",
        TargetOutputKind::BackendIr => "backend_ir",
        TargetOutputKind::BackendBitcode => "backend_bitcode",
        TargetOutputKind::RelocatableObject => "relocatable_object",
        TargetOutputKind::ExecutableModule => "executable_module",
        TargetOutputKind::DebugCompanion => "debug_companion",
        TargetOutputKind::PackageInterface => "package_interface",
        TargetOutputKind::DependencyMetadata => "dependency_metadata",
        TargetOutputKind::Executable => "executable",
        TargetOutputKind::StaticLibrary => "static_library",
        TargetOutputKind::SharedLibrary => "shared_library",
        TargetOutputKind::LinkedCompanion => "linked_companion",
    }
}
