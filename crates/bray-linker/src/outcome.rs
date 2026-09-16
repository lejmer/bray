mod artifact;
mod diagnostics;
mod model;

pub use artifact::LinkedArtifactSet;
pub(crate) use model::failed_outcome;
pub use model::{
    LinkFailure, LinkOutcome, LinkStatus, link_failure_diagnostics,
};
