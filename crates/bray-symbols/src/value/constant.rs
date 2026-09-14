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

/// A language-defined integer type whose width comes from the selected target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetSizedIntegerType {
    /// The signed machine-sized integer type.
    Isize,
    /// The unsigned machine-sized integer type.
    Usize,
}

/// One exact aggregate field and its associated constant payload.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantField<I, V> {
    field: I,
    value: V,
}

impl<I, V> ConstantField<I, V> {
    /// Creates a field payload with its stable declaration identity.
    pub const fn new(field: I, value: V) -> Self {
        Self { field, value }
    }

    /// Returns the exact field declaration.
    pub const fn field(&self) -> &I {
        &self.field
    }

    /// Returns the payload associated with the field.
    pub const fn value(&self) -> &V {
        &self.value
    }
}

/// A host-independent arbitrary-width normalized integer constant.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IntegerConstant {
    sign: IntegerSign,
    magnitude: Arc<[u8]>,
}

impl IntegerConstant {
    /// Creates a nonnegative integer from an unsigned 64-bit value.
    pub fn from_u64(value: u64) -> Self {
        Self::new(IntegerSign::NonNegative, value.to_be_bytes())
    }

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

    /// Returns whether this integer is zero.
    pub fn is_zero(&self) -> bool {
        self.magnitude.is_empty()
    }

    /// Returns whether this integer is greater than zero.
    pub fn is_positive(&self) -> bool {
        self.sign == IntegerSign::NonNegative && !self.is_zero()
    }

    /// Converts this value to an unsigned 64-bit integer when representable.
    pub fn to_u64(&self) -> Option<u64> {
        self.to_u128().and_then(|value| u64::try_from(value).ok())
    }

