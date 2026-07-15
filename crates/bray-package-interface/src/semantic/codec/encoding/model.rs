use crate::InterfaceSectionTag;

/// One encoded semantic section ready for artifact assembly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedSemanticSection {
    tag: InterfaceSectionTag,
    record_count: u64,
    payload: Vec<u8>,
}

impl EncodedSemanticSection {
    pub(super) fn new(tag: InterfaceSectionTag, record_count: usize, payload: Vec<u8>) -> Self {
        Self {
            tag,
            record_count: u64::try_from(record_count).unwrap_or(u64::MAX),
            payload,
        }
    }

    /// Returns the stable section category.
    pub const fn tag(&self) -> InterfaceSectionTag {
        self.tag
    }

    /// Returns the number of logical records in this section.
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Returns the canonical wire payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub(crate) fn into_parts(self) -> (InterfaceSectionTag, u64, Vec<u8>) {
        (self.tag, self.record_count, self.payload)
    }
}
