//! Immutable artifact identities, contributions, and publication records.

mod contribution;
mod identity;
mod kind;
mod record;

pub use contribution::ArtifactContribution;
pub use identity::{ArtifactId, DependencyMetadataProducerId, LinkerProducerId};
pub use kind::{ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole};
pub use record::{EmittedArtifact, EmittedArtifactSet, EmittedArtifactSetBuildError};
