/// Severity assigned to a diagnostic record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SeverityKind {
    /// Prevents successful compilation of the affected product.
    Error,
    /// Does not prevent compilation unless invocation policy promotes it.
    Warning,
    /// Supporting information attached to another diagnostic.
    Note,
    /// Advisory information attached to another diagnostic.
    Help,
}
