use std::sync::Arc;

use bray_symbols::SymbolKey;

/// The semantic operation category used by one selection request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectionKind {
    /// An ordinary callable, method, or overload arm.
    Callable,
    /// A receiver-associated member.
    Member,
    /// A unary or binary operator implementation.
    Operator,
    /// An element or slice indexing contract.
    Index,
    /// A struct, variant, or type-form construction operation.
    Construction,
    /// An explicit conversion operation.
    Conversion,
    /// A trait implementation witness.
    Implementation,
}

/// Stable identity of one candidate participating in deterministic selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectionCandidateKey {
    /// The single compiler-defined rule for the request category and operand types.
    BuiltIn,
    /// A source, imported, or compiler-known declaration candidate.
    Symbol(SymbolKey),
}

impl From<SymbolKey> for SelectionCandidateKey {
    fn from(key: SymbolKey) -> Self {
        Self::Symbol(key)
    }
}

/// Why semantic selection did not produce one exact operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionFailure {
    /// No available candidate matched the request.
    Unavailable,
    /// Several applicable candidates remain after exact applicability checking.
    Ambiguous(Arc<[SelectionCandidateKey]>),
    /// Matching candidates exist but are inaccessible in the current context.
    Inaccessible,
    /// Candidate operand or parameter types reject the supplied expressions.
    Incompatible,
    /// Recovery in an input prevents a sound semantic choice.
    Recovered,
}

/// The deterministic result of selecting exactly one semantic operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateSelection<T> {
    /// One exact operation was selected.
    Selected(T),
    /// Selection completed with a source-facing failure.
    Failed(SelectionFailure),
}
