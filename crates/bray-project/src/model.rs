use std::sync::Arc;

use bray_runtime_interface::PlatformServiceBinding;
use bray_standard_library::PackageSourceAuthority;

use bray_symbols::{PackageIdentity, PackageVersion, ProductIdentity, ProductKind};
use bray_target::{TargetIdentity, TargetOutputKind, TargetProfile, TargetPropertyKind};

use crate::{ProjectPath, TargetPredicate};

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
    profile: TargetProfile,
}

impl ProjectTarget {
    pub(crate) fn new(name: Arc<str>, profile: TargetProfile) -> Self {
        Self { name, profile }
    }

    /// Returns the workspace-local target name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the compiler-facing target identity selected by this configuration.
    pub const fn identity(&self) -> &TargetIdentity {
        self.profile.identity()
    }

    /// Returns the complete language-defined profile selected by this target.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
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
    predicate: Option<TargetPredicate>,
    property_dependencies: Arc<[TargetPropertyKind]>,
    active_targets: Arc<[TargetIdentity]>,
}

impl ProjectDependency {
    pub(crate) const fn new(
        product: ProductIdentity,
        predicate: Option<TargetPredicate>,
        property_dependencies: Arc<[TargetPropertyKind]>,
        active_targets: Arc<[TargetIdentity]>,
    ) -> Self {
        Self {
            product,
            predicate,
            property_dependencies,
            active_targets,
        }
    }

    /// Returns the exact package and product selected by this edge.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the normalized target predicate when the edge is conditional.
    pub const fn predicate(&self) -> Option<&TargetPredicate> {
        self.predicate.as_ref()
    }

    /// Returns every language-defined target property read by the predicate.
    pub fn property_dependencies(&self) -> &[TargetPropertyKind] {
        &self.property_dependencies
    }

    /// Returns the product targets for which this dependency edge is active.
    pub fn active_targets(&self) -> &[TargetIdentity] {
        &self.active_targets
    }

    /// Returns whether this dependency edge is active for the supplied target.
    pub fn is_active_for(&self, target: &TargetIdentity) -> bool {
        self.active_targets.binary_search(target).is_ok()
    }
}

/// One package product with resolved source, target, and output selections.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectProduct {
    identity: ProductIdentity,
    kind: ProductKind,
    tested_library: Option<ProductIdentity>,
    dependencies: Arc<[ProjectDependency]>,
    sources: Arc<[ProjectPath]>,
    targets: Arc<[TargetIdentity]>,
    outputs: Arc<[TargetOutputKind]>,
    platform_services: Arc<[PlatformServiceBinding]>,
}

impl ProjectProduct {
    pub(crate) fn new(
        identity: ProductIdentity,
        kind: ProductKind,
        tested_library: Option<ProductIdentity>,
        dependencies: Arc<[ProjectDependency]>,
        sources: Arc<[ProjectPath]>,
        targets: Arc<[TargetIdentity]>,
        outputs: Arc<[TargetOutputKind]>,
        platform_services: Arc<[PlatformServiceBinding]>,
    ) -> Self {
        Self {
            identity,
            kind,
            tested_library,
            dependencies,
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

    /// Returns the sibling library whose public surface this test product consumes.
    pub const fn tested_library(&self) -> Option<&ProductIdentity> {
        self.tested_library.as_ref()
    }

    /// Returns this product's exact external dependency edges in canonical order.
    pub fn dependencies(&self) -> &[ProjectDependency] {
        &self.dependencies
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
    products: Arc<[ProjectProduct]>,
}

/// Canonical dependency-first package and product order for one workspace target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectTargetBuildPlan {
    target: TargetIdentity,
    packages: Arc<[PackageIdentity]>,
    products: Arc<[ProductIdentity]>,
}

impl ProjectTargetBuildPlan {
    pub(crate) const fn new(
        target: TargetIdentity,
        packages: Arc<[PackageIdentity]>,
        products: Arc<[ProductIdentity]>,
    ) -> Self {
        Self {
            target,
            packages,
            products,
        }
    }

    /// Returns the target whose active dependency graph this plan orders.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns active packages in deterministic dependency-first order.
    pub fn packages(&self) -> &[PackageIdentity] {
        &self.packages
    }

    /// Returns selected products in deterministic dependency-first order.
    pub fn products(&self) -> &[ProductIdentity] {
        &self.products
    }
}

impl ProjectPackage {
    #[expect(
        clippy::too_many_arguments,
        reason = "the package node keeps each independent immutable graph field explicit"
    )]
    pub(crate) fn new(
        identity: PackageIdentity,
        version: PackageVersion,
        role: PackageRole,
        path: ProjectPath,
        declared_features: Arc<[FeatureName]>,
        enabled_features: Arc<[FeatureName]>,
        source_roots: Arc<[ProjectSourceRoot]>,
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

    /// Returns products in canonical product-name order.
    pub fn products(&self) -> &[ProjectProduct] {
        &self.products
    }
}

/// Immutable project contract with deterministic per-target build plans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectGraph {
    source_authority: PackageSourceAuthority,
    formatter_configuration: Option<ProjectPath>,
    output_root: ProjectPath,
    targets: Arc<[ProjectTarget]>,
    packages: Arc<[ProjectPackage]>,
    build_plans: Arc<[ProjectTargetBuildPlan]>,
}

impl ProjectGraph {
    pub(crate) fn new(
        source_authority: PackageSourceAuthority,
        formatter_configuration: Option<ProjectPath>,
        output_root: ProjectPath,
        targets: Arc<[ProjectTarget]>,
        packages: Arc<[ProjectPackage]>,
        build_plans: Arc<[ProjectTargetBuildPlan]>,
    ) -> Self {
        Self {
            source_authority,
            formatter_configuration,
            output_root,
            targets,
            packages,
            build_plans,
        }
    }

    /// Returns the host authority under which package source was loaded.
    pub const fn source_authority(&self) -> PackageSourceAuthority {
        self.source_authority
    }

    /// Returns the workspace-selected formatter configuration path, when present.
    pub const fn formatter_configuration(&self) -> Option<&ProjectPath> {
        self.formatter_configuration.as_ref()
    }

    /// Returns the workspace-relative root for all build outputs.
    pub const fn output_root(&self) -> &ProjectPath {
        &self.output_root
    }

    /// Returns target configurations in canonical target-name order.
    pub fn targets(&self) -> &[ProjectTarget] {
        &self.targets
    }

    /// Returns package inventory nodes in canonical package-identity order.
    pub fn packages(&self) -> &[ProjectPackage] {
        &self.packages
    }

    /// Returns per-target dependency-first build plans in workspace target order.
    pub fn build_plans(&self) -> &[ProjectTargetBuildPlan] {
        &self.build_plans
    }

    /// Finds the dependency-first build plan for one target.
    pub fn build_plan(&self, target: &TargetIdentity) -> Option<&ProjectTargetBuildPlan> {
        self.build_plans.iter().find(|plan| plan.target() == target)
    }

    /// Finds a package by canonical identity.
    pub fn package(&self, identity: &PackageIdentity) -> Option<&ProjectPackage> {
        self.packages
            .iter()
            .find(|package| package.identity() == identity)
    }
}
