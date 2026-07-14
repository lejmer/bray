/// Stable typed identity for one source input snapshot.
///
/// The source loader assigns the raw value. Keeping it typed prevents source
/// snapshots from being mixed with other compiler IDs. Use
/// [`SourceIdentity`](crate::SourceIdentity) for the logical source across
/// multiple versions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(u32);

impl SourceId {
    /// Creates a source snapshot ID from the source loader's stable raw value.
    pub const fn new(raw: u32) -> Self {
        assert!(crate::domain::is_stored(raw));

        Self(raw)
    }

    /// Creates an indexable source-store ID when the raw value is not reserved.
    pub const fn stored(raw: u32) -> Option<Self> {
        match crate::domain::stored(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Creates a generated-source ID in the non-indexable source domain.
    pub const fn generated(ordinal: u32) -> Option<Self> {
        match crate::domain::generated(ordinal) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Returns whether this ID belongs to generated syntax outside the source store.
    pub const fn is_generated(self) -> bool {
        crate::domain::is_generated(self.0)
    }

    /// Returns the source snapshot ID raw value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns the source snapshot ID as a collection index.
    pub fn to_index(self) -> Option<usize> {
        if self.is_generated() {
            None
        } else {
            usize::try_from(self.0).ok()
        }
    }
}

impl From<SourceId> for u32 {
    fn from(source_id: SourceId) -> Self {
        source_id.raw()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceId;

    #[test]
    fn source_ids_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<SourceId>(), size_of::<u32>());

        let source_id = SourceId::new(7);
        let copied = source_id;

        assert_eq!(copied.raw(), 7);
        assert_eq!(copied.to_index(), Some(7));

        let Some(generated) = SourceId::generated(7) else {
            panic!("small generated ordinals must be representable");
        };

        assert!(generated.is_generated());
        assert_eq!(generated.to_index(), None);
    }

    #[test]
    #[should_panic]
    fn stored_source_ids_reject_the_generated_domain() {
        SourceId::new(1 << 31);
    }
}
