use std::{collections::BTreeSet, sync::Arc};

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    ConstantValueId, GenericSubstitutionId, ImplementationInstanceId,
    StructFieldDefaultProviderSymbolId, StructFieldSymbolId, StructSymbolId, SymbolOrdinal, TypeId,
    UnionPayloadDefaultProviderSymbolId, UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use crate::{BoundExpressionId, BoundOperator, BoundUnitId};

use super::{CheckedCallableReference, CheckedConversion};

/// The canonical typed value selected for one literal expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedLiteral {
    value: ConstantValueId,
}

impl CheckedLiteral {
    /// Creates a literal fact from its canonical typed value.
    pub const fn new(value: ConstantValueId) -> Self {
        Self { value }
    }

    /// Returns the canonical materializable literal value.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }
}

/// The exact semantic declaration or tuple component selected by member access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedMemberTarget {
    /// An exact declared value, member, field, or variant identity.
    Declaration(AnySymbolId),
    /// A stable tuple element ordinal.
    TupleElement(SymbolOrdinal),
    /// A payload field whose containing variant is proven active.
    ActiveUnionPayloadField {
        /// The active variant required by this access.
        variant: UnionVariantSymbolId,
        /// The exact selected payload field.
        field: UnionPayloadFieldSymbolId,
    },
}

/// A resolved member target and any implementation witnesses used to reach it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedMemberSelection {
    target: CheckedMemberTarget,
    implementation_witnesses: Arc<[ImplementationInstanceId]>,
}

impl CheckedMemberSelection {
    /// Creates a member selection with witnesses in canonical semantic-set order.
    pub fn new(
        target: CheckedMemberTarget,
        implementation_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
    ) -> Self {
        Self {
            target,
            implementation_witnesses: sorted_unique_shared_slice(implementation_witnesses),
        }
    }

    /// Returns the exact selected member target.
    pub const fn target(&self) -> CheckedMemberTarget {
        self.target
    }

    /// Returns selected implementation witnesses in canonical semantic-set order.
    pub fn implementation_witnesses(&self) -> &[ImplementationInstanceId] {
        &self.implementation_witnesses
    }
}

/// The exact built-in or custom indexing contract selected for an access.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CheckedIndexTarget {
    /// Fixed-array element access.
    ArrayElement,
    /// Slice element access.
    SliceElement,
    /// Contiguous projection from a fixed array.
    ArraySlice,
    /// Contiguous projection from a slice.
    SliceRange,
    /// A selected custom indexing contract operation.
    Custom {
        /// Exact participating implementation.
        implementation: ImplementationInstanceId,
        /// Exact indexing callable and ABI.
        callable: CheckedCallableReference,
    },
}

/// A checked indexing selection ready for access and bounds lowering.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedIndexSelection {
    target: CheckedIndexTarget,
}

impl CheckedIndexSelection {
    /// Creates a checked indexing selection.
    pub const fn new(target: CheckedIndexTarget) -> Self {
        Self { target }
    }

    /// Returns the selected built-in or custom indexing contract.
    pub const fn target(&self) -> &CheckedIndexTarget {
        &self.target
    }
}

/// The exact built-in or callable operation selected for a source operator.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedOperatorTarget {
    /// The source operator is implemented by Bray's built-in value operation.
    BuiltIn(BoundOperator),
    /// The source operator is implemented by one selected callable.
    Callable {
        /// Exact participating implementation.
        implementation: ImplementationInstanceId,
        /// Exact operator callable and ABI.
        callable: CheckedCallableReference,
    },
}

/// A checked source operator selection ready for value lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedOperatorSelection {
    target: CheckedOperatorTarget,
}

impl CheckedOperatorSelection {
    /// Creates a checked operator selection.
    pub const fn new(target: CheckedOperatorTarget) -> Self {
        Self { target }
    }

    /// Returns the selected built-in or callable operation.
    pub const fn target(self) -> CheckedOperatorTarget {
        self.target
    }
}

