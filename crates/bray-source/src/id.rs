/// Stable typed identity for one source input snapshot.
///
/// The source table assigns the raw value. Keeping it typed prevents source
/// inputs from being mixed with other compiler IDs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(u32);

impl SourceId {
    /// Creates a source identity from the source table's stable raw value.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the source table raw value.
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
