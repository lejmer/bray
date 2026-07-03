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
        Self(raw)
    }

    /// Returns the source snapshot ID raw value.
    pub const fn raw(self) -> u32 {
        self.0
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
    }
}
