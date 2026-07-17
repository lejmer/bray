use std::{collections::BTreeSet, sync::Arc};

use bray_base::shared_slice;
use bray_symbols::{
    AnonymousCallableParameterSymbolId, AnonymousCallableSymbolId, CallableAbi, CallableInstanceId,
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, GenericSubstitutionId,
    ReceiverParameterSymbolId, SymbolOrdinal, TypeId,
};

use crate::{BoundExpressionId, BoundUnitId, CheckedConversion};

/// The exact callable mechanism selected for one checked invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedCallableTarget {
    /// A canonical substituted callable declaration.
    Direct(CallableInstanceId),
    /// A separately checked anonymous callable unit.
    Anonymous(AnonymousCallableSymbolId),
    /// A callable value whose complete contract is represented by its type.
    Indirect(TypeId),
}

impl CheckedCallableTarget {
    /// Returns the canonical callable instance for a direct call.
    pub const fn direct(self) -> Option<CallableInstanceId> {
        match self {
            Self::Direct(instance) => Some(instance),
            Self::Anonymous(_) | Self::Indirect(_) => None,
        }
    }
}

/// An exact checked callable target and calling convention.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedCallableReference {
    target: CheckedCallableTarget,
    abi: CallableAbi,
}

impl CheckedCallableReference {
    /// Creates a callable reference after selection and ABI checking.
    pub const fn new(target: CheckedCallableTarget, abi: CallableAbi) -> Self {
        Self { target, abi }
    }

    /// Returns the exact selected callable mechanism.
    pub const fn target(self) -> CheckedCallableTarget {
        self.target
    }

    /// Returns the checked calling convention.
    pub const fn abi(self) -> CallableAbi {
        self.abi
    }
}

/// The implicit receiver supplied to a selected method invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedReceiverArgument {
    parameter: ReceiverParameterSymbolId,
    expression: BoundExpressionId,
}

impl CheckedReceiverArgument {
    /// Creates a receiver-to-parameter association.
    pub const fn new(parameter: ReceiverParameterSymbolId, expression: BoundExpressionId) -> Self {
        Self {
            parameter,
            expression,
        }
    }

    /// Returns the selected callable's implicit receiver parameter.
    pub const fn parameter(self) -> ReceiverParameterSymbolId {
        self.parameter
    }

    /// Returns the receiver expression evaluated before written arguments.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }
}

/// The exact selected parameter for one explicit argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedParameterTarget {
    /// A parameter declared by a compilation-wide callable definition.
    Declared(CallableParameterSymbolId),
    /// A parameter owned by a separately checked anonymous callable.
    Anonymous(AnonymousCallableParameterSymbolId),
    /// A parameter position in an indirect callable type.
    Indirect(SymbolOrdinal),
}

/// One explicit argument mapped to its selected callable parameter.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedExplicitArgument {
    parameter: CheckedParameterTarget,
    expression: BoundExpressionId,
    conversion: CheckedConversion,
}

impl CheckedExplicitArgument {
    /// Creates an explicit source-argument association.
    pub const fn new(
        parameter: CheckedParameterTarget,
        expression: BoundExpressionId,
        conversion: CheckedConversion,
    ) -> Self {
        Self {
            parameter,
            expression,
            conversion,
        }
    }

    /// Returns the exact selected parameter.
    pub const fn parameter(&self) -> CheckedParameterTarget {
        self.parameter
    }

    /// Returns the source argument expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the checked conversion from the source value to the parameter type.
    pub const fn conversion(&self) -> &CheckedConversion {
        &self.conversion
    }
}

/// One omitted parameter and the runtime-default provider selected for it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedDefaultArgument {
    parameter: CallableParameterSymbolId,
    provider: CallableParameterDefaultProviderSymbolId,
    substitution: Option<GenericSubstitutionId>,
}

impl CheckedDefaultArgument {
    /// Creates an omitted-parameter default association.
    pub const fn new(
        parameter: CallableParameterSymbolId,
        provider: CallableParameterDefaultProviderSymbolId,
        substitution: Option<GenericSubstitutionId>,
    ) -> Self {
        Self {
            parameter,
            provider,
            substitution,
        }
    }

    /// Returns the exact omitted parameter.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the checked runtime-default provider.
    pub const fn provider(self) -> CallableParameterDefaultProviderSymbolId {
        self.provider
    }

    /// Returns the selected generic substitution for a generic default provider.
    pub const fn substitution(self) -> Option<GenericSubstitutionId> {
        self.substitution
    }
}

/// A malformed normalized call-argument mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedArgumentMappingError {
    /// One selected parameter is supplied more than once.
    DuplicateParameter(CheckedParameterTarget),
    /// An explicit argument carries a structurally invalid conversion plan.
    InvalidConversion(CheckedParameterTarget),
}

/// Normalized receiver, explicit-argument, and default associations for one call.
///
/// Explicit arguments remain in source evaluation order. Defaults remain in parameter
/// declaration order and therefore follow every explicit argument during evaluation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedArgumentMapping {
    receiver: Option<CheckedReceiverArgument>,
    explicit: Arc<[CheckedExplicitArgument]>,
    defaults: Arc<[CheckedDefaultArgument]>,
}

