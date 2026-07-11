/// An outer binder fact-query failure that is not a source diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BinderFactError {
    /// Cancellation was observed before the requested fact could complete.
    Cancelled,
    /// A required fact could not be supplied by the coordinating query layer.
    ///
    /// Compilation owns the exact cache, cycle, scheduling, and infrastructure policy behind
    /// this boundary. Ordinary malformed source must instead produce an error-aware fact value
    /// with structured diagnostics.
    DependencyUnavailable,
}

/// The outer result of binder fact access or binding-dependent fact computation.
pub type BinderFactResult<T> = Result<T, BinderFactError>;
