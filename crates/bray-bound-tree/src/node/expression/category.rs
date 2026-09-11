use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{BorrowKind, TypeId};

use crate::{BoundBlockId, BoundExpressionId, BoundNodeOrigin, BoundPatternId, BoundUnitKey};

/// A source operator classified independently from parser token representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundOperator {
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

/// A source assignment operator classified independently from parser token representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundAssignmentOperator {
    /// Replace the destination value.
    Assign,
    /// Add to the destination value.
    Add,
    /// Subtract from the destination value.
    Subtract,
    /// Multiply the destination value.
    Multiply,
    /// Divide the destination value.
    Divide,
    /// Replace the destination with the division remainder.
    Remainder,
    /// Matrix-multiply the destination value.
    MatrixMultiply,
    /// Exponentiate the destination value.
    Exponentiate,
    /// Apply bitwise conjunction to the destination value.
    BitwiseAnd,
    /// Apply bitwise disjunction to the destination value.
    BitwiseOr,
    /// Apply bitwise exclusive disjunction to the destination value.
    BitwiseXor,
    /// Shift the destination value left.
    ShiftLeft,
    /// Shift the destination value right.
    ShiftRight,
}

macro_rules! define_operator_expression {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
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

/// A source assignment operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundAssignmentExpression {
    origin: BoundNodeOrigin,
    operator: BoundAssignmentOperator,
    operands: Arc<[BoundExpressionId]>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundAssignmentExpression {
    /// Creates an assignment expression with source-ordered operands.
    pub fn new(
        origin: BoundNodeOrigin,
        operator: BoundAssignmentOperator,
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

    /// Returns the source assignment operator.
    pub const fn operator(&self) -> BoundAssignmentOperator {
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

/// An explicit conversion retaining its operand and resolved target type when available.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
    /// A bounded half-open integer range.
    Range,
    /// A fixed-size array produced by one generator iteration.
    ArrayGenerator,
    /// A lazy sequence produced by one generator iteration.
    GeneralGenerator,
    /// A conditional branch.
    Conditional,
    /// A non-binding structural boolean test that observes its subject.
    /// Its sole block owns temporaries until the test produces its Boolean result.
    PatternTest,
    /// A structural condition whose bindings enter scope after a successful match.
    PatternBinding,
    /// A condition with bindings. Its sole operand is a boolean expression, and its sole block
    /// owns bindings and initializer temporaries through the successful body or failed attempt.
    Condition,
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
    /// Boolean conjunction folding over one iterable operand.
    BooleanAllFold,
    /// Boolean disjunction folding over one iterable operand.
    BooleanAnyFold,
    /// An explicit panic.
    Panic,
}

impl BoundOperator {
    /// Returns this operator's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LogicalOr => "logical_or",
            Self::LogicalAnd => "logical_and",
            Self::Equal => "equal",
            Self::NotEqual => "not_equal",
            Self::Less => "less",
            Self::LessEqual => "less_equal",
            Self::Greater => "greater",
            Self::GreaterEqual => "greater_equal",
            Self::BitwiseOr => "bitwise_or",
            Self::BitwiseXor => "bitwise_xor",
            Self::BitwiseAnd => "bitwise_and",
            Self::ShiftLeft => "shift_left",
            Self::ShiftRight => "shift_right",
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Multiply => "multiply",
            Self::Divide => "divide",
            Self::Remainder => "remainder",
            Self::MatrixMultiply => "matrix_multiply",
            Self::Exponentiate => "exponentiate",
            Self::BitwiseNot => "bitwise_not",
            Self::LogicalNot => "logical_not",
        }
    }
}

impl BoundAssignmentOperator {
    /// Returns the binary operation applied by a compound assignment.
    pub const fn binary_operator(self) -> Option<BoundOperator> {
        match self {
            Self::Assign => None,
            Self::Add => Some(BoundOperator::Add),
            Self::Subtract => Some(BoundOperator::Subtract),
            Self::Multiply => Some(BoundOperator::Multiply),
            Self::Divide => Some(BoundOperator::Divide),
            Self::Remainder => Some(BoundOperator::Remainder),
            Self::MatrixMultiply => Some(BoundOperator::MatrixMultiply),
            Self::Exponentiate => Some(BoundOperator::Exponentiate),
            Self::BitwiseAnd => Some(BoundOperator::BitwiseAnd),
            Self::BitwiseOr => Some(BoundOperator::BitwiseOr),
            Self::BitwiseXor => Some(BoundOperator::BitwiseXor),
            Self::ShiftLeft => Some(BoundOperator::ShiftLeft),
            Self::ShiftRight => Some(BoundOperator::ShiftRight),
        }
    }

    /// Returns this assignment operator's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assign => "assign",
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Multiply => "multiply",
            Self::Divide => "divide",
            Self::Remainder => "remainder",
            Self::MatrixMultiply => "matrix_multiply",
            Self::Exponentiate => "exponentiate",
            Self::BitwiseAnd => "bitwise_and",
            Self::BitwiseOr => "bitwise_or",
            Self::BitwiseXor => "bitwise_xor",
            Self::ShiftLeft => "shift_left",
            Self::ShiftRight => "shift_right",
        }
    }
}

