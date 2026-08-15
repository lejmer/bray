/// An outer binding-query failure that is not a source diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingQueryError {
    /// Cancellation was observed before the requested query could complete.
    Cancelled,
    /// A required dependency could not be supplied by the coordinating query layer.
    ///
    /// Ordinary malformed source must instead produce an error-aware semantic value with structured
    /// diagnostics.
    DependencyUnavailable,
}

/// The outer result of binding-query access or evaluation.
pub type BindingQueryResult<T> = Result<T, BindingQueryError>;
