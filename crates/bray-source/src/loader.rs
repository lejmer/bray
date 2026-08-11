use crate::encoding::{
    SourceUtf8Error, decode_source_bytes, leading_utf8_bom_len, normalize_source_text,
};
use crate::id::SourceId;
use crate::identity::SourceIdentity;
use crate::input::{SourceInput, SourceInputContent};
use crate::newline::SourceNewlinePolicy;
use crate::origin::SourceOrigin;
use crate::snapshot::SourceSnapshot;
use crate::text::{TextSize, TextSizeOverflow};
use crate::version::SourceVersion;

/// Error returned when source input cannot be loaded into a snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceLoadError {
    /// The loader cannot assign another compact source ID.
    TooManySources { count: u64 },
    /// Source bytes are not valid UTF-8.
    InvalidUtf8(SourceUtf8Error),
    /// The source text is too large for compact byte offsets.
    TextTooLarge(TextSizeOverflow),
}

impl From<TextSizeOverflow> for SourceLoadError {
    fn from(error: TextSizeOverflow) -> Self {
        Self::TextTooLarge(error)
    }
}

impl From<SourceUtf8Error> for SourceLoadError {
    fn from(error: SourceUtf8Error) -> Self {
        Self::InvalidUtf8(error)
    }
}

/// Turns requested source inputs into immutable source snapshots.
///
/// `SourceLoader` assigns [`SourceId`] values deterministically in load order.
/// It validates source bytes as UTF-8 and removes an initial UTF-8 byte order
/// mark from the resulting source text. It preserves source newline spellings
/// exactly. It does not perform file, LSP, or standard-input I/O. Callers
/// resolve external input into [`SourceInput`] before loading.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct SourceLoader {
    loaded_count: u64,
}

impl SourceLoader {
    /// Creates an empty source loader.
    pub const fn new() -> Self {
        Self { loaded_count: 0 }
    }

    pub(crate) const fn with_loaded_count(loaded_count: u64) -> Self {
        Self { loaded_count }
    }

    /// Returns the number of source snapshots loaded so far.
    pub const fn loaded_count(&self) -> u64 {
        self.loaded_count
    }

    /// Loads a source input into an immutable source snapshot.
    pub fn load_input(&mut self, input: SourceInput) -> Result<SourceSnapshot, SourceLoadError> {
        let (identity, origin, version, content) = input.into_snapshot_parts();

        self.load_content(identity, origin, version, content)
    }

    /// Loads source text and metadata into an immutable source snapshot.
    ///
    /// A leading byte order mark character is removed from the resulting text.
    pub fn load_snapshot(
        &mut self,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        text: impl Into<String>,
    ) -> Result<SourceSnapshot, SourceLoadError> {
        let text = normalize_source_text(text.into());

        self.load_decoded_text(identity, origin, version, text)
    }

    /// Loads source bytes and metadata into an immutable source snapshot.
    ///
    /// Bytes must be valid UTF-8. A leading UTF-8 byte order mark is removed
    /// from the resulting text.
    pub fn load_bytes(
        &mut self,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<SourceSnapshot, SourceLoadError> {
        let bytes = bytes.into();

        validate_source_byte_count(bytes.len(), leading_utf8_bom_len(&bytes))?;

        let text = match decode_source_bytes(bytes) {
            Ok(text) => text,
            Err(error) => {
                let valid_up_to = TextSize::try_from(error.valid_up_to())?;

                return Err(SourceUtf8Error::new(valid_up_to, error.error_len()).into());
            }
        };

        self.load_decoded_text(identity, origin, version, text)
    }

    fn load_content(
        &mut self,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: SourceVersion,
        content: SourceInputContent,
    ) -> Result<SourceSnapshot, SourceLoadError> {
        match content {
            SourceInputContent::Text(text) => self.load_snapshot(identity, origin, version, text),
            SourceInputContent::Bytes(bytes) => self.load_bytes(identity, origin, version, bytes),
        }
    }

    fn load_decoded_text(
        &mut self,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        text: String,
    ) -> Result<SourceSnapshot, SourceLoadError> {
        let text = SourceNewlinePolicy::DEFAULT.normalize_text(text);
        let source_id = self.next_source_id()?;
        let snapshot = SourceSnapshot::new(source_id, identity, origin, version, text)?;

        self.loaded_count += 1;

        Ok(snapshot)
    }

    fn next_source_id(&self) -> Result<SourceId, SourceLoadError> {
        let raw = match u32::try_from(self.loaded_count) {
            Ok(raw) => raw,
            Err(_) => {
                return Err(SourceLoadError::TooManySources {
                    count: self.loaded_count,
                });
            }
        };

        SourceId::stored(raw).ok_or(SourceLoadError::TooManySources {
            count: self.loaded_count,
        })
    }
}

fn validate_source_byte_count(
    byte_count: usize,
    leading_bom_len: usize,
) -> Result<(), TextSizeOverflow> {
    let content_byte_count = byte_count
        .checked_sub(leading_bom_len)
        .ok_or_else(|| TextSizeOverflow::new(byte_count))?;

    TextSize::try_from(content_byte_count).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{SourceLoadError, SourceLoader, validate_source_byte_count};
    use crate::{
        SourceId, SourceIdentity, SourceInput, SourceOrigin, SourceOriginKind, SourceUtf8Error,
        SourceVersion, TextSize,
    };

    #[test]
    fn source_loader_turns_inputs_into_snapshots() {
        let mut loader = SourceLoader::new();

        let input = SourceInput::lsp_open_document(
            SourceIdentity::new(4),
            "file:///main.bray",
            SourceVersion::new(8),
            "module main\n",
        );

        let snapshot = match loader.load_input(input) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source input should load successfully: {error:?}"),
        };

        assert_eq!(snapshot.source_id(), SourceId::new(0));
        assert_eq!(snapshot.identity(), SourceIdentity::new(4));
        assert_eq!(snapshot.origin().kind(), SourceOriginKind::LspDocument);
        assert_eq!(snapshot.version(), SourceVersion::new(8));
        assert_eq!(snapshot.text(), "module main\n");
        assert_eq!(loader.loaded_count(), 1);
    }

