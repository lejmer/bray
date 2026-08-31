use bray_symbols::SemanticValueStoreError;

/// A failure outside ordinary source diagnostics while binding one semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundUnitBindingError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed before a complete unit could be returned.
    Cancelled,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
    /// The requested unit key does not identify supported source-backed syntax.
    InvalidUnitKey,
    /// The source syntax selected by the unit key is unavailable.
    MissingSyntax,
    /// The unit owner does not resolve to a source symbol in the supplied graph.
    MissingOwner,
    /// The unit owner is not contained by a logical module.
    MissingModule,
    /// A canonical semantic value could not be created.
    SemanticValue(SemanticValueStoreError),
    /// Bound-tree or local-symbol validation failed.
    Construction,
    /// Binding could not establish a complete recovery-aware root.
    Binding,
    /// Bound-unit validation failed.
    Assembly,
}
