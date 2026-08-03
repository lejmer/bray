use std::sync::Arc;

use bray_runtime_interface::PlatformServiceBinding;

use bray_symbols::{PackageIdentity, PackageVersion, ProductIdentity, ProductKind};
use bray_target::{TargetIdentity, TargetOutputKind};

use crate::ProjectPath;

/// A canonical package feature name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FeatureName(Arc<str>);

impl FeatureName {
    pub(crate) const fn new(name: Arc<str>) -> Self {
        Self(name)
    }

    /// Returns the canonical package-local feature name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for FeatureName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Workspace ownership role of one explicitly listed package.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PackageRole {
    /// A package selected as a workspace build root.
    Root,
    /// An exact project-owned dependency input.
    Vendored,
}

/// One named target selection in the workspace build contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectTarget {
    name: Arc<str>,
    identity: TargetIdentity,
}

impl ProjectTarget {
    pub(crate) fn new(name: Arc<str>, identity: TargetIdentity) -> Self {
        Self { name, identity }
    }

    /// Returns the workspace-local target name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the compiler-facing target identity selected by this configuration.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }
}

/// One named package-relative source root and its exact discovered source files.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectSourceRoot {
    name: Arc<str>,
    path: ProjectPath,
    sources: Arc<[ProjectPath]>,
}

impl ProjectSourceRoot {
    pub(crate) fn new(name: Arc<str>, path: ProjectPath, sources: Arc<[ProjectPath]>) -> Self {
        Self {
            name,
            path,
            sources,
        }
    }

    /// Returns the package-local source-root name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the package-relative source-root path.
    pub const fn path(&self) -> &ProjectPath {
        &self.path
    }

    /// Returns workspace-relative `.bray` source files in canonical path order.
    pub fn sources(&self) -> &[ProjectPath] {
        &self.sources
    }
}

/// One exact dependency on a declared package library product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectDependency {
    product: ProductIdentity,
}

impl ProjectDependency {
    pub(crate) const fn new(product: ProductIdentity) -> Self {
        Self { product }
    }

    /// Returns the exact package and product selected by this edge.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }
}

/// One package product with resolved source, target, and output selections.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectProduct {
    identity: ProductIdentity,
    kind: ProductKind,
    sources: Arc<[ProjectPath]>,
    targets: Arc<[TargetIdentity]>,
    outputs: Arc<[TargetOutputKind]>,
    platform_services: Arc<[PlatformServiceBinding]>,
}

impl ProjectProduct {
    pub(crate) fn new(
        identity: ProductIdentity,
        kind: ProductKind,
        sources: Arc<[ProjectPath]>,
        targets: Arc<[TargetIdentity]>,
        outputs: Arc<[TargetOutputKind]>,
        platform_services: Arc<[PlatformServiceBinding]>,
    ) -> Self {
        Self {
            identity,
            kind,
            sources,
            targets,
            outputs,
            platform_services,
        }
    }

    /// Returns the stable package-local product identity.
    pub const fn identity(&self) -> &ProductIdentity {
        &self.identity
    }

    /// Returns the language-level product category.
    pub const fn kind(&self) -> ProductKind {
        self.kind
    }

    /// Returns the product's workspace-relative sources in canonical path order.
    pub fn sources(&self) -> &[ProjectPath] {
        &self.sources
    }

    /// Returns selected compiler target identities in canonical order.
    pub fn targets(&self) -> &[TargetIdentity] {
        &self.targets
    }

    /// Returns requested output categories in canonical order.
    pub fn outputs(&self) -> &[TargetOutputKind] {
        &self.outputs
    }

    /// Returns explicit private platform-service declaration bindings in role order.
    pub fn platform_services(&self) -> &[PlatformServiceBinding] {
        &self.platform_services
    }
}

/// One validated package node in a project graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPackage {
    identity: PackageIdentity,
    version: PackageVersion,
    role: PackageRole,
    path: ProjectPath,
    declared_features: Arc<[FeatureName]>,
    enabled_features: Arc<[FeatureName]>,
    source_roots: Arc<[ProjectSourceRoot]>,
    dependencies: Arc<[ProjectDependency]>,
    products: Arc<[ProjectProduct]>,
}

impl ProjectPackage {
    #[expect(
        clippy::too_many_arguments,
        reason = "the package node keeps each independent immutable graph fact explicit"
    )]
    pub(crate) fn new(
        identity: PackageIdentity,
        version: PackageVersion,
        role: PackageRole,
        path: ProjectPath,
        declared_features: Arc<[FeatureName]>,
        enabled_features: Arc<[FeatureName]>,
        source_roots: Arc<[ProjectSourceRoot]>,
        dependencies: Arc<[ProjectDependency]>,
        products: Arc<[ProjectProduct]>,
    ) -> Self {
        Self {
            identity,
            version,
            role,
            path,
            declared_features,
            enabled_features,
            source_roots,
            dependencies,
            products,
        }
    }

    /// Returns the canonical package identity.
    pub const fn identity(&self) -> &PackageIdentity {
        &self.identity
    }

    /// Returns the package's resolved semantic version.
    pub const fn version(&self) -> &PackageVersion {
        &self.version
    }

    /// Returns whether this is a root or exact vendored package.
    pub const fn role(&self) -> PackageRole {
        self.role
    }

    /// Returns the package directory relative to the workspace.
    pub const fn path(&self) -> &ProjectPath {
        &self.path
    }

    /// Returns all feature names declared by the package.
    pub fn declared_features(&self) -> &[FeatureName] {
        &self.declared_features
    }

    /// Returns the exact feature selection recorded by the workspace.
    pub fn enabled_features(&self) -> &[FeatureName] {
        &self.enabled_features
    }

    /// Returns named source roots in canonical name order.
    pub fn source_roots(&self) -> &[ProjectSourceRoot] {
        &self.source_roots
    }

    /// Returns exact dependency products in canonical identity order.
    pub fn dependencies(&self) -> &[ProjectDependency] {
        &self.dependencies
    }

    /// Returns products in canonical product-name order.
    pub fn products(&self) -> &[ProjectProduct] {
        &self.products
    }
}

/// Immutable project contract ordered for deterministic dependency-first builds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectGraph {
    output_root: ProjectPath,
    targets: Arc<[ProjectTarget]>,
    packages: Arc<[ProjectPackage]>,
}

impl ProjectGraph {
    pub(crate) fn new(
        output_root: ProjectPath,
        targets: Arc<[ProjectTarget]>,
        packages: Arc<[ProjectPackage]>,
    ) -> Self {
        Self {
            output_root,
            targets,
            packages,
        }
    }

    /// Returns the workspace-relative root for all build outputs.
    pub const fn output_root(&self) -> &ProjectPath {
        &self.output_root
    }

    /// Returns target configurations in canonical target-name order.
    pub fn targets(&self) -> &[ProjectTarget] {
        &self.targets
    }

    /// Returns packages in stable dependency-first build order.
    pub fn packages(&self) -> &[ProjectPackage] {
        &self.packages
    }

    /// Finds a package by canonical identity.
    pub fn package(&self, identity: &PackageIdentity) -> Option<&ProjectPackage> {
        self.packages
            .iter()
            .find(|package| package.identity() == identity)
    }
}
