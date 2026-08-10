use bray_source::TextRange;
use bray_symbols::TypeId;

use crate::BoundNodeOrigin;

/// The source-level category of a literal expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundLiteralKind {
    /// A binary, decimal, or hexadecimal integer literal.
    Integer,
    /// A real literal.
    Real,
    /// An imaginary literal.
    Imaginary,
    /// A Boolean literal.
    Boolean,
    /// A character literal.
    Character,
    /// A string literal.
    String,
}

/// A source literal awaiting or retaining its checked type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundLiteralExpression {
    origin: BoundNodeOrigin,
    spelling_range: TextRange,
    kind: BoundLiteralKind,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundLiteralExpression {
    /// Creates a literal with its source category and available checked type.
    pub const fn new(
        origin: BoundNodeOrigin,
        spelling_range: TextRange,
        kind: BoundLiteralKind,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            spelling_range,
            kind,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact source range of the literal token without trivia.
    pub const fn spelling_range(self) -> TextRange {
        self.spelling_range
    }

    /// Returns the exact source literal category.
    pub const fn kind(self) -> BoundLiteralKind {
        self.kind
    }

    /// Returns the checked or recovery type.
    pub const fn ty(self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this literal.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundLiteralExpression, BoundLiteralKind};
    use crate::BoundNodeOrigin;
    use crate::test_support::source_anchor;

    #[test]
    fn literals_retain_their_source_category_without_preselecting_a_type() {
        let origin = BoundNodeOrigin::source(source_anchor());

        let literal = BoundLiteralExpression::new(
            origin,
            source_anchor().syntax().full_range(),
            BoundLiteralKind::Integer,
            None,
            false,
        );

        assert_eq!(literal.origin(), origin);
        assert_eq!(literal.kind(), BoundLiteralKind::Integer);
        assert_eq!(literal.ty(), None);
        assert!(!literal.is_recovered());
    }
}
