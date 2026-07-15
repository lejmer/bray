use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

/// Native product category produced by one link plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkedProductKind {
    /// Native executable image.
    Executable,
    /// Native shared library.
    SharedLibrary,
    /// Native static library archive.
    StaticLibrary,
}

impl LinkedProductKind {
    /// Returns the required primary artifact category for this product.
    pub const fn primary_artifact_kind(self) -> LinkedArtifactKind {
        match self {
            Self::Executable => LinkedArtifactKind::Executable,
            Self::SharedLibrary => LinkedArtifactKind::SharedLibrary,
            Self::StaticLibrary => LinkedArtifactKind::StaticLibrary,
        }
    }
}

/// Staged linked output category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkedArtifactKind {
    /// Primary native executable.
    Executable,
    /// Primary native shared library.
    SharedLibrary,
    /// Primary native static library archive.
    StaticLibrary,
    /// Platform import library accompanying a shared library.
    ImportLibrary,
    /// Separately staged debug information.
    DebugCompanion,
    /// Other target-required companion output.
    PlatformCompanion,
}

/// Whether a planned linked artifact is required for successful completion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkedArtifactRequirement {
    /// Linking cannot complete without this output.
    Required,
    /// Linking may complete when this output is unavailable.
    Optional,
}

/// Stable emitter-assigned identity of one staging destination.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StagingDestinationId(u32);

impl StagingDestinationId {
    /// Creates an identity from its deterministic order in the emission plan.
    pub const fn new(ordinal: u32) -> Self {
        Self(ordinal)
    }

    /// Returns the deterministic staging-destination ordinal.
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

/// Exact emitter-owned filesystem destination available to the linker.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StagingDestination {
    id: StagingDestinationId,
    path: PathBuf,
}

impl StagingDestination {
    /// Creates a staging destination when its path is non-empty.
    pub fn try_new(
        id: StagingDestinationId,
        path: impl Into<PathBuf>,
    ) -> Result<Self, StagingDestinationBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(StagingDestinationBuildError::EmptyPath);
        }

        Ok(Self { id, path })
    }

    /// Returns the emitter-assigned staging identity.
    pub const fn id(&self) -> StagingDestinationId {
        self.id
    }

    /// Returns the exact filesystem path writable by the selected driver.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A contract violation that prevents staging-destination construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagingDestinationBuildError {
    /// The staging filesystem path is empty.
    EmptyPath,
}

/// One output expected from a validated native link operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlannedLinkedArtifact {
    kind: LinkedArtifactKind,
    requirement: LinkedArtifactRequirement,
    destination: StagingDestination,
}

impl PlannedLinkedArtifact {
    /// Creates one typed linked-output requirement.
    pub const fn new(
        kind: LinkedArtifactKind,
        requirement: LinkedArtifactRequirement,
        destination: StagingDestination,
    ) -> Self {
        Self {
            kind,
            requirement,
            destination,
        }
    }

    /// Returns the staged artifact category.
    pub const fn kind(&self) -> LinkedArtifactKind {
        self.kind
    }

    /// Returns whether successful linking requires this output.
    pub const fn requirement(&self) -> LinkedArtifactRequirement {
        self.requirement
    }

    /// Returns the emitter-owned staging destination.
    pub const fn destination(&self) -> &StagingDestination {
        &self.destination
    }
}

/// Immutable metadata for one completely written staging artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedArtifact {
    kind: LinkedArtifactKind,
    destination: StagingDestinationId,
    byte_len: NonZeroU64,
}

impl LinkedArtifact {
    /// Creates one completed staging record before complete-set validation.
    pub const fn new(
        kind: LinkedArtifactKind,
        destination: StagingDestinationId,
        byte_len: NonZeroU64,
    ) -> Self {
        Self {
            kind,
            destination,
            byte_len,
        }
    }

    /// Returns the staged artifact category.
    pub const fn kind(&self) -> LinkedArtifactKind {
        self.kind
    }

    /// Returns the emitter-owned staging identity that received the bytes.
    pub const fn destination(&self) -> StagingDestinationId {
        self.destination
    }

    /// Returns the observed non-zero output length.
    pub const fn byte_len(&self) -> NonZeroU64 {
        self.byte_len
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LinkedArtifactKind, LinkedProductKind, StagingDestination, StagingDestinationBuildError,
        StagingDestinationId,
    };

    #[test]
    fn product_kinds_select_exact_primary_artifacts() {
        assert_eq!(
            LinkedProductKind::Executable.primary_artifact_kind(),
            LinkedArtifactKind::Executable
        );

        assert_eq!(
            LinkedProductKind::SharedLibrary.primary_artifact_kind(),
            LinkedArtifactKind::SharedLibrary
        );

        assert_eq!(
            LinkedProductKind::StaticLibrary.primary_artifact_kind(),
            LinkedArtifactKind::StaticLibrary
        );
    }

    #[test]
    fn staging_destinations_require_paths() {
        assert_eq!(
            StagingDestination::try_new(StagingDestinationId::new(0), ""),
            Err(StagingDestinationBuildError::EmptyPath)
        );
    }
}
