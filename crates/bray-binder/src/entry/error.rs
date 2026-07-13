use bray_symbols::SemanticValueStoreError;

/// A failure outside ordinary source diagnostics while binding one semantic unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitBindingError {
    /// Cancellation was observed before a complete unit could be returned.
    Cancelled,
    /// The requested unit key does not identify supported source-backed syntax.
    InvalidUnitKey,
    /// The source syntax selected by the unit key is unavailable.
    MissingSyntax,
    /// The unit owner does not resolve to a source symbol in the supplied graph.
    MissingOwner,
    /// The unit owner is not contained by a logical module.
    MissingModule,
    /// Canonical semantic value construction failed.
    SemanticValue(SemanticValueStoreError),
    /// Bound-tree or local-symbol construction violated an internal contract.
    Construction,
    /// Binding could not establish a complete recovery-aware root.
    Binding,
    /// Bound-unit assembly violated an internal contract.
    Assembly,
}