/// A compiler-recognized type-form construction operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTypeFormConstruction {
    /// Owned indirection through an explicit storage policy.
    OwnedIndirection {
        /// The selected storage-policy type.
        storage: TypeId,
        /// The value type allocated into the owned storage.
        target: TypeId,
        /// Exact checked allocation and construction operation.
        operation: CheckedCallableReference,
    },
}

/// The exact value-construction mechanism selected by checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedConstructionTarget {
    /// Direct construction of one struct type.
    Struct(StructSymbolId),
    /// Direct construction of one active union variant.
    UnionVariant(UnionVariantSymbolId),
    /// Construction through an ordinary selected constructor callable.
    Callable(CheckedCallableReference),
    /// Compiler-recognized type-form construction.
    TypeForm(CheckedTypeFormConstruction),
}

/// The exact declaration slot initialized by one construction input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedConstructionInputTarget {
    /// A struct field.
    StructField(StructFieldSymbolId),
    /// A union variant payload field.
    UnionPayloadField(UnionPayloadFieldSymbolId),
    /// A selected constructor parameter.
    CallableParameter(CallableParameterSymbolId),
}

/// The explicit expression or runtime-default provider supplying a construction input.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CheckedConstructionInputValue {
    /// An explicit source expression evaluated in source order.
    Explicit {
        /// The source expression supplying the value.
        expression: BoundExpressionId,
        /// The checked conversion from the source value to the field or parameter type.
        conversion: CheckedConversion,
    },
    /// A selected struct-field runtime default.
    StructFieldDefault {
        /// Exact synthesized provider identity.
        provider: StructFieldDefaultProviderSymbolId,
        /// Selected substitution for a generic provider.
        substitution: Option<GenericSubstitutionId>,
    },
    /// A selected union-payload runtime default.
    UnionPayloadDefault {
        /// Exact synthesized provider identity.
        provider: UnionPayloadDefaultProviderSymbolId,
        /// Selected substitution for a generic provider.
        substitution: Option<GenericSubstitutionId>,
    },
    /// A selected callable-parameter runtime default.
    CallableDefault {
        /// Exact synthesized provider identity.
        provider: CallableParameterDefaultProviderSymbolId,
        /// Selected substitution for a generic provider.
        substitution: Option<GenericSubstitutionId>,
    },
}

/// One normalized input to a checked construction operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedConstructionInput {
    target: CheckedConstructionInputTarget,
    value: CheckedConstructionInputValue,
}

impl CheckedConstructionInput {
    /// Creates one target-to-value construction association.
    pub const fn new(
        target: CheckedConstructionInputTarget,
        value: CheckedConstructionInputValue,
    ) -> Self {
        Self { target, value }
    }

    /// Returns the exact initialized field or parameter.
    pub const fn target(&self) -> CheckedConstructionInputTarget {
        self.target
    }

    /// Returns the explicit expression or selected runtime default.
    pub const fn value(&self) -> &CheckedConstructionInputValue {
        &self.value
    }
}

/// A malformed normalized construction input mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedConstructionError {
    /// One field or parameter is initialized more than once.
    DuplicateInput(CheckedConstructionInputTarget),
    /// An input target or value category does not belong to the selected construction mechanism.
    IncompatibleInput(CheckedConstructionInputTarget),
    /// One explicit input carries a structurally invalid conversion plan.
    InvalidConversion(CheckedConstructionInputTarget),
    /// An explicit source input appeared after runtime-default evaluation began.
    ExplicitInputAfterDefault(CheckedConstructionInputTarget),
}

/// A complete checked value-construction target and evaluation-ordered inputs.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedConstruction {
    target: CheckedConstructionTarget,
    inputs: Arc<[CheckedConstructionInput]>,
}

