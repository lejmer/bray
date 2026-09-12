use bray_source::SourceSpan;
use bray_symbols::SymbolOrdinal;

/// A clause identity in source checking or a separately compiled callable contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionClauseId {
    /// Exact source clause identity.
    Source(SourceSpan),
    /// Stable clause or domain ordinal within an imported callable.
    Imported(SymbolOrdinal),
}

impl From<SourceSpan> for ExecutionClauseId {
    fn from(source: SourceSpan) -> Self {
        Self::Source(source)
    }
}
