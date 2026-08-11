use bray_source::SourceSpan;

use crate::DiagnosticArg;

/// Actionable correction associated with one diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticSuggestion {
    kind: DiagnosticSuggestionKind,
    applicability: DiagnosticSuggestionApplicability,
    edits: Vec<DiagnosticSourceEdit>,
    args: Vec<DiagnosticArg>,
}

impl DiagnosticSuggestion {
    /// Creates edit-free manual guidance.
    pub fn manual(kind: DiagnosticSuggestionKind) -> Self {
        Self {
            kind,
            applicability: DiagnosticSuggestionApplicability::Manual,
            edits: Vec::new(),
            args: Vec::new(),
        }
    }

    /// Creates a machine-applicable correction from one exact source edit.
    pub fn machine_edit(kind: DiagnosticSuggestionKind, edit: DiagnosticSourceEdit) -> Self {
        Self::one_edit(
            kind,
            DiagnosticSuggestionApplicability::MachineApplicable,
            edit,
        )
    }

    /// Creates a correction requiring user judgment from one plausible source edit.
    pub fn maybe_edit(kind: DiagnosticSuggestionKind, edit: DiagnosticSourceEdit) -> Self {
        Self::one_edit(
            kind,
            DiagnosticSuggestionApplicability::MaybeApplicable,
            edit,
        )
    }

    fn one_edit(
        kind: DiagnosticSuggestionKind,
        applicability: DiagnosticSuggestionApplicability,
        edit: DiagnosticSourceEdit,
    ) -> Self {
        Self {
            kind,
            applicability,
            edits: vec![edit],
            args: Vec::new(),
        }
    }

    /// Creates an applicable suggestion from a nonempty, non-overlapping edit set.
    pub fn try_edits(
        kind: DiagnosticSuggestionKind,
        applicability: DiagnosticSuggestionApplicability,
        edits: impl IntoIterator<Item = DiagnosticSourceEdit>,
    ) -> Result<Self, DiagnosticSuggestionBuildError> {
        if applicability == DiagnosticSuggestionApplicability::Manual {
            return Err(DiagnosticSuggestionBuildError::ManualWithEdits);
        }

        let mut edits = edits.into_iter().collect::<Vec<_>>();

        if edits.is_empty() {
            return Err(DiagnosticSuggestionBuildError::MissingEdit);
        }

        edits.sort_unstable_by_key(|edit| edit.span());

        if edits
            .windows(2)
            .any(|pair| edits_overlap(&pair[0], &pair[1]))
        {
            return Err(DiagnosticSuggestionBuildError::OverlappingEdits);
        }

        Ok(Self {
            kind,
            applicability,
            edits,
            args: Vec::new(),
        })
    }

    /// Adds one typed message argument.
    pub fn with_arg(mut self, arg: DiagnosticArg) -> Self {
        self.args.push(arg);

        self
    }

    /// Returns the stable suggestion category.
    pub const fn kind(&self) -> DiagnosticSuggestionKind {
        self.kind
    }

    /// Returns how safely tooling can apply this suggestion.
    pub const fn applicability(&self) -> DiagnosticSuggestionApplicability {
        self.applicability
    }

    /// Returns the ordered source edits.
    pub fn edits(&self) -> &[DiagnosticSourceEdit] {
        &self.edits
    }

    /// Returns the typed message arguments.
    pub fn args(&self) -> &[DiagnosticArg] {
        &self.args
    }
}

fn edits_overlap(left: &DiagnosticSourceEdit, right: &DiagnosticSourceEdit) -> bool {
    let left_span = left.span();
    let right_span = right.span();

    left_span.source_id() == right_span.source_id()
        && (left_span.start() == right_span.start() || left_span.end() > right_span.start())
}

/// Invalid construction of a diagnostic suggestion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSuggestionBuildError {
    /// An applicable suggestion did not contain a source edit.
    MissingEdit,
    /// Manual guidance was incorrectly given source edits.
    ManualWithEdits,
    /// Two edits target the same or overlapping source bytes.
    OverlappingEdits,
}

/// One exact source replacement within a suggestion.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticSourceEdit {
    span: SourceSpan,
    replacement: String,
}

impl DiagnosticSourceEdit {
    /// Creates an edit replacing `span` with exact Bray source text.
    pub fn new(span: SourceSpan, replacement: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
        }
    }

    /// Returns the source range replaced by this edit.
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Returns the exact replacement source text.
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// Confidence with which tooling can apply a diagnostic suggestion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSuggestionApplicability {
    /// The edits are valid for the exact source and preserve intent beyond the correction.
    MachineApplicable,
    /// The edits are syntactically plausible but require user judgment.
    MaybeApplicable,
    /// The correction is advisory and cannot be represented as a complete safe edit.
    Manual,
}