impl CheckedConstruction {
    /// Creates a construction after validating input categories, conversions, and uniqueness.
    ///
    /// Explicit inputs must be supplied in source order, followed by omitted defaults in
    /// declaration order.
    pub fn try_new(
        target: CheckedConstructionTarget,
        inputs: impl IntoIterator<Item = CheckedConstructionInput>,
    ) -> Result<Self, CheckedConstructionError> {
        let inputs: Vec<_> = inputs.into_iter().collect();
        let mut targets = BTreeSet::new();
        let mut reached_default = false;

        for input in &inputs {
            if !construction_input_matches(target, input) {
                return Err(CheckedConstructionError::IncompatibleInput(input.target()));
            }

            if matches!(
                input.value(),
                CheckedConstructionInputValue::Explicit { conversion, .. }
                    if !conversion.is_structurally_valid()
            ) {
                return Err(CheckedConstructionError::InvalidConversion(input.target()));
            }

            let is_explicit = matches!(
                input.value(),
                CheckedConstructionInputValue::Explicit { .. }
            );

            if is_explicit && reached_default {
                return Err(CheckedConstructionError::ExplicitInputAfterDefault(
                    input.target(),
                ));
            }

            if !targets.insert(input.target()) {
                return Err(CheckedConstructionError::DuplicateInput(input.target()));
            }

            reached_default |= !is_explicit;
        }

        Ok(Self {
            target,
            inputs: shared_slice(inputs),
        })
    }

    /// Returns the exact selected construction mechanism.
    pub const fn target(&self) -> CheckedConstructionTarget {
        self.target
    }

    /// Returns explicit inputs and defaults in evaluation order.
    pub fn inputs(&self) -> &[CheckedConstructionInput] {
        &self.inputs
    }

    pub(crate) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.inputs.iter().all(|input| match input.value() {
            CheckedConstructionInputValue::Explicit { expression, .. } => expression.unit() == unit,
            CheckedConstructionInputValue::StructFieldDefault { .. }
            | CheckedConstructionInputValue::UnionPayloadDefault { .. }
            | CheckedConstructionInputValue::CallableDefault { .. } => true,
        })
    }

    pub(crate) fn explicit_expressions(&self) -> impl Iterator<Item = BoundExpressionId> + '_ {
        self.inputs.iter().filter_map(|input| match input.value() {
            CheckedConstructionInputValue::Explicit { expression, .. } => Some(*expression),
            CheckedConstructionInputValue::StructFieldDefault { .. }
            | CheckedConstructionInputValue::UnionPayloadDefault { .. }
            | CheckedConstructionInputValue::CallableDefault { .. } => None,
        })
    }

    pub(crate) fn explicit_inputs(
        &self,
    ) -> impl Iterator<Item = (BoundExpressionId, &CheckedConversion)> {
        self.inputs.iter().filter_map(|input| match input.value() {
            CheckedConstructionInputValue::Explicit {
                expression,
                conversion,
            } => Some((*expression, conversion)),
            CheckedConstructionInputValue::StructFieldDefault { .. }
            | CheckedConstructionInputValue::UnionPayloadDefault { .. }
            | CheckedConstructionInputValue::CallableDefault { .. } => None,
        })
    }
}

