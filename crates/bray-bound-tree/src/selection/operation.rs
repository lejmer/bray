use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::{
    AnySymbolId, CallableInstanceData, CallableParameterDefaultProviderSymbolId,
    CallableParameterSymbolId, CallableSignature, ImplementationRequirementKey,
    ReceiverParameterSignature, StructFieldDefaultProviderSymbolId, StructFieldSymbolId,
    StructSymbolId, TypeId, UnionPayloadDefaultProviderSymbolId, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

use crate::{
    BoundExpression, BoundExpressionId, BoundOperator, BoundStructuredExpressionKind,
    SelectedImplementationWitness,
};

/// The semantic operation category retained by a checked selection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectionKind {
    /// An ordinary callable, method, or overload arm.
    Callable,
    /// A receiver-associated member.
    Member,
    /// A unary or binary operator implementation.
    Operator,
    /// An element or slice indexing contract.
    Index,
    /// A struct, variant, or type-form construction operation.
    Construction,
    /// An explicit conversion operation.
    Conversion,
    /// A trait implementation witness.
    Implementation,
}

impl SelectionKind {
    /// Returns this selection kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Callable => "callable",
            Self::Member => "member",
            Self::Operator => "operator",
            Self::Index => "index",
            Self::Construction => "construction",
            Self::Conversion => "conversion",
            Self::Implementation => "implementation",
        }
    }
}

/// The exact semantic target of a member selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberTarget {
    member: AnySymbolId,
    result_type: TypeId,
    callable_instance: Option<CallableInstanceData>,
    callable_signature: Option<CallableSignature>,
    generic_dispatch: Option<bray_symbols::GenericConstraintDispatch>,
    witnesses: Arc<[SelectedImplementationWitness]>,
}

impl MemberTarget {
    /// Creates one exact member target and its selected implementation witnesses.
    pub fn new(
        member: AnySymbolId,
        result_type: TypeId,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            member,
            result_type,
            callable_instance: None,
            callable_signature: None,
            generic_dispatch: None,
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Returns a member target with its exact substituted callable instance and signature.
    pub fn with_callable(
        mut self,
        callable: CallableInstanceData,
        signature: CallableSignature,
    ) -> Self {
        self.callable_instance = Some(callable);
        self.callable_signature = Some(signature);

        self
    }

    /// Returns a member target dispatched through one surrounding generic constraint.
    pub const fn with_generic_dispatch(
        mut self,
        dispatch: bray_symbols::GenericConstraintDispatch,
    ) -> Self {
        self.generic_dispatch = Some(dispatch);

        self
    }

    /// Returns the exact selected member.
    pub const fn member(&self) -> AnySymbolId {
        self.member
    }

    /// Returns the member access result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns the exact substituted callable when this member can be invoked.
    pub const fn callable_instance(&self) -> Option<CallableInstanceData> {
        self.callable_instance
    }

    /// Returns the resolved callable signature when this target is callable.
    pub const fn callable_signature(&self) -> Option<&CallableSignature> {
        self.callable_signature.as_ref()
    }

    /// Returns the resolved implicit receiver when this target is callable.
    pub const fn receiver(&self) -> Option<ReceiverParameterSignature> {
        match &self.callable_signature {
            Some(signature) => signature.receiver(),
            None => None,
        }
    }

    /// Returns the surrounding generic constraint that supplies dispatch.
    pub const fn generic_dispatch(&self) -> Option<bray_symbols::GenericConstraintDispatch> {
        self.generic_dispatch
    }

    /// Returns implementation requirements and witnesses in canonical order.
    pub fn witnesses(&self) -> &[SelectedImplementationWitness] {
        &self.witnesses
    }
}

/// The exact implementation of a source unary or binary operator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperatorTarget {
    /// Compiler-defined behavior for a non-overloadable operator.
    BuiltIn(BoundOperator),
    /// A selected compiler-known operator trait member and implementation witness.
    Trait {
        /// The exact source operator contract being implemented.
        operator: BoundOperator,
        /// The exact substituted trait member selected for the operation.
        member: CallableInstanceData,
        /// The exact substituted implementation callable that executes the operation.
        fulfillment: CallableInstanceData,
        /// The exact trait requirement selected for the operands.
        requirement: ImplementationRequirementKey,
        /// The exact implementation witness.
        witness: bray_symbols::ImplementationInstanceId,
    },
}

