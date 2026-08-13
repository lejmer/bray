//! Artifact content validation and external publication.

mod diagnostic;
mod generation;
mod link;
mod operation;
mod staging;

pub use generation::{
    PublishedGenerationReadError, PublishedProductReadGuard, lock_published_product,
    resolve_published_artifact,
};

pub use operation::ArtifactPublisher;