impl DiagnosticSuggestionApplicability {
    /// Returns the stable machine key for this applicability.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MachineApplicable => "machine_applicable",
            Self::MaybeApplicable => "maybe_applicable",
            Self::Manual => "manual",
        }
    }
}

/// Stable category for an actionable correction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSuggestionKind {
    /// Insert syntax the parser can identify exactly.
    InsertExpectedSyntax,
    /// Replace a lone carriage return with a line feed.
    ReplaceWithLineFeed,
    /// Add the missing terminator for a paired lexical construct.
    AddTerminator,
    /// Run the formatter for the selected source.
    FormatSource,
}

impl DiagnosticSuggestionKind {
    /// Returns the stable machine key for this suggestion category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InsertExpectedSyntax => "insert_expected_syntax",
            Self::ReplaceWithLineFeed => "replace_with_line_feed",
            Self::AddTerminator => "add_terminator",
            Self::FormatSource => "format_source",
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use super::{
        DiagnosticSourceEdit, DiagnosticSuggestion, DiagnosticSuggestionApplicability,
        DiagnosticSuggestionBuildError, DiagnosticSuggestionKind,
    };

    #[test]
    fn applicable_suggestions_require_canonical_non_overlapping_edits() {
        let source = SourceId::new(2);
        let later = edit(source, 8, 9, ")");
        let earlier = edit(source, 2, 3, "(");

        let suggestion = DiagnosticSuggestion::try_edits(
            DiagnosticSuggestionKind::InsertExpectedSyntax,
            DiagnosticSuggestionApplicability::MachineApplicable,
            [later, earlier.clone()],
        )
        .unwrap_or_else(|error| panic!("disjoint edits should be valid: {error:?}"));

        assert_eq!(suggestion.edits()[0], earlier);

        assert_eq!(
            DiagnosticSuggestion::try_edits(
                DiagnosticSuggestionKind::InsertExpectedSyntax,
                DiagnosticSuggestionApplicability::MachineApplicable,
                [],
            ),
            Err(DiagnosticSuggestionBuildError::MissingEdit),
        );

        assert_eq!(
            DiagnosticSuggestion::try_edits(
                DiagnosticSuggestionKind::InsertExpectedSyntax,
                DiagnosticSuggestionApplicability::MaybeApplicable,
                [edit(source, 4, 7, "a"), edit(source, 6, 8, "b")],
            ),
            Err(DiagnosticSuggestionBuildError::OverlappingEdits),
        );

        assert_eq!(
            DiagnosticSuggestion::try_edits(
                DiagnosticSuggestionKind::InsertExpectedSyntax,
                DiagnosticSuggestionApplicability::MachineApplicable,
                [edit(source, 4, 4, "insert"), edit(source, 4, 7, "replace")],
            ),
            Err(DiagnosticSuggestionBuildError::OverlappingEdits),
        );

        let adjacent = DiagnosticSuggestion::try_edits(
            DiagnosticSuggestionKind::InsertExpectedSyntax,
            DiagnosticSuggestionApplicability::MachineApplicable,
            [edit(source, 4, 7, "replace"), edit(source, 7, 7, "insert")],
        );

        assert!(adjacent.is_ok());
    }

    #[test]
    fn manual_guidance_is_the_only_edit_free_suggestion() {
        let guidance = DiagnosticSuggestion::manual(DiagnosticSuggestionKind::FormatSource);

        assert_eq!(
            guidance.applicability(),
            DiagnosticSuggestionApplicability::Manual
        );

        assert!(guidance.edits().is_empty());

        assert_eq!(
            DiagnosticSuggestion::try_edits(
                DiagnosticSuggestionKind::FormatSource,
                DiagnosticSuggestionApplicability::Manual,
                [edit(SourceId::new(0), 0, 0, "")],
            ),
            Err(DiagnosticSuggestionBuildError::ManualWithEdits),
        );
    }

    fn edit(source: SourceId, start: u32, end: u32, replacement: &str) -> DiagnosticSourceEdit {
        DiagnosticSourceEdit::new(
            SourceSpan::new(
                source,
                TextRange::new(TextSize::new(start), TextSize::new(end)),
            ),
            replacement,
        )
    }
}
