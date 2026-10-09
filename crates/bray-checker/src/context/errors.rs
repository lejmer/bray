/// A failure while requesting a checker dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerQueryError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed while obtaining the dependency.
    Cancelled,
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
}

impl CheckerQueryError {
    /// Widens a checker-local failure to a boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> CheckerQueryError<Upstream> {
        match self {
            Self::Cancelled => CheckerQueryError::Cancelled,
            Self::Upstream(error) => match error {},
        }
    }
}

/// The result of requesting one checker dependency.
pub type CheckerQueryResult<T, Upstream = std::convert::Infallible> =
    Result<T, CheckerQueryError<Upstream>>;