impl OperatorTarget {
    /// Returns the exact source operator implemented by this target.
    pub const fn operator(self) -> BoundOperator {
        match self {
            Self::BuiltIn(operator) | Self::Trait { operator, .. } => operator,
        }
    }
}

/// The exact implementation of element or slice access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexTarget {
    /// Built-in fixed-array element projection.
    ArrayElement,
    /// Built-in slice element projection.
    SliceElement,
    /// Built-in fixed-array slice projection.
    ArraySlice,
    /// Built-in slice projection.
    Slice,
    /// A selected custom indexing contract callable and witness.
    Custom {
        /// The exact substituted trait member selected for indexing.
        member: CallableInstanceData,
        /// The exact substituted implementation callable that executes indexing.
        fulfillment: CallableInstanceData,
        /// The exact trait requirement selected for the subject and selectors.
        requirement: ImplementationRequirementKey,
        /// The exact implementation witness.
        witness: bray_symbols::ImplementationInstanceId,
    },
}

/// The exact declaration or type-form behavior used for construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstructionTarget {
    /// A named struct and its field surface.
    Struct(StructSymbolId),
    /// A union variant and its payload surface.
    UnionVariant(UnionVariantSymbolId),
    /// A selected type-form construction callable and implementation witness.
    TypeForm {
        /// The exact callable that performs construction.
        callable: CallableInstanceData,
        /// The exact implementation requirement selected for the type form.
        requirement: ImplementationRequirementKey,
        /// The exact implementation witness.
        witness: bray_symbols::ImplementationInstanceId,
    },
}

impl ConstructionTarget {
    /// Returns whether this target owns the supplied construction input category.
    pub const fn accepts_input(self, input: ConstructionInputId) -> bool {
        matches!(
            (self, input),
            (Self::Struct(_), ConstructionInputId::StructField(_))
                | (
                    Self::UnionVariant(_),
                    ConstructionInputId::UnionPayloadField(_)
                )
                | (
                    Self::TypeForm { .. },
                    ConstructionInputId::CallableParameter(_)
                )
        )
    }
}

/// One exact declaration input initialized by construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstructionInputId {
    /// A struct field.
    StructField(StructFieldSymbolId),
    /// A union variant payload field.
    UnionPayloadField(UnionPayloadFieldSymbolId),
    /// A type-form construction parameter.
    CallableParameter(CallableParameterSymbolId),
}

impl ConstructionInputId {
    /// Returns whether this input category owns the supplied default provider category.
    pub const fn accepts_default(self, provider: ConstructionDefaultProvider) -> bool {
        matches!(
            (self, provider),
            (
                Self::StructField(_),
                ConstructionDefaultProvider::StructField(_)
            ) | (
                Self::UnionPayloadField(_),
                ConstructionDefaultProvider::UnionPayload(_)
            ) | (
                Self::CallableParameter(_),
                ConstructionDefaultProvider::CallableParameter(_)
            )
        )
    }
}

/// One declaration-owned runtime default used by construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstructionDefaultProvider {
    /// A struct field default.
    StructField(StructFieldDefaultProviderSymbolId),
    /// A union payload field default.
    UnionPayload(UnionPayloadDefaultProviderSymbolId),
    /// A type-form callable parameter default.
    CallableParameter(CallableParameterDefaultProviderSymbolId),
}

impl ConstructionDefaultProvider {
    /// Returns the declaration symbol that owns this runtime default.
    pub const fn symbol(self) -> AnySymbolId {
        match self {
            Self::StructField(symbol) => AnySymbolId::StructFieldDefaultProvider(symbol),
            Self::UnionPayload(symbol) => AnySymbolId::UnionPayloadDefaultProvider(symbol),
            Self::CallableParameter(symbol) => {
                AnySymbolId::CallableParameterDefaultProvider(symbol)
            }
        }
    }
}

