use std::sync::Arc;

use bray_base::shared_str;

use crate::id::SourceId;
use crate::identity::SourceIdentity;
use crate::origin::SourceOrigin;
use crate::text::{TextRange, TextSize, TextSizeOverflow};
use crate::version::SourceVersion;

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

/// Stable non-cryptographic hash of source input bytes.
///
/// The value is deterministic across processes and suitable for incremental
/// source cache keys. It is not a security boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceChecksum(u64);

impl SourceChecksum {
    /// Computes the checksum for source text.
    pub fn for_text(text: &str) -> Self {
        Self::for_bytes(text.as_bytes())
    }

    /// Computes the checksum for source bytes.
    pub fn for_bytes(bytes: &[u8]) -> Self {
        let mut checksum = FNV_OFFSET_BASIS;

        for byte in bytes {
            checksum ^= u64::from(*byte);
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
///
/// Cloning a snapshot shares the source text allocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceSnapshot {
    source_id: SourceId,
    identity: SourceIdentity,
    origin: SourceOrigin,
    version: SourceVersion,
    checksum: SourceChecksum,
    text_len: TextSize,
    text: Arc<str>,
}

impl SourceSnapshot {
    /// Creates an immutable source snapshot.
    pub fn new(
        source_id: SourceId,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        text: impl Into<Arc<str>>,
    ) -> Result<Self, TextSizeOverflow> {
        let text = shared_str(text);
        let text_len = TextSize::try_from(text.len())?;
        let checksum = SourceChecksum::for_text(text.as_ref());

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
        self.text.as_ref()
    }

    /// Returns a cheap shared handle to the immutable source text.
    pub fn shared_text(&self) -> Arc<str> {
        // Cloning the Arc shares immutable source text without copying bytes.
        Arc::clone(&self.text)
    }

    /// Returns the source text as UTF-8 bytes.
    pub fn bytes(&self) -> &[u8] {
        self.text().as_bytes()
    }

    /// Returns the source text slice covered by `range`.
    ///
    /// Returns `None` when the range is outside the text or does not align with
    /// UTF-8 scalar boundaries.
    pub fn text_slice(&self, range: TextRange) -> Option<&str> {
        range.slice_str(self.text())
    }

    /// Returns the source byte slice covered by `range`.
    pub fn byte_slice(&self, range: TextRange) -> Option<&[u8]> {
        range.slice_bytes(self.bytes())
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
    use std::sync::Arc;

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
        assert_eq!(snapshot.bytes(), b"module main\n");
        assert_eq!(snapshot.text_len(), TextSize::new(12));

        assert_eq!(
            snapshot.checksum(),
            SourceChecksum::for_text("module main\n")
        );

        assert_eq!(
            snapshot.full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(12))
        );

        assert!(!snapshot.is_empty());
    }

    #[test]
    fn snapshots_share_immutable_text_when_cloned() {
        let snapshot = snapshot("module main\n");
        let cloned = snapshot.clone();
        let original_text = snapshot.shared_text();
        let cloned_text = cloned.shared_text();

        assert_eq!(original_text.as_ref(), "module main\n");
        assert!(Arc::ptr_eq(&original_text, &cloned_text));
    }

    #[test]
    fn snapshots_slice_source_text_by_text_range() {
        let snapshot = snapshot("aébc");

        assert_eq!(
            snapshot.text_slice(TextRange::new(TextSize::new(1), TextSize::new(3))),
            Some("é")
        );

        assert_eq!(
            snapshot.text_slice(TextRange::new(TextSize::new(2), TextSize::new(3))),
            None
        );

        assert_eq!(
            snapshot.byte_slice(TextRange::new(TextSize::new(2), TextSize::new(4))),
            Some(&snapshot.bytes()[2..4])
        );
    }

    #[test]
    fn source_snapshots_are_send_and_sync() {
        assert_send_sync::<SourceSnapshot>();
    }

    #[test]
    fn checksums_are_deterministic_for_source_text() {
        let left = SourceChecksum::for_text("abc");
        let right = SourceChecksum::for_text("abc");
        let other = SourceChecksum::for_text("abcd");

        assert_eq!(left, right);
        assert_eq!(left, SourceChecksum::for_bytes(b"abc"));
        assert_eq!(left.raw(), 0xe71fa2190541574b);
        assert_ne!(left, other);
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(2),
            SourceIdentity::new(9),
            SourceOrigin::virtual_source("buffer"),
            SourceVersion::new(4),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
