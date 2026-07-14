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
        assert!(crate::domain::is_stored(raw));

        Self(raw)
    }

    /// Creates a logical identity in the generated-source domain.
    pub const fn generated(ordinal: u32) -> Option<Self> {
        match crate::domain::generated(ordinal) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Returns whether this identity belongs to generated syntax.
    pub const fn is_generated(self) -> bool {
        crate::domain::is_generated(self.0)
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

        let Some(generated) = SourceIdentity::generated(7) else {
            panic!("small generated ordinals must be representable");
        };

        assert!(generated.is_generated());
    }

    #[test]
    #[should_panic]
    fn stored_source_identities_reject_the_generated_domain() {
        SourceIdentity::new(1 << 31);
    }
}
