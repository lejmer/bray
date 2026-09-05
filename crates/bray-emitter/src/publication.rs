//! Artifact content validation and external publication.

mod diagnostic;
pub(crate) mod generation;
mod link;
mod operation;
mod publisher;
mod staging;

pub use generation::{
    PublishedArtifact, PublishedGenerationReadError, PublishedProductReadGuard,
    RetainedProductGeneration, lock_published_product, resolve_published_artifact,
    retain_published_generation,
};

pub use publisher::{ArtifactPublisher, PublicationValidator};
