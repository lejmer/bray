use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_diagnostics::DiagnosticProjectManifestField as Field;
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_standard_library::PackageSourceAuthority;
use bray_symbols::{PackageIdentity, PackageVersion, ProductIdentity, ProductKind};
use bray_target::{TargetIdentity, TargetOutputKind};

use crate::manifest::{
    DependencyManifest, OutputKindManifest, PackageRoleManifest, PackageVersionManifest,
    ProductKindManifest, ProductManifest, SourceRootManifest, WorkspacePackageManifest,
    decode_package_manifest,
};
use crate::{
    FeatureName, PackageRole, ProjectDependency, ProjectLoadError, ProjectPackage, ProjectPath,
    ProjectProduct, ProjectSourceRoot, ProjectTarget,
};

use super::predicate::normalize_target_predicate;
use super::source::collect_sources;
use super::validation::{
    local_name, manifest_path, package_identity, paths_overlap, project_path, read_manifest_source,
    require_owned_package_path, sorted_unique_names,
};

pub(super) struct PendingPackage {
    pub identity: PackageIdentity,
    pub version: PackageVersion,
    pub role: PackageRole,
    pub path: ProjectPath,
    pub manifest_path: PathBuf,
    pub declared_features: Arc<[FeatureName]>,
    pub enabled_features: Arc<[FeatureName]>,
    pub source_roots: Arc<[ProjectSourceRoot]>,
    pub products: Arc<[ProjectProduct]>,
}

