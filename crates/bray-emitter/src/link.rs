//! Pure construction of native link plans from emission-owned staging.

mod construction;
mod facts;
mod staging;

pub use construction::{LinkPlanConstructionError, construct_link_plan};
pub use facts::ProductLinkFacts;
pub use staging::{
    LinkOutputStaging, LinkOutputStagingBuildError, StagedArtifact,
    StagedArtifactBuildError,
};
