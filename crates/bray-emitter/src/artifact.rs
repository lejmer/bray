//! Immutable artifact identities, contributions, and publication records.

mod contribution;
pub(crate) mod content;
mod identity;
mod kind;
mod merge;
mod record;

pub use contribution::{ArtifactContribution, BackendContributionSet};
pub use identity::{ArtifactId, DependencyMetadataProducerId, LinkerProducerId};
pub use kind::{ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole};
pub use merge::{BackendContributionMergeError, BackendContributionMergeErrorKind};
pub use record::{EmittedArtifact, EmittedArtifactSet};
