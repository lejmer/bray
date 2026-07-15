use std::sync::Arc;

use bray_base::NonEmptySharedStr;

/// Supported category of one selected linker or archiver driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkerDriverKind {
    /// Linker embedded in the compiler process.
    EmbeddedLld,
    /// Explicit external LLD executable.
    ExternalLld,
    /// Configured platform system linker.
    System,
    /// Static-library archiver.
    Archiver,
    /// Explicit target-specific driver.
    TargetSpecific,
}

/// Stable identity of one selected linker driver and compatible toolchain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkerDriverIdentity {
    kind: LinkerDriverKind,
    name: NonEmptySharedStr,
    revision: NonEmptySharedStr,
    toolchain_revision: NonEmptySharedStr,
}

impl LinkerDriverIdentity {
    /// Creates a driver identity when every canonical revision component is non-empty.
    pub fn try_new(
        kind: LinkerDriverKind,
        name: impl Into<Arc<str>>,
        revision: impl Into<Arc<str>>,
        toolchain_revision: impl Into<Arc<str>>,
    ) -> Option<Self> {
        Some(Self {
            kind,
            name: NonEmptySharedStr::try_new(name)?,
            revision: NonEmptySharedStr::try_new(revision)?,
            toolchain_revision: NonEmptySharedStr::try_new(toolchain_revision)?,
        })
    }

    /// Returns the selected driver category.
    pub const fn kind(&self) -> LinkerDriverKind {
        self.kind
    }

    /// Returns the stable driver name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the Bray driver implementation revision.
    pub fn revision(&self) -> &str {
        self.revision.as_str()
    }

    /// Returns the compatible linker or archiver toolchain revision.
    pub fn toolchain_revision(&self) -> &str {
        self.toolchain_revision.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::{LinkerDriverIdentity, LinkerDriverKind};

    #[test]
    fn driver_identities_require_complete_revision_metadata() {
        assert_eq!(
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "", "1", "20"),
            None
        );

        assert_eq!(
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "", "20"),
            None
        );

        assert_eq!(
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", ""),
            None
        );

        let Some(identity) =
            LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
        else {
            panic!("complete test driver identity must be valid");
        };

        assert_eq!(identity.kind(), LinkerDriverKind::EmbeddedLld);
        assert_eq!(identity.name(), "lld");
        assert_eq!(identity.revision(), "1");
        assert_eq!(identity.toolchain_revision(), "20");
    }
}
