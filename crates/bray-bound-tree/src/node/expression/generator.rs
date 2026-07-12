use bray_declarations::SyntaxAnchor;
use bray_symbols::TypeId;

use crate::{BoundBlockId, BoundExpressionId, BoundNodeOrigin, BoundPatternId};

/// One generator iteration with its exact source, pattern, body, and region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundGeneratorExpression {
    origin: BoundNodeOrigin,
    source: BoundExpressionId,
    pattern: BoundPatternId,
    body: BoundBlockId,
    region: SyntaxAnchor,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundGeneratorExpression {
    /// Creates one bound generator iteration.
    pub const fn new(
        origin: BoundNodeOrigin,
        source: BoundExpressionId,
        pattern: BoundPatternId,
        body: BoundBlockId,
        region: SyntaxAnchor,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            source,
            pattern,
            body,
            region,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the iteration source.
    pub const fn source(self) -> BoundExpressionId {
        self.source
    }

    /// Returns the iteration pattern.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the iteration body.
    pub const fn body(self) -> BoundBlockId {
        self.body
    }

    /// Returns the generator region target.
    pub const fn region(self) -> SyntaxAnchor {
        self.region
    }

    /// Returns the resolved type when available.
    pub const fn ty(self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this generator.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.source)
    }

    pub(crate) fn blocks(&self) -> &[BoundBlockId] {
        std::slice::from_ref(&self.body)
    }

    pub(crate) fn patterns(&self) -> &[BoundPatternId] {
        std::slice::from_ref(&self.pattern)
    }
}
