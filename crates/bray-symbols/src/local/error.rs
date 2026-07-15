/// A structural failure while constructing one local semantic-region snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSymbolBuildError {
    /// An ID belongs to another local semantic region.
    ForeignRegion,
    /// A referenced lexical scope does not exist.
    UnknownScope,
    /// A referenced anonymous callable does not exist.
    UnknownAnonymousCallable,
    /// A referenced local symbol does not exist in its category table.
    UnknownLocalSymbol,
    /// A non-root scope was created without a lexical parent.
    MissingParentScope,
    /// A root scope was created with a lexical parent.
    RootHasParentScope,
    /// More than one root scope was created for one semantic region.
    DuplicateRootScope,
    /// A snapshot cannot be created without a root scope.
    MissingRootScope,
    /// A local symbol was indexed outside its containing scope.
    SymbolOutsideScope,
    /// A contract scope was assigned more than one contextual result.
    DuplicatePostconditionResult,
    /// A contextual symbol was attached to an incompatible lexical boundary.
    InvalidScopeBoundary,
    /// An anonymous callable boundary is not a callable child of its introduction scope.
    InvalidAnonymousCallableScope,
    /// An anonymous callable boundary is already owned by another callable.
    AnonymousCallableScopeAlreadyAssigned,
    /// An anonymous parameter does not use its callable's exact boundary scope.
    AnonymousCallableParameterScopeMismatch,
    /// A symbol without an ordinary name was inserted into an ordinary-name index.
    SymbolHasNoOrdinaryName,
    /// A category-specific local table exceeded the compact ID representation.
    CapacityExceeded,
    /// A local symbol key could not be formed from the supplied syntax.
    MissingSyntaxAnchor,
}
