//! Bray-owned project manifests and immutable deterministic package build graphs.

#![forbid(unsafe_code)]

mod contract;
mod error;
mod loader;
mod manifest;
mod model;
mod path;

pub use contract::{PACKAGE_MANIFEST_FILE_NAME, WORKSPACE_MANIFEST_FILE_NAME};
pub use error::{ProjectLoadError, ProjectManifestProblem};
pub use loader::{load_project_graph, load_standard_library_project_graph};
pub use model::{
    FeatureName, PackageRole, ProjectDependency, ProjectGraph, ProjectPackage, ProjectProduct,
    ProjectSourceRoot, ProjectTarget,
};
pub use path::ProjectPath;
