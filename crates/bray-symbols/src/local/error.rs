/// Failure to allocate a local semantic identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSymbolBuildError {
    /// A category-specific local table exceeded the compact ID representation.
    CapacityExceeded,
}
