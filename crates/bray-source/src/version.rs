/// Monotonic version for one logical source.
///
/// Source versions are stable inputs for LSP synchronization and incremental
/// cache invalidation. A new version of the same
/// [`SourceIdentity`](crate::SourceIdentity) represents a new logical revision
/// of that source.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceVersion(u64);

impl SourceVersion {
    /// Creates a source version from a raw value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw source version value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Returns the next source version, or `None` on overflow.
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }
}

impl From<u64> for SourceVersion {
    fn from(raw: u64) -> Self {
        Self::new(raw)
    }
}

impl From<SourceVersion> for u64 {
    fn from(version: SourceVersion) -> Self {
        version.raw()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceVersion;

    #[test]
    fn source_versions_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<SourceVersion>(), size_of::<u64>());

        let version = SourceVersion::new(42);
        let copied = version;

        assert_eq!(copied.raw(), 42);
    }

    #[test]
    fn source_versions_track_next_revision() {
        assert_eq!(
            SourceVersion::new(42).checked_next(),
            Some(SourceVersion::new(43))
        );
        assert_eq!(SourceVersion::new(u64::MAX).checked_next(), None);
    }
}
