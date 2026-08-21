mod artifact;
mod diagnostics;
mod model;

pub(crate) use model::failed_outcome;
pub use artifact::{LinkedArtifactSet, LinkedArtifactSetBuildError};
pub use model::{LinkFailure, LinkOutcome, LinkOutcomeBuildError, LinkStatus, link_failure_diagnostics};