impl PendingPackage {
    pub fn finish(self) -> ProjectPackage {
        ProjectPackage::new(
            self.identity,
            self.version,
            self.role,
            self.path,
            self.declared_features,
            self.enabled_features,
            self.source_roots,
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
    workspace_package_version: Option<&PackageVersion>,
    source_authority: PackageSourceAuthority,
) -> Result<PendingPackage, ProjectLoadError> {
    let package_path = project_path(
        selection.path,
        true,
        workspace_manifest_path,
        Field::WorkspacePackagePath,
    )?;

    require_owned_package_path(workspace_root, &package_path, workspace_manifest_path)?;

    let package_root = package_path.beneath(workspace_root);
    let manifest_path = manifest_path(&package_root);
    let manifest_source = read_manifest_source(&manifest_path)?;
    let manifest = decode_package_manifest(&manifest_source, &manifest_path)?;

    let identity = package_identity(
        manifest.identity,
        &manifest_path,
        source_authority,
        Field::PackageIdentity,
    )?;

    let version = package_version(manifest.version, workspace_package_version, &manifest_path)?;

    let declared_feature_names =
        sorted_unique_names(manifest.features, &manifest_path, Field::PackageFeatures)?;

    let enabled_feature_names = sorted_unique_names(
        selection.features,
        workspace_manifest_path,
        Field::PackageFeatures,
    )?;

    if let Some(feature) = enabled_feature_names
        .iter()
        .find(|feature| declared_feature_names.binary_search(feature).is_err())
    {
        return Err(ProjectLoadError::undeclared_feature(
            workspace_manifest_path.to_path_buf(),
            Field::PackageFeatures,
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
        source_authority,
    )?;

    let role = match selection.role {
        PackageRoleManifest::Root => PackageRole::Root,
        PackageRoleManifest::Vendored => PackageRole::Vendored,
    };

    Ok(PendingPackage {
        identity,
        version,
        role,
        path: package_path,
        manifest_path,
        declared_features,
        enabled_features,
        source_roots,
        products,
    })
}

fn package_version(
    manifest: PackageVersionManifest,
    workspace_version: Option<&PackageVersion>,
    manifest_path: &Path,
) -> Result<PackageVersion, ProjectLoadError> {
    match manifest {
        PackageVersionManifest::Explicit(value) => {
            PackageVersion::try_new(&value).ok_or_else(|| {
                ProjectLoadError::invalid_package_version(
                    manifest_path.to_path_buf(),
                    Field::PackageVersion,
                    value,
                )
            })
        }
        PackageVersionManifest::Inherited(inherited) if inherited.workspace => {
            // Package nodes share the immutable Arc-backed workspace version.
            workspace_version.cloned().ok_or_else(|| {
                ProjectLoadError::missing_workspace_package_version(
                    manifest_path.to_path_buf(),
                    Field::WorkspacePackageVersion,
                )
            })
        }
        PackageVersionManifest::Inherited(_) => Err(ProjectLoadError::invalid_package_version(
            manifest_path.to_path_buf(),
            Field::PackageVersion,
            "workspace",
        )),
    }
}

fn load_source_roots(
    workspace_root: &Path,
    package_path: &ProjectPath,
    manifests: Vec<SourceRootManifest>,
    output_root: &ProjectPath,
    manifest_path: &Path,
) -> Result<Arc<[ProjectSourceRoot]>, ProjectLoadError> {
    if manifests.is_empty() {
        return Err(missing_selection(manifest_path, Field::PackageSourceRoots));
    }

    let mut roots = manifests
        .into_iter()
        .map(|root| {
            let name = local_name(root.name, manifest_path, Field::SourceRootName)?;
            let path = project_path(root.path, false, manifest_path, Field::SourceRootPath)?;
            let workspace_source_path = package_path.joined(&path);

            if paths_overlap(&workspace_source_path, output_root) {
                return Err(ProjectLoadError::invalid_source_root(
                    manifest_path.to_path_buf(),
                    Field::SourceRootPath,
                    path.as_str().into(),
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
        Field::SourceRootName,
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
    source_authority: PackageSourceAuthority,
) -> Result<Arc<[ProjectProduct]>, ProjectLoadError> {
    if manifests.is_empty() {
        return Err(missing_selection(manifest_path, Field::PackageProducts));
    }

    let mut products = manifests
        .into_iter()
        .map(|manifest| {
            load_product(
                package,
                manifest,
                source_roots,
                targets,
                manifest_path,
                source_authority,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    products.sort_unstable_by(|left, right| left.identity().name().cmp(right.identity().name()));

    reject_duplicate(
        products
            .windows(2)
            .find(|pair| pair[0].identity().name() == pair[1].identity().name()),
        manifest_path,
        Field::ProductIdentity,
        |pair| pair[0].identity().name(),
    )?;

    validate_tested_libraries(&products, manifest_path)?;

    Ok(products.into())
}

fn validate_tested_libraries(
    products: &[ProjectProduct],
    manifest_path: &Path,
) -> Result<(), ProjectLoadError> {
    for product in products {
        let Some(tested_library) = product.tested_library() else {
            continue;
        };

        if product.kind() != ProductKind::Test {
            return Err(ProjectLoadError::tested_library_on_non_test_product(
                manifest_path.to_path_buf(),
                Field::ProductTestedLibrary,
                product.identity().name(),
            ));
        }

        let Some(library) = products
            .iter()
            .find(|candidate| candidate.identity() == tested_library)
        else {
            return Err(ProjectLoadError::unknown_dependency_product(
                manifest_path.to_path_buf(),
                Field::ProductTestedLibrary,
                tested_library.clone(),
            ));
        };

        if library.kind() != ProductKind::Library {
            return Err(ProjectLoadError::dependency_product_not_library(
                manifest_path.to_path_buf(),
                Field::ProductTestedLibrary,
                tested_library.clone(),
            ));
        }
    }

    Ok(())
}

fn load_product(
    package: &PackageIdentity,
    manifest: ProductManifest,
    source_roots: &[ProjectSourceRoot],
    targets: &[ProjectTarget],
    manifest_path: &Path,
    source_authority: PackageSourceAuthority,
) -> Result<ProjectProduct, ProjectLoadError> {
    let name = local_name(manifest.name, manifest_path, Field::ProductIdentity)?;

    // Product identities retain the package's immutable Arc-backed canonical identity.
    let Some(identity) = ProductIdentity::try_new(package.clone(), Arc::clone(&name)) else {
        return Err(ProjectLoadError::invalid_name(
            manifest_path.to_path_buf(),
            Field::ProductIdentity,
            name.to_string(),
        ));
    };

    let kind = product_kind(manifest.kind);

    let tested_library = manifest
        .tested_library
        .map(|name| {
            let name = local_name(name, manifest_path, Field::ProductTestedLibrary)?;

            ProductIdentity::try_new(package.clone(), name.clone()).ok_or_else(|| {
                ProjectLoadError::invalid_name(
                    manifest_path.to_path_buf(),
                    Field::ProductTestedLibrary,
                    name.to_string(),
                )
            })
        })
        .transpose()?;

    let sources = select_sources(manifest.source_roots, source_roots, manifest_path)?;
    let selected_targets = select_targets(manifest.targets, targets, manifest_path)?;

    let dependencies = load_dependencies(
        manifest.dependencies,
        &selected_targets,
        targets,
        manifest_path,
        source_authority,
    )?;

    let outputs = select_outputs(manifest.outputs, manifest_path)?;
    let platform_services = select_platform_services(manifest.platform_services, manifest_path)?;

    Ok(ProjectProduct::new(
        identity,
        kind,
        tested_library,
        dependencies,
        sources,
        selected_targets,
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
                return Err(ProjectLoadError::invalid_name(
                    manifest_path.to_path_buf(),
                    Field::PlatformServiceRole,
                    manifest.role,
                ));
            };

            PlatformServiceBinding::try_new(role, &manifest.declaration).ok_or_else(|| {
                ProjectLoadError::invalid_name(
                    manifest_path.to_path_buf(),
                    Field::PlatformServiceDeclaration,
                    manifest.declaration,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    bindings.sort_unstable();

    if let Some(duplicate) = bindings.windows(2).find(|pair| {
        pair[0].role() == pair[1].role() || pair[0].dotted_path() == pair[1].dotted_path()
    }) {
        return Err(ProjectLoadError::duplicate_selection(
            manifest_path.to_path_buf(),
            Field::PackagePlatformServices,
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
    let names = require_names(selections, manifest_path, Field::ProductSourceRoots)?;
    let mut sources = Vec::new();

    for name in names.iter() {
        let root = select_named(
            roots,
            name,
            ProjectSourceRoot::name,
            manifest_path,
            Field::ProductSourceRoots,
            ProjectLoadError::unknown_source_root,
        )?;

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
    let names = require_names(selections, manifest_path, Field::ProductTargets)?;
    let mut identities = Vec::with_capacity(names.len());

    for name in names.iter() {
        let target = select_named(
            targets,
            name,
            ProjectTarget::name,
            manifest_path,
            Field::ProductTargets,
            ProjectLoadError::unknown_target,
        )?;

        // Product selections retain immutable Arc-backed target identities.
        identities.push(target.identity().clone());
    }

    identities.sort_unstable();

    Ok(identities.into())
}

fn select_named<'a, T>(
    values: &'a [T],
    name: &str,
    value_name: impl Fn(&T) -> &str,
    manifest_path: &Path,
    field: Field,
    missing: impl FnOnce(PathBuf, Field, String) -> ProjectLoadError,
) -> Result<&'a T, ProjectLoadError> {
    values
        .iter()
        .find(|value| value_name(value) == name)
        .ok_or_else(|| missing(manifest_path.to_path_buf(), field, name.to_string()))
}

fn select_outputs(
    selections: Vec<OutputKindManifest>,
    manifest_path: &Path,
) -> Result<Arc<[TargetOutputKind]>, ProjectLoadError> {
    if selections.is_empty() {
        return Err(missing_selection(manifest_path, Field::ProductOutputs));
    }

    let mut outputs: Vec<_> = selections.into_iter().map(output_kind).collect();
    outputs.sort_unstable();

    if let Some(output) = outputs.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ProjectLoadError::duplicate_selection(
            manifest_path.to_path_buf(),
            Field::ProductOutputs,
            output_kind_name(output[0]).to_owned(),
        ));
    }

    Ok(outputs.into())
}

fn load_dependencies(
    manifests: Vec<DependencyManifest>,
    selected_targets: &[TargetIdentity],
    targets: &[ProjectTarget],
    manifest_path: &Path,
    source_authority: PackageSourceAuthority,
) -> Result<Arc<[ProjectDependency]>, ProjectLoadError> {
    let mut dependencies = manifests
        .into_iter()
        .map(|dependency| {
            let package = package_identity(
                dependency.package,
                manifest_path,
                source_authority,
                Field::DependencyPackage,
            )?;

            let product = local_name(dependency.product, manifest_path, Field::DependencyProduct)?;

            let Some(product) = ProductIdentity::try_new(package, product) else {
                return Err(ProjectLoadError::invalid_name(
                    manifest_path.to_path_buf(),
                    Field::DependencyProduct,
                    "dependency.product",
                ));
            };

            let predicate = dependency
                .when
                .map(|predicate| {
                    normalize_target_predicate(predicate, selected_targets, targets, manifest_path)
                })
                .transpose()?;

            let mut property_dependencies = Vec::new();

            if let Some(predicate) = &predicate {
                predicate.collect_properties(&mut property_dependencies);
            }

            let property_dependencies = sorted_unique_shared_slice(property_dependencies);

            let active_targets = selected_targets
                .iter()
                .filter(|target| {
                    predicate.as_ref().is_none_or(|predicate| {
                        targets
                            .iter()
                            .find(|candidate| candidate.identity() == *target)
                            .is_some_and(|target| predicate.evaluate(target.profile()))
                    })
                })
                .cloned()
                .collect::<Vec<_>>()
                .into();

            Ok(ProjectDependency::new(
                product,
                predicate,
                property_dependencies,
                active_targets,
            ))
        })
        .collect::<Result<Vec<_>, ProjectLoadError>>()?;

    dependencies.sort_unstable();

    if let Some(pair) = dependencies
        .windows(2)
        .find(|pair| pair[0].product() == pair[1].product())
    {
        return Err(ProjectLoadError::duplicate_selection(
            manifest_path.to_path_buf(),
            Field::ProductDependencies,
            format!(
                "{}/{}",
                pair[0].product().package().as_str(),
                pair[0].product().name()
            ),
        ));
    }

    Ok(dependencies.into())
}

fn require_names(
    names: Vec<String>,
    manifest_path: &Path,
    field: Field,
) -> Result<Arc<[Arc<str>]>, ProjectLoadError> {
    if names.is_empty() {
        return Err(missing_selection(manifest_path, field));
    }

    sorted_unique_names(names, manifest_path, field)
}

fn missing_selection(manifest_path: &Path, field: Field) -> ProjectLoadError {
    ProjectLoadError::missing_selection(manifest_path.to_path_buf(), field)
}

fn reject_duplicate<T>(
    duplicate: Option<&[T]>,
    manifest_path: &Path,
    field: Field,
    value: impl FnOnce(&[T]) -> &str,
) -> Result<(), ProjectLoadError> {
    let Some(duplicate) = duplicate else {
        return Ok(());
    };

    Err(ProjectLoadError::duplicate_selection(
        manifest_path.to_path_buf(),
        field,
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
        OutputKindManifest::PackageImplementation => TargetOutputKind::PackageImplementation,
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
        TargetOutputKind::PackageImplementation => "package_implementation",
        TargetOutputKind::DependencyMetadata => "dependency_metadata",
        TargetOutputKind::TestCatalog => "test_catalog",
        TargetOutputKind::Executable => "executable",
        TargetOutputKind::StaticLibrary => "static_library",
        TargetOutputKind::SharedLibrary => "shared_library",
        TargetOutputKind::LinkedCompanion => "linked_companion",
    }
}
