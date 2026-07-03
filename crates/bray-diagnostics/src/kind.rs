/// Stable category for a compiler diagnostic.
///
/// The kind is locale-neutral and survives wording changes in rendered
/// diagnostics. Variants are grouped by owning phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticKind {
    /// Source input contains bytes that are not valid UTF-8.
    LexicalInvalidUtf8,
    /// Source input contains a character that the lexer cannot accept.
    LexicalInvalidCharacter,
    /// A block comment reaches the end of input before its terminator.
    LexicalUnterminatedBlockComment,
}
