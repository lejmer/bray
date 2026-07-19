use std::sync::Arc;

use bray_symbols::{
    ConstantBinaryOperation, ConstantUnaryOperation, IntegerConstant, RealConstantBits,
    SymbolOrdinal,
};

use super::{
    InterfaceCallableInstanceId, InterfaceConstantTermId, InterfaceConstantValueId,
    InterfaceGenericSubstitutionId, InterfaceImplementationInstanceId, InterfaceTypeId,
};
use crate::InterfaceSymbolReference;

/// Durable closed constant value representation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceConstantValue {
    pub(crate) ty: InterfaceTypeId,
    pub(crate) kind: InterfaceConstantValueKind,
}

impl InterfaceConstantValue {
    /// Creates one typed closed constant value.
    pub const fn new(ty: InterfaceTypeId, kind: InterfaceConstantValueKind) -> Self {
        Self { ty, kind }
    }
}

/// Durable payload of one closed constant value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceConstantValueKind {
    /// Boolean value.
    Boolean(bool),
    /// Unicode scalar value.
    Character(char),
    /// Arbitrary-width normalized integer.
    Integer(IntegerConstant),
    /// Selected-format real bits.
    Real(RealConstantBits),
    /// Selected-format complex bits.
    Complex {
        /// Real component.
        real: RealConstantBits,
        /// Imaginary component.
        imaginary: RealConstantBits,
    },
    /// Canonical string content.
    String(Arc<str>),
    /// Unit value.
    Unit,
    /// Nullable absence.
    NullableAbsent,
    /// Nullable presence.
    NullablePresent(InterfaceConstantValueId),
    /// Ordered tuple values.
    Tuple(Arc<[InterfaceConstantValueId]>),
    /// Ordered array values.
    Array(Arc<[InterfaceConstantValueId]>),
    /// Ordered product fields.
    Product(Arc<[InterfaceConstantValueId]>),
    /// Active union variant and payload.
    Union {
        /// Active variant.
        variant: InterfaceSymbolReference,
        /// Ordered payload fields.
        fields: Arc<[InterfaceConstantValueId]>,
    },
}

/// Durable checked open constant term.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceConstantTerm {
    /// A closed constant value.
    Value(InterfaceConstantValueId),
    /// A typed integer literal awaiting selected-target representability checking.
    IntegerLiteral {
        /// The literal's established integer type.
        ty: InterfaceTypeId,
        /// The normalized source value.
        value: IntegerConstant,
    },
    /// A generic constant parameter.
    Parameter(InterfaceSymbolReference),
    /// A compiler-known target fact.
    TargetFact(InterfaceSymbolReference),
    /// A selected unary operation.
    Unary {
        /// Operation.
        operation: ConstantUnaryOperation,
        /// Operand.
        operand: InterfaceConstantTermId,
    },
    /// A selected binary operation.
    Binary {
        /// Operation.
        operation: ConstantBinaryOperation,
        /// Left operand.
        left: InterfaceConstantTermId,
        /// Right operand.
        right: InterfaceConstantTermId,
    },
    /// An applied constant definition that remains open.
    DefinitionApplication {
        /// Exact constant definition.
        definition: InterfaceSymbolReference,
        /// Ordered generic substitution.
        substitution: InterfaceGenericSubstitutionId,
        /// Selected implementation witness when required.
        selected_implementation: Option<InterfaceImplementationInstanceId>,
    },
    /// A checked constant call.
    Call {
        /// Substituted callable.
        callable: InterfaceCallableInstanceId,
        /// Ordered argument terms.
        arguments: Arc<[InterfaceConstantTermId]>,
    },
    /// A checked projection from another term.
    Projection {
        /// Projected subject.
        subject: InterfaceConstantTermId,
        /// Exact projection operation.
        kind: InterfaceConstantProjection,
    },
}

/// Durable projection operation used by an open constant term.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceConstantProjection {
    /// Tuple element ordinal.
    TupleElement(SymbolOrdinal),
    /// Array element selected by a checked term.
    ArrayElement(InterfaceConstantTermId),
    /// Named product field.
    ProductField(InterfaceSymbolReference),
    /// Named union payload field.
    UnionPayloadField(InterfaceSymbolReference),
    /// Present nullable value.
    NullableValue,
}
