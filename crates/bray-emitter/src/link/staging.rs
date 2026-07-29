use std::path::{Path, PathBuf};

use bray_linker::{LinkedArtifactKind, StagingPathKey};

use crate::ArtifactId;

/// Exact filesystem artifact retained for one native link operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StagedArtifact {
    pub(super) artifact: ArtifactId,
    pub(super) path: PathBuf,
}

impl StagedArtifact {
    /// Creates a completed staged artifact when its filesystem path is nonempty.
    pub fn try_new(
        artifact: ArtifactId,
        path: impl Into<PathBuf>,
    ) -> Result<Self, StagedArtifactBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(StagedArtifactBuildError::EmptyPath);
        }

        Ok(Self { artifact, path })
    }

    /// Returns the artifact identity selected by the emission plan.
    pub const fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }

    /// Returns the exact staged filesystem path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A contract violation that prevents staged-artifact construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedArtifactBuildError {
    /// The staged filesystem path is empty.
    EmptyPath,
}

/// Emitter-owned staging location for one planned linked output.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LinkOutputStaging {
    pub(super) artifact: ArtifactId,
    pub(super) kind: LinkedArtifactKind,
    pub(super) path: PathBuf,
    pub(super) path_key: StagingPathKey,
}

impl LinkOutputStaging {
    /// Creates one linked-output staging location when its filesystem path is nonempty.
    pub fn try_new(
        artifact: ArtifactId,
        kind: LinkedArtifactKind,
        path: impl Into<PathBuf>,
        path_key: StagingPathKey,
    ) -> Result<Self, LinkOutputStagingBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(LinkOutputStagingBuildError::EmptyPath);
        }

        Ok(Self {
            artifact,
            kind,
            path,
            path_key,
        })
    }

    /// Returns the planned artifact that will receive the linked bytes.
    pub const fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }

    /// Returns the native linked artifact category.
    pub const fn kind(&self) -> LinkedArtifactKind {
        self.kind
    }

    /// Returns the exact writable staging path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the normalized staging-path identity.
    pub const fn path_key(&self) -> &StagingPathKey {
        &self.path_key
    }
}

/// A contract violation that prevents linked-output staging construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkOutputStagingBuildError {
    /// The linked-output staging path is empty.
    EmptyPath,
}
