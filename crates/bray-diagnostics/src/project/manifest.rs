/// Exact field in a Bray workspace or package manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectManifestField {
    /// Workspace or package manifest file path.
    ManifestPath,
    /// Workspace manifest format revision.
    WorkspaceFormat,
    /// Workspace output-root path.
    WorkspaceOutputRoot,
    /// Workspace target declarations.
    WorkspaceTargets,
    /// Workspace package selections.
    WorkspacePackages,
    /// Workspace root-package selection.
    WorkspaceRootPackage,
    /// Workspace-wide inherited package version.
    WorkspacePackageVersion,
    /// Workspace package path.
    WorkspacePackagePath,
    /// Package identity.
    PackageIdentity,
    /// Package version declaration.
    PackageVersion,
    /// Package feature declarations or selections.
    PackageFeatures,
    /// Package source-root declarations.
    PackageSourceRoots,
    /// Package product declarations.
    PackageProducts,
    /// Product identity or local name.
    ProductIdentity,
    /// Product source-root selections.
    ProductSourceRoots,
    /// Product target selections.
    ProductTargets,
    /// Product output selections.
    ProductOutputs,
    /// Product dependency declarations.
    ProductDependencies,
    /// Product tested-library selection.
    ProductTestedLibrary,
    /// Package platform-service declarations.
    PackagePlatformServices,
    /// Platform-service role.
    PlatformServiceRole,
    /// Platform-service declaration path.
    PlatformServiceDeclaration,
    /// Dependency package identity.
    DependencyPackage,
    /// Dependency product identity.
    DependencyProduct,
    /// Dependency target predicate.
    DependencyTargetPredicate,
    /// Source-root local name.
    SourceRootName,
    /// Source-root portable path or discovered filesystem path.
    SourceRootPath,
    /// Target local name.
    TargetName,
    /// Target identity.
    TargetIdentity,
    /// Target-predicate property and value.
    TargetPredicate,
}

impl DiagnosticProjectManifestField {
    /// Returns the stable machine key for this manifest field.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestPath => "manifest_path",
            Self::WorkspaceFormat => "workspace_format",
            Self::WorkspaceOutputRoot => "workspace_output_root",
            Self::WorkspaceTargets => "workspace_targets",
            Self::WorkspacePackages => "workspace_packages",
            Self::WorkspaceRootPackage => "workspace_root_package",
            Self::WorkspacePackageVersion => "workspace_package_version",
            Self::WorkspacePackagePath => "workspace_package_path",
            Self::PackageIdentity => "package_identity",
            Self::PackageVersion => "package_version",
            Self::PackageFeatures => "package_features",
            Self::PackageSourceRoots => "package_source_roots",
            Self::PackageProducts => "package_products",
            Self::ProductIdentity => "product_identity",
            Self::ProductSourceRoots => "product_source_roots",
            Self::ProductTargets => "product_targets",
            Self::ProductOutputs => "product_outputs",
            Self::ProductDependencies => "product_dependencies",
            Self::ProductTestedLibrary => "product_tested_library",
            Self::PackagePlatformServices => "package_platform_services",
            Self::PlatformServiceRole => "platform_service_role",
            Self::PlatformServiceDeclaration => "platform_service_declaration",
            Self::DependencyPackage => "dependency_package",
            Self::DependencyProduct => "dependency_product",
            Self::DependencyTargetPredicate => "dependency_target_predicate",
            Self::SourceRootName => "source_root_name",
            Self::SourceRootPath => "source_root_path",
            Self::TargetName => "target_name",
            Self::TargetIdentity => "target_identity",
            Self::TargetPredicate => "target_predicate",
        }
    }
}

/// Exact package or product identity participating in a project dependency cycle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectDependencyCycleMember {
    /// A package-level dependency cycle member.
    Package {
        /// Canonical package identity.
        identity: String,
    },
    /// A product-level dependency cycle member.
    Product {
        /// Canonical owning package identity.
        package: String,
        /// Canonical package-local product identity.
        product: String,
    },
}
