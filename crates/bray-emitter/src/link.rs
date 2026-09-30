//! Pure construction of native link plans from emission-owned staging.

mod archive;
mod construction;
mod inputs;
mod staging;

pub use archive::construct_package_archive_plan;
pub use construction::{LinkPlanConstructionError, construct_link_plan};
pub use inputs::ProductLinkInputs;
pub use staging::{
    LinkOutputStaging, LinkOutputStagingBuildError, LinkStaging, LinkStagingError, StagedArtifact,
    StagedArtifactBuildError,
};
