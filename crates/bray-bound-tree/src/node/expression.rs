use bray_symbols::TypeId;

use crate::{BoundBlockId, BoundNodeOrigin};

/// A checked expression retaining its exact semantic category.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundExpression {
    /// A source-shaped block used as an expression.
    Block(BoundBlockExpression),
    /// An expression that could not be checked successfully.
    Error(BoundErrorExpression),
}

impl BoundExpression {
    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(&self) -> BoundNodeOrigin {
        match self {
            Self::Block(expression) => expression.origin(),
            Self::Error(expression) => expression.origin(),
        }
    }

    /// Returns the checked or recovery type of this expression.
    pub const fn ty(&self) -> TypeId {
        match self {
            Self::Block(expression) => expression.ty(),
            Self::Error(expression) => expression.ty(),
        }
    }

    pub(crate) const fn block(&self) -> Option<BoundBlockId> {
        match self {
            Self::Block(expression) => Some(expression.block()),
            Self::Error(_) => None,
        }
    }
}

/// A source-shaped block expression and its checked result type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundBlockExpression {
    origin: BoundNodeOrigin,
    block: BoundBlockId,
    ty: TypeId,
}

impl BoundBlockExpression {
    /// Creates a checked block expression.
    pub const fn new(origin: BoundNodeOrigin, block: BoundBlockId, ty: TypeId) -> Self {
        Self { origin, block, ty }
    }

    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the block evaluated by this expression.
    pub const fn block(self) -> BoundBlockId {
        self.block
    }

    /// Returns the checked result type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// An expression preserved after semantic recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundErrorExpression {
    origin: BoundNodeOrigin,
    ty: TypeId,
}

impl BoundErrorExpression {
    /// Creates an error expression with the most useful type known after recovery.
    pub const fn new(origin: BoundNodeOrigin, ty: TypeId) -> Self {
        Self { origin, ty }
    }

    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the useful recovery type or canonical error type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_type, source_anchor};
    use crate::{BoundErrorExpression, BoundExpression, BoundNodeOrigin};

    #[test]
    fn error_expressions_retain_source_and_recovery_type() {
        let source = source_anchor();
        let ty = error_type();

        let expression = BoundExpression::Error(BoundErrorExpression::new(
            BoundNodeOrigin::source(source),
            ty,
        ));

        assert_eq!(expression.origin().source_anchor(), source);
        assert_eq!(expression.ty(), ty);
    }
}
