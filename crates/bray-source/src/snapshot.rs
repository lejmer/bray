use crate::id::SourceId;
use crate::identity::SourceIdentity;
use crate::origin::SourceOrigin;
use crate::text::{TextRange, TextSize, TextSizeOverflow};
use crate::version::SourceVersion;

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

/// Deterministic non-cryptographic checksum of a source input snapshot's text.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceChecksum(u64);

impl SourceChecksum {
    /// Computes the checksum for source text.
    pub fn for_text(text: &str) -> Self {
        let mut checksum = FNV_OFFSET_BASIS;

        for byte in text.bytes() {
            checksum ^= u64::from(byte);
            checksum = checksum.wrapping_mul(FNV_PRIME);
        }

        Self(checksum)
    }

    /// Returns the raw checksum value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl From<SourceChecksum> for u64 {
    fn from(checksum: SourceChecksum) -> Self {
        checksum.raw()
    }
}

/// Immutable source text plus identity and provenance metadata.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct SourceSnapshot {
    source_id: SourceId,
    identity: SourceIdentity,
    origin: SourceOrigin,
    version: SourceVersion,
    checksum: SourceChecksum,
    text_len: TextSize,
    text: String,
}

impl SourceSnapshot {
    /// Creates an immutable source snapshot.
    pub fn new(
        source_id: SourceId,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        text: impl Into<String>,
    ) -> Result<Self, TextSizeOverflow> {
        let text = text.into();
        let text_len = TextSize::try_from(text.len())?;
        let checksum = SourceChecksum::for_text(&text);

        Ok(Self {
            source_id,
            identity,
            origin,
            version: version.into(),
            checksum,
            text_len,
            text,
        })
    }

    /// Returns the source snapshot identity.
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Returns the logical source identity.
    pub const fn identity(&self) -> SourceIdentity {
        self.identity
    }

    /// Returns the source origin.
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Returns the source version.
    pub const fn version(&self) -> SourceVersion {
        self.version
    }

    /// Returns the checksum of the source text.
    pub const fn checksum(&self) -> SourceChecksum {
        self.checksum
    }

    /// Returns the source text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the source text byte length.
    pub const fn text_len(&self) -> TextSize {
        self.text_len
    }

    /// Returns the full source text range.
    pub const fn full_range(&self) -> TextRange {
        TextRange::new(TextSize::ZERO, self.text_len)
    }

    /// Returns whether the source text is empty.
    pub const fn is_empty(&self) -> bool {
        self.text_len.bytes() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceChecksum, SourceSnapshot};
    use crate::{SourceId, SourceIdentity, SourceOrigin, SourceVersion, TextRange, TextSize};

    #[test]
    fn snapshots_store_immutable_text_metadata() {
        let snapshot = match SourceSnapshot::new(
            SourceId::new(2),
            SourceIdentity::new(9),
            SourceOrigin::virtual_source("buffer"),
            SourceVersion::new(4),
            "module main\n",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        };

        assert_eq!(snapshot.source_id(), SourceId::new(2));
        assert_eq!(snapshot.identity(), SourceIdentity::new(9));
        assert_eq!(snapshot.origin().kind(), crate::SourceOriginKind::Virtual);
        assert_eq!(snapshot.version(), SourceVersion::new(4));
        assert_eq!(snapshot.text(), "module main\n");
        assert_eq!(snapshot.text_len(), TextSize::new(12));

        assert_eq!(
            snapshot.full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(12))
        );

        assert!(!snapshot.is_empty());
    }

    #[test]
    fn checksums_are_deterministic_for_source_text() {
        let left = SourceChecksum::for_text("abc");
        let right = SourceChecksum::for_text("abc");
        let other = SourceChecksum::for_text("abcd");

        assert_eq!(left, right);
        assert_ne!(left, other);
    }
}
