use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;
use bray_symbols::TypeId;

use crate::{BoundBlockId, BoundExpressionId, BoundNodeOrigin, BoundPatternId, BoundUnitKey};

/// A source operator classified independently from parser token representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundOperator {
    /// Simple assignment.
    Assign,
    /// Short-circuit logical disjunction.
    LogicalOr,
    /// Short-circuit logical conjunction.
    LogicalAnd,
    /// Equality comparison.
    Equal,
    /// Inequality comparison.
    NotEqual,
    /// Strict less-than comparison.
    Less,
    /// Less-than-or-equal comparison.
    LessEqual,
    /// Strict greater-than comparison.
    Greater,
    /// Greater-than-or-equal comparison.
    GreaterEqual,
    /// Bitwise disjunction.
    BitwiseOr,
    /// Bitwise exclusive disjunction.
    BitwiseXor,
    /// Bitwise conjunction.
    BitwiseAnd,
    /// Left shift.
    ShiftLeft,
    /// Right shift.
    ShiftRight,
    /// Addition or unary positive.
    Add,
    /// Subtraction or unary negation.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Division.
    Divide,
    /// Remainder.
    Remainder,
    /// Matrix multiplication.
    MatrixMultiply,
    /// Exponentiation.
    Exponentiate,
    /// Bitwise complement.
    BitwiseNot,
    /// Logical negation.
    LogicalNot,
}

macro_rules! define_operator_expression {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            origin: BoundNodeOrigin,
            operator: BoundOperator,
            operands: Arc<[BoundExpressionId]>,
            ty: Option<TypeId>,
            is_recovered: bool,
        }

        impl $name {
            /// Creates an operator expression with source-ordered operands.
            pub fn new(
                origin: BoundNodeOrigin,
                operator: BoundOperator,
                operands: impl IntoIterator<Item = BoundExpressionId>,
                ty: Option<TypeId>,
                is_recovered: bool,
            ) -> Self {
                Self {
                    origin,
                    operator,
                    operands: shared_slice(operands),
                    ty,
                    is_recovered,
                }
            }

            /// Returns the source or synthesized origin.
            pub const fn origin(&self) -> BoundNodeOrigin {
                self.origin
            }

            /// Returns the source operator kind.
            pub const fn operator(&self) -> BoundOperator {
                self.operator
            }

            /// Returns operands in evaluation order.
            pub fn operands(&self) -> &[BoundExpressionId] {
                &self.operands
            }

            /// Returns the checked or recovery type.
            pub const fn ty(&self) -> Option<TypeId> {
                self.ty
            }

            /// Returns whether recovery contributed to this expression.
            pub const fn is_recovered(&self) -> bool {
                self.is_recovered
            }
        }
    };
}

define_operator_expression!(BoundUnaryExpression, "A source unary operation.");
define_operator_expression!(BoundBinaryExpression, "A source binary operation.");
define_operator_expression!(BoundAssignmentExpression, "A source assignment operation.");

/// An explicit conversion retaining its operand and resolved target type when available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundConversionExpression {
    origin: BoundNodeOrigin,
    operand: BoundExpressionId,
    target_syntax: SyntaxAnchor,
    target_type: Option<TypeId>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundConversionExpression {
    /// Creates an explicit conversion expression.
    pub const fn new(
        origin: BoundNodeOrigin,
        operand: BoundExpressionId,
        target_syntax: SyntaxAnchor,
        target_type: Option<TypeId>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            operand,
            target_syntax,
            target_type,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the converted operand.
    pub const fn operand(self) -> BoundExpressionId {
        self.operand
    }

    /// Returns the exact target type-expression syntax anchor.
    pub const fn target_syntax(self) -> SyntaxAnchor {
        self.target_syntax
    }

    /// Returns the resolved target type when available.
    pub const fn target_type(self) -> Option<TypeId> {
        self.target_type
    }

    /// Returns the checked or recovery type.
    pub const fn ty(self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.operand)
    }
}

/// A reference to a separately bound anonymous callable unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundAnonymousCallableExpression {
    origin: BoundNodeOrigin,
    unit: BoundUnitKey,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundAnonymousCallableExpression {
    /// Creates an anonymous-callable reference.
    pub const fn new(
        origin: BoundNodeOrigin,
        unit: BoundUnitKey,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            unit,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the nested bound-unit key.
    pub const fn unit(&self) -> &BoundUnitKey {
        &self.unit
    }

    /// Returns the checked or recovery type.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// Closed source-level categories whose detailed checker decisions are added by focused services.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundStructuredExpressionKind {
    /// The unit value.
    Unit,
    /// The absence value.
    Absence,
    /// Element selection from a receiver.
    ElementIndex,
    /// Slice selection from a receiver.
    SliceIndex,
    /// Nullable propagation from a receiver.
    NullablePropagation,
    /// A tuple aggregate.
    Tuple,
    /// An array aggregate.
    Array,
    /// An array that repeats one value by a constant count.
    RepeatedArray,
    /// A conditional branch.
    Conditional,
    /// A conditional loop.
    While,
    /// An unconditional loop.
    Loop,
    /// A scoped lifecycle expression.
    With,
    /// A borrow operation.
    Borrow,
    /// A trusted boundary.
    TrustBoundary,
    /// A runtime assertion.
    Assertion,
    /// Result propagation.
    ResultPropagation,
    /// Panic catching.
    Catch,
    /// Construction through a type form.
    TypeFormConstruction,
    /// Boolean folding over operands.
    BooleanFold,
    /// An explicit panic.
    Panic,
}

/// A source-shaped aggregate, control-flow, or effect expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundStructuredExpression {
    origin: BoundNodeOrigin,
    kind: BoundStructuredExpressionKind,
    operands: Arc<[BoundExpressionId]>,
    blocks: Arc<[BoundBlockId]>,
    patterns: Arc<[BoundPatternId]>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundStructuredExpression {
    /// Creates a source-shaped expression with evaluation-ordered children.
    pub fn new(
        origin: BoundNodeOrigin,
        kind: BoundStructuredExpressionKind,
        operands: impl IntoIterator<Item = BoundExpressionId>,
        blocks: impl IntoIterator<Item = BoundBlockId>,
        patterns: impl IntoIterator<Item = BoundPatternId>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            operands: shared_slice(operands),
            blocks: shared_slice(blocks),
            patterns: shared_slice(patterns),
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact source-level semantic category.
    pub const fn kind(&self) -> BoundStructuredExpressionKind {
        self.kind
    }

    /// Returns nested expressions in evaluation order.
    pub fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }

    /// Returns nested blocks in source order.
    pub fn blocks(&self) -> &[BoundBlockId] {
        &self.blocks
    }

    /// Returns bound patterns in source order.
    pub fn patterns(&self) -> &[BoundPatternId] {
        &self.patterns
    }

    /// Returns the checked or recovery type.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}