/// One supplied or defaulted construction value in evaluation order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectedConstructionInput {
    /// A source initializer mapped to its exact declaration input.
    Explicit {
        /// The source expression occurrence.
        expression: BoundExpressionId,
        /// The exact initialized field or parameter.
        input: ConstructionInputId,
        /// The initialized input's declaration-order ordinal.
        ordinal: u32,
    },
    /// An omitted declaration input supplied by its runtime default.
    Default {
        /// The exact initialized field or parameter.
        input: ConstructionInputId,
        /// The declaration-owned provider evaluated by construction.
        provider: ConstructionDefaultProvider,
        /// The initialized input's declaration-order ordinal.
        ordinal: u32,
    },
}

/// One exact construction target and normalized initializer mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedConstruction {
    target: ConstructionTarget,
    result_type: TypeId,
    inputs: Arc<[SelectedConstructionInput]>,
}

impl SelectedConstruction {
    /// Creates a complete construction selection.
    pub fn new(
        target: ConstructionTarget,
        result_type: TypeId,
        inputs: impl IntoIterator<Item = SelectedConstructionInput>,
    ) -> Self {
        Self {
            target,
            result_type,
            inputs: shared_slice(inputs),
        }
    }

    /// Returns the exact declaration or type-form construction target.
    pub const fn target(&self) -> ConstructionTarget {
        self.target
    }

    /// Returns the constructed semantic type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns supplied inputs in source order followed by defaults in declaration order.
    pub fn inputs(&self) -> &[SelectedConstructionInput] {
        &self.inputs
    }
}

/// The exact rule used by one level of an explicit conversion.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ConversionTarget {
    /// Source and target are the same semantic type.
    Identity,
    /// A compiler-defined total value-preserving scalar conversion.
    BuiltInScalar,
    /// A compiler-defined structural conversion with exact nested conversion plans.
    Composite(Arc<[SelectedConversion]>),
    /// A selected `ConvertTo<Target>` member and implementation witness.
    Trait {
        /// The exact substituted trait member selected for conversion.
        member: CallableInstanceData,
        /// The exact substituted implementation callable that executes the conversion.
        fulfillment: CallableInstanceData,
        /// The exact conversion trait requirement.
        requirement: ImplementationRequirementKey,
        /// The exact implementation witness.
        witness: bray_symbols::ImplementationInstanceId,
    },
}

/// One exact source-to-target conversion plan.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedConversion {
    source_type: TypeId,
    target_type: TypeId,
    target: ConversionTarget,
}

impl SelectedConversion {
    /// Creates one explicit conversion plan.
    pub const fn new(source_type: TypeId, target_type: TypeId, target: ConversionTarget) -> Self {
        Self {
            source_type,
            target_type,
            target,
        }
    }

    /// Returns the converted source type.
    pub const fn source_type(&self) -> TypeId {
        self.source_type
    }

    /// Returns the explicit target type.
    pub const fn target_type(&self) -> TypeId {
        self.target_type
    }

    /// Returns the exact selected conversion rule.
    pub const fn target(&self) -> &ConversionTarget {
        &self.target
    }
}

/// One exact operation whose operands have already been associated by binding and checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedOperation {
    /// A receiver-associated member.
    Member(MemberTarget),
    /// A unary or binary operator.
    Operator {
        /// The selected implementation.
        target: OperatorTarget,
        /// The expression result type.
        result_type: TypeId,
    },
    /// Element or slice indexing.
    Index {
        /// The selected indexing contract.
        target: IndexTarget,
        /// The expression result type.
        result_type: TypeId,
    },
    /// Struct, union variant, or type-form construction.
    Construction(SelectedConstruction),
    /// An explicit conversion.
    Conversion(SelectedConversion),
    /// A trait implementation witness selected for an exact requirement.
    Implementation(SelectedImplementationWitness),
}

impl SelectedOperation {
    /// Returns this operation's closed selection category.
    pub const fn kind(&self) -> SelectionKind {
        match self {
            Self::Member(_) => SelectionKind::Member,
            Self::Operator { .. } => SelectionKind::Operator,
            Self::Index { .. } => SelectionKind::Index,
            Self::Construction(_) => SelectionKind::Construction,
            Self::Conversion(_) => SelectionKind::Conversion,
            Self::Implementation(_) => SelectionKind::Implementation,
        }
    }

