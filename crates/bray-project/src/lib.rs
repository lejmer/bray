//! Bray-owned project manifests and immutable deterministic package build graphs.

#![forbid(unsafe_code)]

mod contract;
mod error;
mod loader;
mod manifest;
mod model;
mod path;
mod predicate;

pub use contract::{PACKAGE_MANIFEST_FILE_NAME, WORKSPACE_MANIFEST_FILE_NAME};
pub use error::{ProjectLoadError, ProjectManifestProblem};
pub use loader::{
    is_valid_ordinary_package_identity, load_project_graph, load_standard_library_project_graph,
};
pub use manifest::{canonicalize_package_manifest, canonicalize_workspace_manifest};
pub use model::{
    FeatureName, PackageRole, ProjectDependency, ProjectGraph, ProjectPackage, ProjectProduct,
    ProjectSourceRoot, ProjectTarget, ProjectTargetBuildPlan,
};
pub use path::ProjectPath;
pub use predicate::{TargetPredicate, TargetPredicateValue};
