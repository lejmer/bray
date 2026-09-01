/// An outer binding-query failure that is not a source diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingQueryError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed before the requested query could complete.
    Cancelled,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    /// The semantic value store rejected a lookup or interning operation.
    SemanticValue(bray_symbols::SemanticValueStoreError),
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
            Self::SemanticValue(error) => BindingQueryError::SemanticValue(error),
            Self::DependencyUnavailable => BindingQueryError::DependencyUnavailable,
            Self::Upstream(error) => match error {},
        }
    }
}

/// The outer result of binding-query access or evaluation.
pub type BindingQueryResult<T, Upstream = std::convert::Infallible> =
    Result<T, BindingQueryError<Upstream>>;

#[cfg(test)]
mod tests {
    use bray_symbols::{SemanticValueKind, SemanticValueStoreError};

    use super::BindingQueryError;

    #[test]
    fn semantic_value_failures_survive_upstream_widening() {
        let error = SemanticValueStoreError::UnknownId {
            kind: SemanticValueKind::Type,
        };

        assert_eq!(
            BindingQueryError::SemanticValue(error).with_upstream::<u8>(),
            BindingQueryError::SemanticValue(error)
        );
    }
}
