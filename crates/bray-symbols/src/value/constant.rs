use std::sync::Arc;

use bray_base::{shared_slice, shared_str};

use crate::{
    AnySymbolId, ConstantSymbolId, GenericConstParameterSymbolId, StructFieldSymbolId,
    SymbolOrdinal, TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use super::{
    CallableInstanceId, ConstantTermId, ConstantValueId, GenericSubstitutionId,
    ImplementationInstanceId, TypeId,
};

/// A closed adapter over symbol categories that define compile-time constants.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AnyConstantDefinitionId {
    /// An ordinary constant declaration.
    Constant(ConstantSymbolId),
    /// A constant required or defaulted by a trait.
    TraitMember(TraitConstantMemberSymbolId),
    /// A constant fulfilling a trait requirement.
    TraitFulfillment(TraitConstantFulfillmentSymbolId),
}

impl AnyConstantDefinitionId {
    /// Erases this constant definition while retaining its exact symbol kind.
    pub fn into_any(self) -> AnySymbolId {
        match self {
            Self::Constant(id) => id.into(),
            Self::TraitMember(id) => id.into(),
            Self::TraitFulfillment(id) => id.into(),
        }
    }
}

impl From<ConstantSymbolId> for AnyConstantDefinitionId {
    fn from(id: ConstantSymbolId) -> Self {
        Self::Constant(id)
    }
}

impl From<TraitConstantMemberSymbolId> for AnyConstantDefinitionId {
    fn from(id: TraitConstantMemberSymbolId) -> Self {
        Self::TraitMember(id)
    }
}

impl From<TraitConstantFulfillmentSymbolId> for AnyConstantDefinitionId {
    fn from(id: TraitConstantFulfillmentSymbolId) -> Self {
        Self::TraitFulfillment(id)
    }
}

/// Sign retained by an arbitrary-width normalized integer constant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IntegerSign {
    /// Zero or a positive integer.
    NonNegative,
    /// A negative non-zero integer.
    Negative,
}

/// A host-independent arbitrary-width normalized integer constant.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IntegerConstant {
    sign: IntegerSign,
    magnitude: Arc<[u8]>,
}

impl IntegerConstant {
    /// Creates a canonical integer from a sign and big-endian unsigned magnitude.
    pub fn new(sign: IntegerSign, magnitude: impl IntoIterator<Item = u8>) -> Self {
        let magnitude: Vec<_> = magnitude
            .into_iter()
            .skip_while(|byte| *byte == 0)
            .collect();

        let sign = if magnitude.is_empty() {
            IntegerSign::NonNegative
        } else {
            sign
        };

        Self {
            sign,
            magnitude: shared_slice(magnitude),
        }
    }

    /// Returns this integer's canonical sign.
    pub const fn sign(&self) -> IntegerSign {
        self.sign
    }

    /// Returns the canonical big-endian unsigned magnitude without leading zero bytes.
    pub fn magnitude(&self) -> &[u8] {
        &self.magnitude
    }
}

/// Exact selected runtime-format bits for a real constant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RealConstantBits {
    /// IEEE binary16 bits.
    Binary16(u16),
    /// IEEE binary32 bits.
    Binary32(u32),
    /// IEEE binary64 bits.
    Binary64(u64),
    /// IEEE binary128 bits in canonical big-endian byte order.
    Binary128([u8; 16]),
}

/// The closed materializable payload of one typed constant value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantValueKind {
    /// The single canonical recovery value.
    Error,
    /// A Boolean value.
    Boolean(bool),
    /// A Unicode scalar value.
    Character(char),
    /// A normalized arbitrary-width integer value.
    Integer(IntegerConstant),
    /// A selected-format real value.
    Real(RealConstantBits),
    /// A selected-format complex value.
    Complex {
        /// Real component bits.
        real: RealConstantBits,
        /// Imaginary component bits.
        imaginary: RealConstantBits,
    },
    /// Canonical string content.
    String(Arc<str>),
    /// The unit value.
    Unit,
    /// Nullable absence.
    NullableAbsent,
    /// Nullable presence around a constant child value.
    NullablePresent(ConstantValueId),
    /// An ordered tuple value.
    Tuple(Arc<[ConstantValueId]>),
    /// An ordered array value.
    Array(Arc<[ConstantValueId]>),
    /// Ordered fields of a product value.
    Product(Arc<[ConstantValueId]>),
    /// An active union variant and its ordered payload values.
    Union {
        /// Exact active variant.
        variant: UnionVariantSymbolId,
        /// Ordered payload values.
        fields: Arc<[ConstantValueId]>,
    },
}

impl ConstantValueKind {
    /// Creates a canonical string constant payload.
    pub fn string(value: impl Into<Arc<str>>) -> Self {
        Self::String(shared_str(value))
    }

    /// Creates an ordered tuple constant payload.
    pub fn tuple(values: impl IntoIterator<Item = ConstantValueId>) -> Self {
        Self::Tuple(shared_slice(values))
    }

    /// Creates an ordered array constant payload.
    pub fn array(values: impl IntoIterator<Item = ConstantValueId>) -> Self {
        Self::Array(shared_slice(values))
    }

    /// Creates an ordered product constant payload.
    pub fn product(values: impl IntoIterator<Item = ConstantValueId>) -> Self {
        Self::Product(shared_slice(values))
    }

