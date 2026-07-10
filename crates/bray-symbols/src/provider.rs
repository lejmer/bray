use crate::ExactSymbolId;

/// Associates one exact symbol ID category with its canonical immutable record type.
///
/// The association is independent of symbol origin. Source, imported, compiler-known,
/// compiler-provided, and synthesized providers therefore expose the same record type for a
/// given exact ID category.
pub trait SymbolRecordId: ExactSymbolId {
    /// The canonical immutable record addressed by this exact ID type.
    type Record;
}

/// Provides checked read-only access to one exact category of compilation-wide symbols.
///
/// Providers may use different origin-specific storage internally, but callers observe only the
/// canonical kind-specific record selected by [`SymbolRecordId`]. An unknown ID returns `None`.
/// Provider implementations must support concurrent read-only requests.
pub trait SymbolProvider<I>: Send + Sync
where
    I: SymbolRecordId,
{
    /// Returns the symbol record addressed by `id` when this provider owns that identity.
    fn symbol(&self, id: I) -> Option<&I::Record>;
}