    /// Returns the selected operation's result type when it produces a value.
    pub const fn result_type(&self) -> Option<TypeId> {
        match self {
            Self::Member(target) => Some(target.result_type()),
            Self::Operator { result_type, .. } | Self::Index { result_type, .. } => {
                Some(*result_type)
            }
            Self::Construction(construction) => Some(construction.result_type()),
            Self::Conversion(conversion) => Some(conversion.target_type()),
            Self::Implementation(_) => None,
        }
    }

    /// Returns whether this operation can describe the supplied bound expression category.
    pub fn matches_expression(&self, expression: &BoundExpression) -> bool {
        match (self, expression) {
            (Self::Member(_), BoundExpression::MemberAccess(_))
            | (Self::Member(_), BoundExpression::TraitQualifiedMember(_)) => true,
            (Self::Operator { target, .. }, BoundExpression::Unary(source)) => {
                target.operator() == source.operator()
            }
            (Self::Operator { target, .. }, BoundExpression::Binary(source)) => {
                target.operator() == source.operator()
            }
            (Self::Index { target, .. }, BoundExpression::Structured(source)) => {
                index_target_matches(*target, source.kind())
            }
            (Self::Construction(construction), BoundExpression::StructConstruction(_)) => {
                matches!(construction.target(), ConstructionTarget::Struct(_))
            }
            (Self::Construction(construction), BoundExpression::Structured(source)) => {
                matches!(construction.target(), ConstructionTarget::TypeForm { .. })
                    && source.kind() == BoundStructuredExpressionKind::TypeFormConstruction
            }
            (
                Self::Construction(construction),
                BoundExpression::LeadingDotVariant(_)
                | BoundExpression::UnqualifiedVariant(_)
                | BoundExpression::MemberAccess(_)
                | BoundExpression::Call(_),
            ) => matches!(construction.target(), ConstructionTarget::UnionVariant(_)),
            (Self::Conversion(conversion), BoundExpression::Conversion(source)) => source
                .target_type()
                .is_none_or(|target| target == conversion.target_type()),
            (Self::Implementation(_), _) => true,
            _ => false,
        }
    }

    /// Returns every exact implementation requirement and witness used by this operation.
    pub fn witnesses(&self) -> Vec<SelectedImplementationWitness> {
        let mut witnesses = match self {
            Self::Member(target) => target.witnesses().to_vec(),
            Self::Operator {
                target:
                    OperatorTarget::Trait {
                        requirement,
                        witness,
                        ..
                    },
                ..
            }
            | Self::Index {
                target:
                    IndexTarget::Custom {
                        requirement,
                        witness,
                        ..
                    },
                ..
            } => vec![SelectedImplementationWitness::new(*requirement, *witness)],
            Self::Conversion(conversion) => conversion_witnesses(conversion),
            Self::Implementation(witness) => vec![*witness],
            Self::Construction(construction) => match construction.target() {
                ConstructionTarget::TypeForm {
                    requirement,
                    witness,
                    ..
                } => vec![SelectedImplementationWitness::new(requirement, witness)],
                ConstructionTarget::Struct(_) | ConstructionTarget::UnionVariant(_) => Vec::new(),
            },
            Self::Operator { .. } | Self::Index { .. } => Vec::new(),
        };

        witnesses.sort_unstable();
        witnesses.dedup();

        witnesses
    }
}

fn conversion_witnesses(conversion: &SelectedConversion) -> Vec<SelectedImplementationWitness> {
    let mut witnesses = Vec::new();
    let mut pending = vec![conversion];

    while let Some(conversion) = pending.pop() {
        match conversion.target() {
            ConversionTarget::Trait {
                requirement,
                witness,
                ..
            } => witnesses.push(SelectedImplementationWitness::new(*requirement, *witness)),
            ConversionTarget::Composite(children) => pending.extend(children.iter()),
            ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
        }
    }

    witnesses
}

const fn index_target_matches(target: IndexTarget, source: BoundStructuredExpressionKind) -> bool {
    match source {
        BoundStructuredExpressionKind::ElementIndex => matches!(
            target,
            IndexTarget::ArrayElement | IndexTarget::SliceElement | IndexTarget::Custom { .. }
        ),
        BoundStructuredExpressionKind::SliceIndex => matches!(
            target,
            IndexTarget::ArraySlice | IndexTarget::Slice | IndexTarget::Custom { .. }
        ),
        _ => false,
    }
}