    /// Creates an active union variant constant payload.
    pub fn union(
        variant: UnionVariantSymbolId,
        fields: impl IntoIterator<Item = ConstantValueId>,
    ) -> Self {
        Self::Union {
            variant,
            fields: shared_slice(fields),
        }
    }
}

/// One fully evaluated typed materializable constant value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantValueData {
    ty: TypeId,
    kind: ConstantValueKind,
}

impl ConstantValueData {
    /// Creates a checked typed constant value.
    pub const fn new(ty: TypeId, kind: ConstantValueKind) -> Self {
        Self { ty, kind }
    }

    /// Returns the value's exact semantic type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the category-specific constant payload.
    pub const fn kind(&self) -> &ConstantValueKind {
        &self.kind
    }
}

/// A selected checked unary operation in an open constant term.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantUnaryOperation {
    /// Arithmetic identity.
    Identity,
    /// Arithmetic negation.
    Negate,
    /// Logical negation.
    LogicalNot,
    /// Bitwise complement.
    BitwiseNot,
}

/// A selected checked binary operation in an open constant term.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantBinaryOperation {
    /// Addition.
    Add,
    /// Subtraction.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Division.
    Divide,
    /// Remainder.
    Remainder,
    /// Logical conjunction.
    LogicalAnd,
    /// Logical disjunction.
    LogicalOr,
    /// Bitwise conjunction.
    BitwiseAnd,
    /// Bitwise disjunction.
    BitwiseOr,
    /// Bitwise exclusive disjunction.
    BitwiseXor,
    /// Left shift.
    ShiftLeft,
    /// Right shift.
    ShiftRight,
    /// Equality comparison.
    Equal,
    /// Inequality comparison.
    NotEqual,
    /// Less-than comparison.
    Less,
    /// Less-than-or-equal comparison.
    LessOrEqual,
    /// Greater-than comparison.
    Greater,
    /// Greater-than-or-equal comparison.
    GreaterOrEqual,
}

/// The exact projection applied to an open constant subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantProjectionKind {
    /// A tuple element by stable ordinal.
    TupleElement(SymbolOrdinal),
    /// An array element selected by a checked constant term.
    ArrayElement(ConstantTermId),
    /// A named product field.
    ProductField(StructFieldSymbolId),
    /// A named union payload field.
    UnionPayloadField(UnionPayloadFieldSymbolId),
    /// The present value of a nullable subject.
    NullableValue,
}

/// A checked projection from one open constant term.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantProjection {
    subject: ConstantTermId,
    kind: ConstantProjectionKind,
}

impl ConstantProjection {
    /// Creates a checked constant projection.
    pub const fn new(subject: ConstantTermId, kind: ConstantProjectionKind) -> Self {
        Self { subject, kind }
    }

    /// Returns the projected subject.
    pub const fn subject(self) -> ConstantTermId {
        self.subject
    }

    /// Returns the exact projection operation.
    pub const fn kind(self) -> ConstantProjectionKind {
        self.kind
    }
}

/// The restricted canonical representation of a checked open constant expression.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantTermData {
    /// A fully evaluated closed value.
    Value(ConstantValueId),
    /// A generic constant parameter.
    Parameter(GenericConstParameterSymbolId),
    /// A compiler-known target fact represented by its exact constant declaration.
    TargetFact(ConstantSymbolId),
    /// A selected unary operation.
    Unary {
        /// Exact checked operation.
        operation: ConstantUnaryOperation,
        /// Operand term.
        operand: ConstantTermId,
    },
    /// A selected ordered binary operation.
    Binary {
        /// Exact checked operation.
        operation: ConstantBinaryOperation,
        /// Left operand.
        left: ConstantTermId,
        /// Right operand.
        right: ConstantTermId,
    },
    /// An applied constant definition that can remain open.
    DefinitionApplication {
        /// Exact constant definition category.
        definition: AnyConstantDefinitionId,
        /// Ordered generic substitution.
        substitution: GenericSubstitutionId,
        /// Selected implementation witness when trait lookup participates.
        selected_implementation: Option<ImplementationInstanceId>,
    },
    /// A checked constant-call operation.
    Call {
        /// Exact substituted callable.
        callable: CallableInstanceId,
        /// Ordered argument terms.
        arguments: Arc<[ConstantTermId]>,
    },
    /// A checked projection from another term.
    Projection(ConstantProjection),
}

impl ConstantTermData {
    /// Creates a checked constant-call term.
    pub fn call(
        callable: CallableInstanceId,
        arguments: impl IntoIterator<Item = ConstantTermId>,
    ) -> Self {
        Self::Call {
            callable,
            arguments: shared_slice(arguments),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IntegerConstant, IntegerSign};

    #[test]
    fn integer_constants_normalize_leading_zeroes_and_negative_zero() {
        let zero = IntegerConstant::new(IntegerSign::Negative, [0, 0]);

        let positive = IntegerConstant::new(IntegerSign::NonNegative, [0, 0, 5]);

        assert_eq!(zero.sign(), IntegerSign::NonNegative);
        assert!(zero.magnitude().is_empty());

        assert_eq!(positive.magnitude(), &[5]);
    }
}