    #[test]
    fn source_loader_assigns_snapshot_ids_in_load_order() {
        let mut loader = SourceLoader::new();

        let first = load_virtual(
            &mut loader,
            SourceIdentity::new(10),
            SourceVersion::new(0),
            "a",
        );

        let second = load_virtual(
            &mut loader,
            SourceIdentity::new(10),
            SourceVersion::new(1),
            "b",
        );

        assert_eq!(first.source_id(), SourceId::new(0));
        assert_eq!(second.source_id(), SourceId::new(1));
        assert_eq!(loader.loaded_count(), 2);
    }

    #[test]
    fn source_loader_strips_leading_bom_from_source_bytes() {
        let mut loader = SourceLoader::new();

        let input = SourceInput::file_bytes(
            SourceIdentity::new(11),
            "main.bray",
            SourceVersion::new(0),
            Vec::from(b"\xEF\xBB\xBFmodule main\n"),
        );

        let snapshot = match loader.load_input(input) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source input should load successfully: {error:?}"),
        };

        assert_eq!(snapshot.text(), "module main\n");
        assert_eq!(snapshot.source_id(), SourceId::new(0));
        assert_eq!(loader.loaded_count(), 1);
    }

    #[test]
    fn source_loader_strips_leading_bom_from_source_text() {
        let mut loader = SourceLoader::new();

        let snapshot = match loader.load_snapshot(
            SourceIdentity::new(12),
            SourceOrigin::virtual_source("buffer"),
            SourceVersion::new(0),
            "\u{feff}module main\n",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source input should load successfully: {error:?}"),
        };

        assert_eq!(snapshot.text(), "module main\n");
    }

    #[test]
    fn source_loader_preserves_non_initial_bom_character() {
        let mut loader = SourceLoader::new();

        let snapshot = match loader.load_snapshot(
            SourceIdentity::new(13),
            SourceOrigin::virtual_source("buffer"),
            SourceVersion::new(0),
            "a\u{feff}b",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source input should load successfully: {error:?}"),
        };

        assert_eq!(snapshot.text(), "a\u{feff}b");
    }

    #[test]
    fn source_loader_preserves_newline_spelling() {
        let mut loader = SourceLoader::new();

        let snapshot = load_virtual(
            &mut loader,
            SourceIdentity::new(16),
            SourceVersion::new(0),
            "a\r\nb\rc\n",
        );

        assert_eq!(snapshot.text(), "a\r\nb\rc\n");
    }

    #[test]
    fn source_loader_rejects_invalid_utf8_without_consuming_source_id() {
        let mut loader = SourceLoader::new();

        let input = SourceInput::file_bytes(
            SourceIdentity::new(14),
            "main.bray",
            SourceVersion::new(0),
            vec![b'a', 0xff, b'b'],
        );

        let error = match loader.load_input(input) {
            Ok(snapshot) => panic!("test source input should fail: {snapshot:?}"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            SourceLoadError::InvalidUtf8(SourceUtf8Error::new(TextSize::new(1), Some(1)))
        );

        assert_eq!(loader.loaded_count(), 0);

        let snapshot = load_virtual(
            &mut loader,
            SourceIdentity::new(15),
            SourceVersion::new(0),
            "valid",
        );

        assert_eq!(snapshot.source_id(), SourceId::new(0));
        assert_eq!(loader.loaded_count(), 1);
    }

    #[test]
    fn oversized_source_bytes_are_rejected_before_utf8_offsets_are_reported() {
        let byte_count = usize::try_from(u64::from(u32::MAX) + 1)
            .unwrap_or_else(|_| panic!("test host must represent a byte count above TextSize"));

        assert_eq!(
            validate_source_byte_count(byte_count, 0),
            Err(crate::TextSizeOverflow::new(byte_count))
        );

        assert_eq!(validate_source_byte_count(byte_count, 1), Ok(()));
    }

    #[test]
    fn source_loader_is_send_and_sync() {
        assert_send_sync::<SourceLoader>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn load_virtual(
        loader: &mut SourceLoader,
        identity: SourceIdentity,
        version: SourceVersion,
        text: &str,
    ) -> crate::SourceSnapshot {
        let input = SourceInput::virtual_text(identity, "buffer", version, text);

        match loader.load_input(input) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source input should load successfully: {error:?}"),
        }
    }
}
