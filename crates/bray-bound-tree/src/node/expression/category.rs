use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, SymbolName, TypeId};

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

/// The exact value identity reached by a bound name expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundValueTarget {
    /// A body-local value identity.
    Local(AnyLocalSymbolId),
    /// A compilation-wide value identity.
    Surface(AnySymbolId),
}

/// A resolved value name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundNameExpression {
    origin: BoundNodeOrigin,
    target: BoundValueTarget,
    ty: TypeId,
    is_recovered: bool,
}

impl BoundNameExpression {
    /// Creates a resolved value-name expression.
    pub const fn new(
        origin: BoundNodeOrigin,
        target: BoundValueTarget,
        ty: TypeId,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            target,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact referenced value.
    pub const fn target(self) -> BoundValueTarget {
        self.target
    }

    /// Returns the checked or recovery type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

macro_rules! define_operator_expression {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            origin: BoundNodeOrigin,
            operator: BoundOperator,
            operands: Arc<[BoundExpressionId]>,
            ty: TypeId,
            is_recovered: bool,
        }

        impl $name {
            /// Creates an operator expression with source-ordered operands.
            pub fn new(
                origin: BoundNodeOrigin,
                operator: BoundOperator,
                operands: impl IntoIterator<Item = BoundExpressionId>,
                ty: TypeId,
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
            pub const fn ty(&self) -> TypeId {
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

/// One source-ordered input to overload selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundArgument {
    expression: BoundExpressionId,
    name: Option<SymbolName>,
    is_recovered: bool,
}

impl BoundArgument {
    /// Creates a positional or named call argument.
    pub const fn new(
        expression: BoundExpressionId,
        name: Option<SymbolName>,
        is_recovered: bool,
    ) -> Self {
        Self {
            expression,
            name,
            is_recovered,
        }
    }

    /// Returns the argument expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the canonical argument name when named.
    pub const fn name(&self) -> Option<&SymbolName> {
        self.name.as_ref()
    }

    /// Returns whether recovery contributed to this argument.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A call expression retaining inputs for later overload selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundCallExpression {
    origin: BoundNodeOrigin,
    callee: BoundExpressionId,
    arguments: Arc<[BoundArgument]>,
    operands: Arc<[BoundExpressionId]>,
    ty: TypeId,
    is_recovered: bool,
}

impl BoundCallExpression {
    /// Creates a call with arguments in source evaluation order.
    pub fn new(
        origin: BoundNodeOrigin,
        callee: BoundExpressionId,
        arguments: impl IntoIterator<Item = BoundArgument>,
        ty: TypeId,
        is_recovered: bool,
    ) -> Self {
        let arguments = shared_slice(arguments);
        let operands = shared_slice(
            std::iter::once(callee).chain(arguments.iter().map(|argument| argument.expression())),
        );

        Self {
            origin,
            callee,
            arguments,
            operands,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the callable expression supplied to overload selection.
    pub const fn callee(&self) -> BoundExpressionId {
        self.callee
    }

    /// Returns arguments in source evaluation order.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }

    /// Returns the checked or recovery type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}

/// An explicit conversion retaining its operand and resolved target type when available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundConversionExpression {
    origin: BoundNodeOrigin,
    operand: BoundExpressionId,
    target_syntax: SyntaxAnchor,
    target_type: Option<TypeId>,
    ty: TypeId,
    is_recovered: bool,
}

impl BoundConversionExpression {
    /// Creates an explicit conversion expression.
    pub const fn new(
        origin: BoundNodeOrigin,
        operand: BoundExpressionId,
        target_syntax: SyntaxAnchor,
        target_type: Option<TypeId>,
        ty: TypeId,
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
    pub const fn ty(self) -> TypeId {
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
    ty: TypeId,
    is_recovered: bool,
}

impl BoundAnonymousCallableExpression {
    /// Creates an anonymous-callable reference.
    pub const fn new(
        origin: BoundNodeOrigin,
        unit: BoundUnitKey,
        ty: TypeId,
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

    /// Returns the nested checked-unit key.
    pub const fn unit(&self) -> &BoundUnitKey {
        &self.unit
    }

    /// Returns the checked or recovery type.
    pub const fn ty(&self) -> TypeId {
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
    /// A scalar or string literal.
    Literal,
    /// The unit value.
    Unit,
    /// The absence value.
    Absence,
    /// Member selection from a receiver.
    MemberAccess,
    /// Element selection from a receiver.
    ElementIndex,
    /// Slice selection from a receiver.
    SliceIndex,
    /// Nullable propagation from a receiver.
    NullablePropagation,
    /// Trait-qualified member selection.
    TraitQualifiedMember,
    /// A tuple aggregate.
    Tuple,
    /// An array aggregate.
    Array,
    /// A struct construction.
    StructConstruction,
    /// A generator construction.
    Generator,
    /// A conditional branch.
    Conditional,
    /// A pattern match.
    Match,
    /// A conditional loop.
    While,
    /// An iteration loop.
    For,
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
    /// Asynchronous result waiting.
    Await,
    /// Construction through a type form.
    TypeFormConstruction,
    /// Boolean folding over operands.
    BooleanFold,
    /// An asynchronous block.
    AsyncBlock,
    /// A spawned computation.
    Spawn,
    /// A block result transfer.
    Yield,
    /// A callable result transfer.
    Return,
    /// A loop or block exit.
    Break,
    /// A loop continuation.
    Continue,
    /// An explicit panic.
    Panic,
}

/// A source member selector awaiting receiver-aware lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundMemberSelector {
    /// A named member selector.
    Name(SymbolName),
    /// A tuple element selector.
    TupleElement(u32),
}

/// A source-shaped aggregate, control-flow, or effect expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundStructuredExpression {
    origin: BoundNodeOrigin,
    kind: BoundStructuredExpressionKind,
    member_selector: Option<BoundMemberSelector>,
    control_target: Option<SyntaxAnchor>,
    operands: Arc<[BoundExpressionId]>,
    blocks: Arc<[BoundBlockId]>,
    patterns: Arc<[BoundPatternId]>,
    ty: TypeId,
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
        ty: TypeId,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            member_selector: None,
            control_target: None,
            operands: shared_slice(operands),
            blocks: shared_slice(blocks),
            patterns: shared_slice(patterns),
            ty,
            is_recovered,
        }
    }

    /// Attaches the source member selector for receiver-aware lookup.
    pub fn with_member_selector(mut self, selector: Option<BoundMemberSelector>) -> Self {
        self.member_selector = selector;

        self
    }

    /// Attaches the exact target of a source control transfer.
    pub const fn with_control_target(mut self, target: Option<SyntaxAnchor>) -> Self {
        self.control_target = target;

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

    /// Returns the member selector when receiver-aware lookup is required.
    pub const fn member_selector(&self) -> Option<&BoundMemberSelector> {
        self.member_selector.as_ref()
    }

    /// Returns the exact target of a source control transfer when applicable.
    pub const fn control_target(&self) -> Option<SyntaxAnchor> {
        self.control_target
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
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}
