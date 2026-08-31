/// An outer binding-query failure that is not a source diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingQueryError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed before the requested query could complete.
    Cancelled,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    /// A required dependency could not be supplied by the coordinating query layer.
    ///
    /// Ordinary malformed source must instead produce an error-aware semantic value with structured
    /// diagnostics.
    DependencyUnavailable,
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
}

impl BindingQueryError {
    /// Widens a locally produced failure to a query boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> BindingQueryError<Upstream> {
        match self {
            Self::Cancelled => BindingQueryError::Cancelled,
            Self::CheckerInfrastructure(error) => BindingQueryError::CheckerInfrastructure(error),
            Self::DependencyUnavailable => BindingQueryError::DependencyUnavailable,
            Self::Upstream(error) => match error {},
        }
    }
}

/// The outer result of binding-query access or evaluation.
pub type BindingQueryResult<T, Upstream = std::convert::Infallible> =
    Result<T, BindingQueryError<Upstream>>;
