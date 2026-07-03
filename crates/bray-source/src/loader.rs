use crate::id::SourceId;
use crate::identity::SourceIdentity;
use crate::input::SourceInput;
use crate::origin::SourceOrigin;
use crate::snapshot::SourceSnapshot;
use crate::text::TextSizeOverflow;
use crate::version::SourceVersion;

/// Error returned when source input cannot be loaded into a snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceLoadError {
    /// The loader cannot assign another compact source ID.
    TooManySources { count: u64 },
    /// The source text is too large for compact byte offsets.
    TextTooLarge(TextSizeOverflow),
}

impl From<TextSizeOverflow> for SourceLoadError {
    fn from(error: TextSizeOverflow) -> Self {
        Self::TextTooLarge(error)
    }
}

/// Turns requested source inputs into immutable source snapshots.
///
/// `SourceLoader` assigns [`SourceId`] values deterministically in load order.
/// It does not perform file, LSP, or standard-input I/O; callers resolve
/// external input into [`SourceInput`] before loading.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct SourceLoader {
    loaded_count: u64,
}

impl SourceLoader {
    /// Creates an empty source loader.
    pub const fn new() -> Self {
        Self { loaded_count: 0 }
    }

    /// Returns the number of source snapshots loaded so far.
    pub const fn loaded_count(&self) -> u64 {
        self.loaded_count
    }

    /// Loads a source input into an immutable source snapshot.
    pub fn load_input(&mut self, input: SourceInput) -> Result<SourceSnapshot, SourceLoadError> {
        let (identity, origin, version, text) = input.into_snapshot_parts();

        self.load_snapshot(identity, origin, version, text)
    }

    /// Loads source text and metadata into an immutable source snapshot.
    pub fn load_snapshot(
        &mut self,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        text: impl Into<String>,
    ) -> Result<SourceSnapshot, SourceLoadError> {
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

        Ok(SourceId::new(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::SourceLoader;
    use crate::{SourceId, SourceIdentity, SourceInput, SourceOriginKind, SourceVersion};

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