const fn construction_input_matches(
    construction: CheckedConstructionTarget,
    input: &CheckedConstructionInput,
) -> bool {
    matches!(
        (construction, input.target(), input.value()),
        (
            CheckedConstructionTarget::Struct(_),
            CheckedConstructionInputTarget::StructField(_),
            CheckedConstructionInputValue::Explicit { .. }
                | CheckedConstructionInputValue::StructFieldDefault { .. },
        ) | (
            CheckedConstructionTarget::UnionVariant(_),
            CheckedConstructionInputTarget::UnionPayloadField(_),
            CheckedConstructionInputValue::Explicit { .. }
                | CheckedConstructionInputValue::UnionPayloadDefault { .. },
        ) | (
            CheckedConstructionTarget::Callable(_) | CheckedConstructionTarget::TypeForm(_),
            CheckedConstructionInputTarget::CallableParameter(_),
            CheckedConstructionInputValue::Explicit { .. }
                | CheckedConstructionInputValue::CallableDefault { .. },
        )
    )
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        StructFieldDefaultProviderSymbolId, StructFieldSymbolId, StructSymbolId, SymbolId,
    };

    use super::{
        CheckedConstruction, CheckedConstructionError, CheckedConstructionInput,
        CheckedConstructionInputTarget, CheckedConstructionInputValue, CheckedConstructionTarget,
    };
    use crate::{BoundExpressionId, BoundUnitId, CheckedConversion, CheckedConversionKind};

    #[test]
    fn constructions_reject_duplicate_target_fields() {
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(1));
        let field = StructFieldSymbolId::from_symbol_id(SymbolId::new(2));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(3), 0);
        let input = CheckedConstructionInput::new(
            CheckedConstructionInputTarget::StructField(field),
            CheckedConstructionInputValue::Explicit {
                expression,
                conversion: identity_conversion(),
            },
        );

        let result = CheckedConstruction::try_new(
            CheckedConstructionTarget::Struct(structure),
            [input.clone(), input],
        );

        assert_eq!(
            result,
            Err(CheckedConstructionError::DuplicateInput(
                CheckedConstructionInputTarget::StructField(field)
            ))
        );
    }

    #[test]
    fn constructions_validate_input_categories_and_evaluation_order() {
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(1));
        let first_field = StructFieldSymbolId::from_symbol_id(SymbolId::new(2));
        let second_field = StructFieldSymbolId::from_symbol_id(SymbolId::new(3));
        let provider = StructFieldDefaultProviderSymbolId::from_symbol_id(SymbolId::new(4));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(5), 0);
        let explicit = CheckedConstructionInput::new(
            CheckedConstructionInputTarget::StructField(second_field),
            CheckedConstructionInputValue::Explicit {
                expression,
                conversion: identity_conversion(),
            },
        );
        let default = CheckedConstructionInput::new(
            CheckedConstructionInputTarget::StructField(first_field),
            CheckedConstructionInputValue::StructFieldDefault {
                provider,
                substitution: None,
            },
        );

        assert_eq!(
            CheckedConstruction::try_new(
                CheckedConstructionTarget::Struct(structure),
                [default, explicit],
            ),
            Err(CheckedConstructionError::ExplicitInputAfterDefault(
                CheckedConstructionInputTarget::StructField(second_field),
            ))
        );

        let incompatible = CheckedConstructionInput::new(
            CheckedConstructionInputTarget::StructField(first_field),
            CheckedConstructionInputValue::CallableDefault {
                provider: bray_symbols::CallableParameterDefaultProviderSymbolId::from_symbol_id(
                    SymbolId::new(6),
                ),
                substitution: None,
            },
        );

        assert_eq!(
            CheckedConstruction::try_new(
                CheckedConstructionTarget::Struct(structure),
                [incompatible],
            ),
            Err(CheckedConstructionError::IncompatibleInput(
                CheckedConstructionInputTarget::StructField(first_field)
            ))
        );

        let invalid_conversion = CheckedConstructionInput::new(
            CheckedConstructionInputTarget::StructField(first_field),
            CheckedConstructionInputValue::Explicit {
                expression,
                conversion: invalid_identity_conversion(),
            },
        );

        assert_eq!(
            CheckedConstruction::try_new(
                CheckedConstructionTarget::Struct(structure),
                [invalid_conversion],
            ),
            Err(CheckedConstructionError::InvalidConversion(
                CheckedConstructionInputTarget::StructField(first_field)
            ))
        );
    }

    fn identity_conversion() -> CheckedConversion {
        let ty = crate::test_support::error_type();

        CheckedConversion::new(ty, ty, CheckedConversionKind::Identity)
    }

    fn invalid_identity_conversion() -> CheckedConversion {
        let values = crate::test_support::semantic_values();
        let source = crate::test_support::error_type_in(&values);
        let target = crate::test_support::tuple_type_in(&values);

        CheckedConversion::new(source, target, CheckedConversionKind::Identity)
    }
}