impl CheckedArgumentMapping {
    /// Creates a mapping after validating conversions and rejecting duplicate parameters.
    pub fn try_new(
        receiver: Option<CheckedReceiverArgument>,
        explicit: impl IntoIterator<Item = CheckedExplicitArgument>,
        defaults: impl IntoIterator<Item = CheckedDefaultArgument>,
    ) -> Result<Self, CheckedArgumentMappingError> {
        let explicit: Vec<_> = explicit.into_iter().collect();
        let defaults: Vec<_> = defaults.into_iter().collect();
        let mut parameters = BTreeSet::new();

        for argument in &explicit {
            if !argument.conversion().is_structurally_valid() {
                return Err(CheckedArgumentMappingError::InvalidConversion(
                    argument.parameter(),
                ));
            }
        }

        for parameter in explicit.iter().map(|argument| argument.parameter()).chain(
            defaults
                .iter()
                .map(|argument| CheckedParameterTarget::Declared(argument.parameter())),
        ) {
            if !parameters.insert(parameter) {
                return Err(CheckedArgumentMappingError::DuplicateParameter(parameter));
            }
        }

        Ok(Self {
            receiver,
            explicit: shared_slice(explicit),
            defaults: shared_slice(defaults),
        })
    }

    /// Returns the implicit receiver association when this is a method call.
    pub const fn receiver(&self) -> Option<CheckedReceiverArgument> {
        self.receiver
    }

    /// Returns explicit arguments in source evaluation order.
    pub fn explicit(&self) -> &[CheckedExplicitArgument] {
        &self.explicit
    }

    /// Returns omitted defaults in parameter declaration order.
    pub fn defaults(&self) -> &[CheckedDefaultArgument] {
        &self.defaults
    }

    pub(crate) fn is_valid_for(&self, target: CheckedCallableTarget, unit: BoundUnitId) -> bool {
        let shape_is_valid = match target {
            CheckedCallableTarget::Direct(_) => self.explicit.iter().all(|argument| {
                matches!(argument.parameter(), CheckedParameterTarget::Declared(_))
            }),
            CheckedCallableTarget::Anonymous(_) => {
                self.receiver.is_none()
                    && self.defaults.is_empty()
                    && self.explicit.iter().all(|argument| {
                        matches!(argument.parameter(), CheckedParameterTarget::Anonymous(_))
                    })
            }
            CheckedCallableTarget::Indirect(_) => {
                self.receiver.is_none()
                    && self.defaults.is_empty()
                    && self.explicit.iter().all(|argument| {
                        matches!(argument.parameter(), CheckedParameterTarget::Indirect(_))
                    })
            }
        };

        shape_is_valid
            && self
                .receiver
                .is_none_or(|receiver| receiver.expression().unit() == unit)
            && self
                .explicit
                .iter()
                .all(|argument| argument.expression().unit() == unit)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, SymbolId,
        SymbolOrdinal,
    };

    use super::{
        CheckedArgumentMapping, CheckedArgumentMappingError, CheckedCallableTarget,
        CheckedDefaultArgument, CheckedExplicitArgument, CheckedParameterTarget,
    };
    use crate::{BoundExpressionId, BoundUnitId, CheckedConversion, CheckedConversionKind};

    #[test]
    fn argument_mappings_reject_duplicate_explicit_and_default_parameters() {
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let provider = CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(4));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(2), 0);
        let conversion = identity_conversion();

        let result = CheckedArgumentMapping::try_new(
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Declared(parameter),
                expression,
                conversion,
            )],
            [CheckedDefaultArgument::new(parameter, provider, None)],
        );

        assert_eq!(
            result,
            Err(CheckedArgumentMappingError::DuplicateParameter(
                CheckedParameterTarget::Declared(parameter)
            ))
        );
    }

    #[test]
    fn indirect_calls_require_parameter_ordinals_without_receivers_or_defaults() {
        let unit = BoundUnitId::new(7);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let ty = crate::test_support::error_type();
        let indirect = match CheckedArgumentMapping::try_new(
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Indirect(SymbolOrdinal::new(0)),
                expression,
                CheckedConversion::new(ty, ty, CheckedConversionKind::Identity),
            )],
            [],
        ) {
            Ok(mapping) => mapping,
            Err(error) => panic!("indirect argument mapping must be valid: {error:?}"),
        };

        assert!(indirect.is_valid_for(CheckedCallableTarget::Indirect(ty), unit));
    }

    #[test]
    fn argument_mappings_reject_structurally_invalid_conversions() {
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(8));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(9), 0);
        let result = CheckedArgumentMapping::try_new(
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Declared(parameter),
                expression,
                invalid_identity_conversion(),
            )],
            [],
        );

        assert_eq!(
            result,
            Err(CheckedArgumentMappingError::InvalidConversion(
                CheckedParameterTarget::Declared(parameter)
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
