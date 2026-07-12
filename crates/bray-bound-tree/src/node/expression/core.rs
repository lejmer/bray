use bray_symbols::TypeId;

use crate::{BoundBlockId, BoundExpressionId, BoundNodeOrigin, BoundPatternId};

use super::{
    BoundAnonymousCallableExpression, BoundAssignmentExpression, BoundBinaryExpression,
    BoundCallExpression, BoundConversionExpression, BoundNameExpression, BoundStructuredExpression,
    BoundUnaryExpression,
};

/// A checked expression retaining its exact semantic category.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundExpression {
    /// A source-shaped block used as an expression.
    Block(BoundBlockExpression),
    /// A resolved value name.
    Name(BoundNameExpression),
    /// A prefix operator expression.
    Unary(BoundUnaryExpression),
    /// A binary operator expression.
    Binary(BoundBinaryExpression),
    /// An assignment expression.
    Assignment(BoundAssignmentExpression),
    /// A call with source-ordered argument inputs.
    Call(BoundCallExpression),
    /// An explicit conversion expression.
    Conversion(BoundConversionExpression),
    /// A reference to a separately checked anonymous callable unit.
    AnonymousCallable(BoundAnonymousCallableExpression),
    /// A source-shaped aggregate, control-flow, or effect expression.
    Structured(BoundStructuredExpression),
    /// An expression that could not be checked successfully.
    Error(BoundErrorExpression),
}

impl BoundExpression {
    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(&self) -> BoundNodeOrigin {
        match self {
            Self::Block(expression) => expression.origin(),
            Self::Name(expression) => expression.origin(),
            Self::Unary(expression) => expression.origin(),
            Self::Binary(expression) => expression.origin(),
            Self::Assignment(expression) => expression.origin(),
            Self::Call(expression) => expression.origin(),
            Self::Conversion(expression) => expression.origin(),
            Self::AnonymousCallable(expression) => expression.origin(),
            Self::Structured(expression) => expression.origin(),
            Self::Error(expression) => expression.origin(),
        }
    }

    /// Returns the checked or recovery type of this expression.
    pub const fn ty(&self) -> TypeId {
        match self {
            Self::Block(expression) => expression.ty(),
            Self::Name(expression) => expression.ty(),
            Self::Unary(expression) => expression.ty(),
            Self::Binary(expression) => expression.ty(),
            Self::Assignment(expression) => expression.ty(),
            Self::Call(expression) => expression.ty(),
            Self::Conversion(expression) => expression.ty(),
            Self::AnonymousCallable(expression) => expression.ty(),
            Self::Structured(expression) => expression.ty(),
            Self::Error(expression) => expression.ty(),
        }
    }

    /// Returns whether syntax or semantic recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        match self {
            Self::Block(expression) => expression.is_recovered(),
            Self::Name(expression) => expression.is_recovered(),
            Self::Unary(expression) => expression.is_recovered(),
            Self::Binary(expression) => expression.is_recovered(),
            Self::Assignment(expression) => expression.is_recovered(),
            Self::Call(expression) => expression.is_recovered(),
            Self::Conversion(expression) => expression.is_recovered(),
            Self::AnonymousCallable(expression) => expression.is_recovered(),
            Self::Structured(expression) => expression.is_recovered(),
            Self::Error(_) => true,
        }
    }

    pub(crate) fn child_expressions(&self) -> impl Iterator<Item = BoundExpressionId> + '_ {
        let children = match self {
            Self::Unary(expression) => expression.operands(),
            Self::Binary(expression) => expression.operands(),
            Self::Assignment(expression) => expression.operands(),
            Self::Call(expression) => expression.operands(),
            Self::Conversion(expression) => expression.operands(),
            Self::Structured(expression) => expression.operands(),
            Self::Block(_) | Self::Name(_) | Self::AnonymousCallable(_) | Self::Error(_) => &[],
        };

        children.iter().copied()
    }

    pub(crate) fn child_blocks(&self) -> impl Iterator<Item = BoundBlockId> + '_ {
        let block = match self {
            Self::Block(expression) => Some(expression.block()),
            Self::Structured(expression) => expression.blocks().first().copied(),
            _ => None,
        };

        let remaining = match self {
            Self::Structured(expression) => expression.blocks().get(1..).unwrap_or_default(),
            _ => &[],
        };

        block.into_iter().chain(remaining.iter().copied())
    }

    pub(crate) fn child_patterns(&self) -> impl Iterator<Item = BoundPatternId> + '_ {
        match self {
            Self::Structured(expression) => expression.patterns(),
            _ => &[],
        }
        .iter()
        .copied()
    }
}

/// A source-shaped block expression and its checked result type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundBlockExpression {
    origin: BoundNodeOrigin,
    block: BoundBlockId,
    ty: TypeId,
    is_recovered: bool,
}

impl BoundBlockExpression {
    /// Creates a checked block expression.
    pub const fn new(
        origin: BoundNodeOrigin,
        block: BoundBlockId,
        ty: TypeId,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            block,
            ty,
            is_recovered,
        }
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

    /// Returns whether recovery contributed to this expression or its block.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
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
        assert!(expression.is_recovered());
    }
}
