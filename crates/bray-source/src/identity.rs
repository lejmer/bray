/// Stable typed identity for one logical source across versions.
///
/// A source identity answers "which logical source is this?". A
/// [`SourceId`](crate::SourceId) answers "which immutable source snapshot is
/// this?".
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceIdentity(u32);

impl SourceIdentity {
    /// Creates a source identity from a stable raw value.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the source identity raw value.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl From<SourceIdentity> for u32 {
    fn from(identity: SourceIdentity) -> Self {
        identity.raw()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceIdentity;

    #[test]
    fn source_identities_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<SourceIdentity>(), size_of::<u32>());

        let identity = SourceIdentity::new(7);
        let copied = identity;

        assert_eq!(copied.raw(), 7);
    }
}
