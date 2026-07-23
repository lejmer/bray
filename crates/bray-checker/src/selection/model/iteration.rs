use bray_bound_tree::{BoundExpressionId, IterationSourceMode, SelectedIterationSource};
use bray_symbols::SymbolKey;

use super::SelectionCandidateKey;

/// One complete applicable iteration protocol pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IterationSourceCandidate {
    key: SelectionCandidateKey,
    selection: SelectedIterationSource,
}

impl IterationSourceCandidate {
    /// Creates one candidate from its exact iterable and iterator implementation keys.
    pub fn new(
        iterable: SymbolKey,
        iterator: SymbolKey,
        selection: SelectedIterationSource,
    ) -> Self {
        Self {
            key: SelectionCandidateKey::Iteration { iterable, iterator },
            selection,
        }
    }

    /// Returns the stable candidate identity.
    pub const fn key(&self) -> &SelectionCandidateKey {
        &self.key
    }

    /// Returns the exact selection committed by this candidate.
    pub const fn selection(&self) -> &SelectedIterationSource {
        &self.selection
    }
}

/// Inputs for selecting one iteration source protocol pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IterationSourceSelectionRequest {
    expression: BoundExpressionId,
    source: BoundExpressionId,
    mode: IterationSourceMode,
    candidates: Vec<IterationSourceCandidate>,
}

impl IterationSourceSelectionRequest {
    /// Creates one request with candidates normalized by stable semantic identity.
    pub fn new(
        expression: BoundExpressionId,
        source: BoundExpressionId,
        mode: IterationSourceMode,
        candidates: impl IntoIterator<Item = IterationSourceCandidate>,
    ) -> Self {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();

        candidates.sort_unstable_by(|left, right| left.key.cmp(&right.key));
        candidates.dedup_by(|left, right| left.key == right.key);

        Self {
            expression,
            source,
            mode,
            candidates,
        }
    }

    /// Returns the iteration expression owning this request.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the source expression being iterated.
    pub const fn source(&self) -> BoundExpressionId {
        self.source
    }

    /// Returns how the iteration source is accessed.
    pub const fn mode(&self) -> IterationSourceMode {
        self.mode
    }

    pub(in crate::selection) fn candidates(&self) -> &[IterationSourceCandidate] {
        &self.candidates
    }
}
