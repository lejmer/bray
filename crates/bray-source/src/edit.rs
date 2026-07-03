use crate::id::SourceId;
use crate::snapshot::SourceSnapshot;
use crate::text::{TextRange, TextSize, TextSizeOverflow};
use crate::version::SourceVersion;

/// Text edit expressed in UTF-8 byte offsets.
///
/// LSP UTF-16 ranges should be converted through [`LineIndex`](crate::LineIndex)
/// before constructing a source edit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceEdit {
    range: TextRange,
    replacement: String,
}

impl SourceEdit {
    /// Creates a source edit.
    pub fn new(range: TextRange, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }

    /// Returns the source range replaced by this edit.
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns the replacement text.
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// Error returned when a source edit cannot be applied to a snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceEditError {
    /// The edit range is outside the snapshot text.
    RangeOutOfBounds {
        range: TextRange,
        text_len: TextSize,
    },
    /// The edit range does not align with UTF-8 scalar boundaries.
    InvalidUtf8Boundary { range: TextRange },
    /// The edited source text is too large for compact byte offsets.
    TextTooLarge(TextSizeOverflow),
}

impl From<TextSizeOverflow> for SourceEditError {
    fn from(error: TextSizeOverflow) -> Self {
        Self::TextTooLarge(error)
    }
}

impl SourceSnapshot {
    /// Applies a source edit and returns a new immutable snapshot.
    ///
    /// The original snapshot is unchanged. The new snapshot keeps the same
    /// logical source identity and origin, but receives the supplied snapshot ID
    /// and document version.
    pub fn apply_edit(
        &self,
        source_id: SourceId,
        version: impl Into<SourceVersion>,
        edit: &SourceEdit,
    ) -> Result<Self, SourceEditError> {
        let range = edit.range();
        if range.end() > self.text_len() {
            return Err(SourceEditError::RangeOutOfBounds {
                range,
                text_len: self.text_len(),
            });
        }

        let byte_range = match range.to_usize_range() {
            Some(range) => range,
            None => {
                return Err(SourceEditError::RangeOutOfBounds {
                    range,
                    text_len: self.text_len(),
                });
            }
        };

        let prefix = match self.text().get(..byte_range.start) {
            Some(prefix) => prefix,
            None => return Err(SourceEditError::InvalidUtf8Boundary { range }),
        };

        let suffix = match self.text().get(byte_range.end..) {
            Some(suffix) => suffix,
            None => return Err(SourceEditError::InvalidUtf8Boundary { range }),
        };

        let edited_len = self
            .text()
            .len()
            .checked_sub(byte_range.len())
            .and_then(|len| len.checked_add(edit.replacement().len()))
            .ok_or(SourceEditError::TextTooLarge(TextSizeOverflow::new(
                usize::MAX,
            )))?;

        let mut text = String::with_capacity(edited_len);

        text.push_str(prefix);
        text.push_str(edit.replacement());
        text.push_str(suffix);

        // Cloning the origin preserves immutable snapshot provenance while the
        // new revision receives its own SourceId and SourceVersion.
        SourceSnapshot::new(
            source_id,
            self.identity(),
            self.origin().clone(),
            version,
            text,
        )
        .map_err(SourceEditError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceEdit, SourceEditError};
    use crate::{
        SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
    };

    #[test]
    fn source_edits_create_new_immutable_snapshot_revisions() {
        let snapshot = snapshot(SourceId::new(0), SourceVersion::new(4), "let x = 1\n");
        let edit = SourceEdit::new(TextRange::new(TextSize::new(8), TextSize::new(9)), "2");

        let edited = match snapshot.apply_edit(SourceId::new(1), SourceVersion::new(5), &edit) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test edit should apply: {error:?}"),
        };

        assert_eq!(snapshot.source_id(), SourceId::new(0));
        assert_eq!(snapshot.version(), SourceVersion::new(4));
        assert_eq!(snapshot.text(), "let x = 1\n");

        assert_eq!(edited.source_id(), SourceId::new(1));
        assert_eq!(edited.identity(), snapshot.identity());
        assert_eq!(edited.origin(), snapshot.origin());
        assert_eq!(edited.version(), SourceVersion::new(5));
        assert_eq!(edited.text(), "let x = 2\n");
    }

    #[test]
    fn source_edits_reject_out_of_bounds_ranges() {
        let snapshot = snapshot(SourceId::new(0), SourceVersion::new(0), "abc");
        let edit = SourceEdit::new(TextRange::new(TextSize::new(2), TextSize::new(4)), "");

        assert_eq!(
            snapshot.apply_edit(SourceId::new(1), SourceVersion::new(1), &edit),
            Err(SourceEditError::RangeOutOfBounds {
                range: edit.range(),
                text_len: TextSize::new(3)
            })
        );
    }

    #[test]
    fn source_edits_reject_ranges_inside_utf8_scalars() {
        let snapshot = snapshot(SourceId::new(0), SourceVersion::new(0), "aé");
        let edit = SourceEdit::new(TextRange::new(TextSize::new(2), TextSize::new(3)), "");

        assert_eq!(
            snapshot.apply_edit(SourceId::new(1), SourceVersion::new(1), &edit),
            Err(SourceEditError::InvalidUtf8Boundary {
                range: edit.range()
            })
        );
    }

    #[test]
    fn source_edits_are_send_and_sync() {
        assert_send_sync::<SourceEdit>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn snapshot(source_id: SourceId, version: SourceVersion, text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            source_id,
            SourceIdentity::new(7),
            SourceOrigin::lsp_document("untitled:main.bray"),
            version,
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