impl BoundStructuredExpressionKind {
    /// Returns this structured expression kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Absence => "absence",
            Self::ElementIndex => "element_index",
            Self::SliceIndex => "slice_index",
            Self::NullablePropagation => "nullable_propagation",
            Self::Tuple => "tuple",
            Self::Array => "array",
            Self::RepeatedArray => "repeated_array",
            Self::Range => "range",
            Self::ArrayGenerator => "array_generator",
            Self::GeneralGenerator => "general_generator",
            Self::Conditional => "conditional",
            Self::PatternTest => "pattern_test",
            Self::PatternBinding => "pattern_binding",
            Self::Condition => "condition",
            Self::While => "while",
            Self::Loop => "loop",
            Self::With => "with",
            Self::Borrow => "borrow",
            Self::TrustBoundary => "trust_boundary",
            Self::Assertion => "assertion",
            Self::ResultPropagation => "result_propagation",
            Self::Catch => "catch",
            Self::BooleanAllFold => "boolean_all_fold",
            Self::BooleanAnyFold => "boolean_any_fold",
            Self::Panic => "panic",
        }
    }
}
/// The optional lower and upper bounds of one slice operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundSliceBounds {
    lower: Option<BoundExpressionId>,
    upper: Option<BoundExpressionId>,
}

impl BoundSliceBounds {
    /// Creates slice bounds without losing which side of `..` was omitted.
    pub const fn new(lower: Option<BoundExpressionId>, upper: Option<BoundExpressionId>) -> Self {
        Self { lower, upper }
    }

    /// Returns the optional inclusive lower-bound expression.
    pub const fn lower(self) -> Option<BoundExpressionId> {
        self.lower
    }

    /// Returns the optional exclusive upper-bound expression.
    pub const fn upper(self) -> Option<BoundExpressionId> {
        self.upper
    }
}

/// A source-shaped aggregate, control-flow, or effect expression.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundStructuredExpression {
    origin: BoundNodeOrigin,
    kind: BoundStructuredExpressionKind,
    operands: Arc<[BoundExpressionId]>,
    blocks: Arc<[BoundBlockId]>,
    patterns: Arc<[BoundPatternId]>,
    slice_bounds: Option<BoundSliceBounds>,
    borrow_kind: Option<BorrowKind>,
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
            slice_bounds: None,
            borrow_kind: None,
            ty,
            is_recovered,
        }
    }

    /// Retains the exact optional bounds of a slice operation.
    pub fn with_slice_bounds(mut self, bounds: BoundSliceBounds) -> Self {
        self.slice_bounds = Some(bounds);

        self
    }

    /// Retains whether a borrow permits shared observation or exclusive mutation.
    pub const fn with_borrow_kind(mut self, kind: BorrowKind) -> Self {
        self.borrow_kind = Some(kind);

        self
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

    /// Returns the exact slice bounds when this is a slice operation.
    pub const fn slice_bounds(&self) -> Option<BoundSliceBounds> {
        self.slice_bounds
    }

    /// Returns the exact borrow kind for a borrow expression.
    pub const fn borrow_kind(&self) -> Option<BorrowKind> {
        self.borrow_kind
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
