/// Stable category for a compiler diagnostic.
///
/// The kind is locale-neutral and survives wording changes in rendered
/// diagnostics. Variants are grouped by owning phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticKind {
    /// A requested source file could not be read.
    SourceFileReadFailed,
    /// Source input contains bytes that are not valid UTF-8.
    SourceInvalidUtf8,
    /// Source input count cannot fit in compact source IDs.
    SourceTooManyInputs,
    /// Source text is too large for compact byte offsets.
    SourceTextTooLarge,
    /// The compilation request does not contain any source inputs.
    RequestMissingSourceInput,
    /// A compilation request source input is not valid.
    RequestInvalidSourceInput,
    /// The requested worker budget is not valid.
    RequestInvalidWorkerBudget,
    /// Source input contains a character that the lexer cannot accept.
    LexicalInvalidCharacter,
    /// A block comment reaches the end of input before its terminator.
    LexicalUnterminatedBlockComment,
}
