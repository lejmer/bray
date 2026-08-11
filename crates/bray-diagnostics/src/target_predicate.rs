/// The literal category accepted or supplied by a target predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTargetPredicateValueKind {
    /// A target string literal.
    String,
    /// A target unsigned-integer literal.
    UnsignedInteger,
    /// A target Boolean literal.
    Boolean,
}

impl DiagnosticTargetPredicateValueKind {
    /// Returns the stable locale-neutral key for this literal category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::UnsignedInteger => "unsigned_integer",
            Self::Boolean => "boolean",
        }
    }
}