    /// Converts this value to an unsigned 128-bit integer when representable.
    pub fn to_u128(&self) -> Option<u128> {
        if self.sign != IntegerSign::NonNegative {
            return None;
        }

        self.magnitude.iter().try_fold(0_u128, |value, byte| {
            value.checked_mul(256)?.checked_add(u128::from(*byte))
        })
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
    /// A shared address of one selected static instance in a static initializer.
    StaticAddress(crate::StaticReferenceSelection),
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
    Product(Arc<[ConstantField<StructFieldSymbolId, ConstantValueId>]>),
    /// An active union variant and its ordered payload values.
    Union {
        /// Exact active variant.
        variant: UnionVariantSymbolId,
        /// Ordered payload values.
        fields: Arc<[ConstantField<UnionPayloadFieldSymbolId, ConstantValueId>]>,
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
    pub fn product(
        fields: impl IntoIterator<Item = ConstantField<StructFieldSymbolId, ConstantValueId>>,
    ) -> Self {
        Self::Product(shared_slice(fields))
    }

    /// Creates an active union variant constant payload.
    pub fn union(
        variant: UnionVariantSymbolId,
        fields: impl IntoIterator<Item = ConstantField<UnionPayloadFieldSymbolId, ConstantValueId>>,
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

    /// Separates the exact semantic type from the owned constant payload.
    pub fn into_parts(self) -> (TypeId, ConstantValueKind) {
        (self.ty, self.kind)
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
    /// Exponentiation.
    Exponentiate,
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
pub enum ConstantProjectionKind<
    Term = ConstantTermId,
    ProductField = StructFieldSymbolId,
    UnionField = UnionPayloadFieldSymbolId,
> {
    /// A tuple element by stable ordinal.
    TupleElement(SymbolOrdinal),
    /// An array element selected by a checked constant term.
    ArrayElement(Term),
    /// A half-open array slice with checked optional bounds.
    ArraySlice {
        /// Inclusive start. Absence means zero.
        lower: Option<Term>,
        /// Exclusive end. Absence means the subject length.
        upper: Option<Term>,
    },
    /// A named product field.
    ProductField(ProductField),
    /// A named union payload field.
    UnionPayloadField(UnionField),
    /// The present value of a nullable subject.
    NullableValue,
}

impl<Term: Copy, ProductField, UnionField> ConstantProjectionKind<Term, ProductField, UnionField> {
    /// Visits checked selector terms in evaluation order. Omitted bounds have no dependency.
    pub fn term_references(&self) -> impl Iterator<Item = Term> {
        match self {
            Self::ArrayElement(index) => [Some(*index), None],
            Self::ArraySlice { lower, upper } => [*lower, *upper],
            Self::TupleElement(_)
            | Self::ProductField(_)
            | Self::UnionPayloadField(_)
            | Self::NullableValue => [None, None],
        }
        .into_iter()
        .flatten()
    }
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
    /// A term retained with its exact checked result type.
    Typed {
        /// Retained term.
        term: ConstantTermId,
        /// Exact checked result type.
        ty: TypeId,
    },
    /// A fully evaluated closed value.
    Value(ConstantValueId),
    /// A typed integer literal awaiting selected-target representability checking.
    IntegerLiteral {
        /// The literal's established target-sized integer type.
        ty: TargetSizedIntegerType,
        /// The normalized source value.
        value: IntegerConstant,
    },
    /// A generic constant parameter.
    Parameter(GenericConstParameterSymbolId),
    /// A const-callable argument addressed in receiver-first call order.
    CallableArgument(SymbolOrdinal),
    /// A compiler-known target property represented by its exact constant declaration.
    TargetProperty(ConstantSymbolId),
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
    /// A selected compiler-defined conversion.
    Conversion {
        /// Converted operand.
        operand: ConstantTermId,
        /// Exact conversion target type.
        target: TypeId,
    },
    /// Nullable presence around an open or closed child term.
    NullablePresent(ConstantTermId),
    /// An ordered tuple whose elements may remain open.
    Tuple(Arc<[ConstantTermId]>),
    /// An ordered array whose elements may remain open.
    Array(Arc<[ConstantTermId]>),
    /// Ordered product fields whose values may remain open.
    Product(Arc<[ConstantField<StructFieldSymbolId, ConstantTermId>]>),
    /// An active union variant whose payload values may remain open.
    Union {
        /// Exact active variant.
        variant: UnionVariantSymbolId,
        /// Ordered payload fields.
        fields: Arc<[ConstantField<UnionPayloadFieldSymbolId, ConstantTermId>]>,
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
        /// Selected implementation whose fulfillment supplies the callable, when applicable.
        selected_implementation: Option<ImplementationInstanceId>,
        /// Ordered argument terms.
        arguments: Arc<[ConstantTermId]>,
    },
    /// A checked predicate application that can remain open.
    PredicateCall {
        /// Exact substituted predicate.
        predicate: crate::PredicateInstanceData,
        /// Ordered argument terms.
        arguments: Arc<[ConstantTermId]>,
    },
    /// A checked projection from another term.
    Projection(ConstantProjection),
}

impl ConstantTermData {
    /// Retains a term with its exact checked result type.
    pub const fn typed(term: ConstantTermId, ty: TypeId) -> Self {
        Self::Typed { term, ty }
    }

    /// Creates an ordered tuple constant term.
    pub fn tuple(values: impl IntoIterator<Item = ConstantTermId>) -> Self {
        Self::Tuple(shared_slice(values))
    }

    /// Creates an ordered array constant term.
    pub fn array(values: impl IntoIterator<Item = ConstantTermId>) -> Self {
        Self::Array(shared_slice(values))
    }

    /// Creates an ordered product constant term.
    pub fn product(
        fields: impl IntoIterator<Item = ConstantField<StructFieldSymbolId, ConstantTermId>>,
    ) -> Self {
        Self::Product(shared_slice(fields))
    }

    /// Creates an active union variant constant term.
    pub fn union(
        variant: UnionVariantSymbolId,
        fields: impl IntoIterator<Item = ConstantField<UnionPayloadFieldSymbolId, ConstantTermId>>,
    ) -> Self {
        Self::Union {
            variant,
            fields: shared_slice(fields),
        }
    }

    /// Creates a checked constant-call term.
    pub fn call(
        callable: CallableInstanceId,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: impl IntoIterator<Item = ConstantTermId>,
    ) -> Self {
        Self::Call {
            callable,
            selected_implementation,
            arguments: shared_slice(arguments),
        }
    }

    /// Creates a checked predicate application.
    pub fn predicate_call(
        predicate: crate::PredicateInstanceData,
        arguments: impl IntoIterator<Item = ConstantTermId>,
    ) -> Self {
        Self::PredicateCall {
            predicate,
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
        assert!(zero.is_zero());

        assert_eq!(positive.magnitude(), &[5]);
    }

    #[test]
    fn integer_constants_convert_to_u64_only_when_representable() {
        let maximum = IntegerConstant::new(IntegerSign::NonNegative, [0xff; 8]);
        let overflow = IntegerConstant::new(IntegerSign::NonNegative, [1, 0, 0, 0, 0, 0, 0, 0, 0]);
        let maximum_u128 = IntegerConstant::new(IntegerSign::NonNegative, [0xff; 16]);

        let overflow_u128 = IntegerConstant::new(
            IntegerSign::NonNegative,
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        let negative = IntegerConstant::new(IntegerSign::Negative, [1]);

        assert_eq!(maximum.to_u64(), Some(u64::MAX));
        assert_eq!(overflow.to_u64(), None);
        assert_eq!(maximum_u128.to_u128(), Some(u128::MAX));
        assert_eq!(overflow_u128.to_u128(), None);
        assert_eq!(negative.to_u64(), None);
        assert_eq!(negative.to_u128(), None);
    }

    #[test]
    fn integer_constants_construct_from_unsigned_values() {
        assert!(IntegerConstant::from_u64(0).magnitude().is_empty());
        assert_eq!(IntegerConstant::from_u64(256).magnitude(), [1, 0]);
    }
}
