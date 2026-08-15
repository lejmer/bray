//! Pure construction of native link plans from emission-owned staging.

mod construction;
mod inputs;
mod staging;

pub use construction::{LinkPlanConstructionError, construct_link_plan};
pub use inputs::ProductLinkInputs;
pub use staging::{
    LinkOutputStaging, LinkOutputStagingBuildError, LinkStaging, LinkStagingError, StagedArtifact,
    StagedArtifactBuildError,
};
