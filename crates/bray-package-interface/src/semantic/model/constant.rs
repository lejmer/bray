use std::sync::Arc;

use bray_symbols::{
    ConstantBinaryOperation, ConstantField, ConstantUnaryOperation, IntegerConstant,
    RealConstantBits, SymbolOrdinal, TargetSizedIntegerType,
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

    /// Returns the durable closed-value payload.
    pub const fn kind(&self) -> &InterfaceConstantValueKind {
        &self.kind
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
    Product(Arc<[ConstantField<InterfaceSymbolReference, InterfaceConstantValueId>]>),
    /// Active union variant and payload.
    Union {
        /// Active variant.
        variant: InterfaceSymbolReference,
        /// Ordered payload fields.
        fields: Arc<[ConstantField<InterfaceSymbolReference, InterfaceConstantValueId>]>,
    },
}

/// Durable checked open constant term.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceConstantTerm {
    /// A term retained with its exact checked result type.
    Typed {
        /// Retained term.
        term: InterfaceConstantTermId,
        /// Exact checked result type.
        ty: InterfaceTypeId,
    },
    /// A closed constant value.
    Value(InterfaceConstantValueId),
    /// A typed integer literal awaiting selected-target representability checking.
    IntegerLiteral {
        /// The literal's established target-sized integer type.
        ty: TargetSizedIntegerType,
        /// The normalized source value.
        value: IntegerConstant,
    },
    /// A generic constant parameter.
    Parameter(InterfaceSymbolReference),
    /// A const-callable argument addressed in receiver-first call order.
    CallableArgument(SymbolOrdinal),
    /// A compiler-known target property.
    TargetProperty(InterfaceSymbolReference),
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
    /// A selected compiler-defined conversion.
    Conversion {
        /// Converted operand.
        operand: InterfaceConstantTermId,
        /// Exact conversion target type.
        target: InterfaceTypeId,
    },
    /// Nullable presence around an open or closed child term.
    NullablePresent(InterfaceConstantTermId),
    /// An ordered tuple whose elements may remain open.
    Tuple(Arc<[InterfaceConstantTermId]>),
    /// An ordered array whose elements may remain open.
    Array(Arc<[InterfaceConstantTermId]>),
    /// Ordered product fields whose values may remain open.
    Product(Arc<[ConstantField<InterfaceSymbolReference, InterfaceConstantTermId>]>),
    /// An active union variant whose payload values may remain open.
    Union {
        /// Exact active variant.
        variant: InterfaceSymbolReference,
        /// Ordered payload fields.
        fields: Arc<[ConstantField<InterfaceSymbolReference, InterfaceConstantTermId>]>,
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
        /// Selected implementation whose fulfillment supplies the callable, when applicable.
        selected_implementation: Option<InterfaceImplementationInstanceId>,
        /// Ordered argument terms.
        arguments: Arc<[InterfaceConstantTermId]>,
    },
    /// A checked predicate application.
    PredicateCall {
        /// Predicate declaration.
        predicate: InterfaceSymbolReference,
        /// Ordered generic substitution.
        substitution: InterfaceGenericSubstitutionId,
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

/// Durable constant projection using interface-local semantic references.
pub type InterfaceConstantProjection = bray_symbols::ConstantProjectionKind<
    InterfaceConstantTermId,
    InterfaceSymbolReference,
    InterfaceSymbolReference,
>;
