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
    callable: CallableInstanceId,
    parameter: ReceiverParameterSymbolId,
    expression: BoundExpressionId,
}

impl CheckedReceiverArgument {
    /// Creates a receiver-to-parameter association.
    pub const fn new(
        callable: CallableInstanceId,
        parameter: ReceiverParameterSymbolId,
        expression: BoundExpressionId,
    ) -> Self {
        Self {
            callable,
            parameter,
            expression,
        }
    }

    /// Returns the exact direct callable that owns this receiver parameter.
    pub const fn callable(self) -> CallableInstanceId {
        self.callable
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
    Declared {
        /// Exact selected direct callable instance.
        callable: CallableInstanceId,
        /// Exact declared parameter identity.
        parameter: CallableParameterSymbolId,
        /// The parameter's declaration-order ordinal.
        ordinal: SymbolOrdinal,
    },
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
    callable: CallableInstanceId,
    parameter: CallableParameterSymbolId,
    provider: CallableParameterDefaultProviderSymbolId,
    ordinal: SymbolOrdinal,
    substitution: Option<GenericSubstitutionId>,
}

impl CheckedDefaultArgument {
    /// Creates an omitted-parameter default association.
    pub const fn new(
        callable: CallableInstanceId,
        parameter: CallableParameterSymbolId,
        provider: CallableParameterDefaultProviderSymbolId,
        ordinal: SymbolOrdinal,
        substitution: Option<GenericSubstitutionId>,
    ) -> Self {
        Self {
            callable,
            parameter,
            provider,
            ordinal,
            substitution,
        }
    }

    /// Returns the exact direct callable that owns this omitted parameter and provider.
    pub const fn callable(self) -> CallableInstanceId {
        self.callable
    }

    /// Returns the exact omitted parameter.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the checked runtime-default provider.
    pub const fn provider(self) -> CallableParameterDefaultProviderSymbolId {
        self.provider
    }

    /// Returns the omitted parameter's declaration-order ordinal.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CheckedParameterIdentity {
    Declared(CallableInstanceId, CallableParameterSymbolId),
    Anonymous(AnonymousCallableParameterSymbolId),
    Indirect(SymbolOrdinal),
}

/// Normalized receiver, explicit-argument, and default associations for one call.
///
/// Explicit arguments remain in source evaluation order. Defaults remain in parameter
/// declaration order and therefore follow every explicit argument during evaluation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedArgumentMapping {
    abi: CallableAbi,
    receiver: Option<CheckedReceiverArgument>,
    explicit: Arc<[CheckedExplicitArgument]>,
    defaults: Arc<[CheckedDefaultArgument]>,
}

impl CheckedArgumentMapping {
    /// Creates a mapping after validating conversions and rejecting duplicate parameters.
    pub fn try_new(
        abi: CallableAbi,
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

        for parameter in
            explicit
                .iter()
                .map(|argument| argument.parameter())
                .chain(
                    defaults
                        .iter()
                        .map(|argument| CheckedParameterTarget::Declared {
                            callable: argument.callable(),
                            parameter: argument.parameter(),
                            ordinal: argument.ordinal(),
                        }),
                )
        {
            if !parameters.insert(parameter.identity()) {
                return Err(CheckedArgumentMappingError::DuplicateParameter(parameter));
            }
        }

        Ok(Self {
            abi,
            receiver,
            explicit: shared_slice(explicit),
            defaults: shared_slice(defaults),
        })
    }

    /// Returns the ABI used when this mapping was checked.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
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

    pub(crate) fn is_valid_for(
        &self,
        callable: CheckedCallableReference,
        unit: BoundUnitId,
    ) -> bool {
        let shape_is_valid = match callable.target() {
            CheckedCallableTarget::Direct(target) => {
                self.receiver
                    .is_none_or(|receiver| receiver.callable() == target)
                    && self.explicit.iter().all(|argument| {
                        matches!(
                            argument.parameter(),
                            CheckedParameterTarget::Declared { callable, .. } if callable == target
                        )
                    })
                    && self
                        .defaults
                        .iter()
                        .all(|argument| argument.callable() == target)
                    && self
                        .defaults
                        .windows(2)
                        .all(|arguments| arguments[0].ordinal() < arguments[1].ordinal())
            }
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

        self.abi == callable.abi()
            && shape_is_valid
            && self
                .receiver
                .is_none_or(|receiver| receiver.expression().unit() == unit)
            && self
                .explicit
                .iter()
                .all(|argument| argument.expression().unit() == unit)
    }
}

impl CheckedParameterTarget {
    const fn identity(self) -> CheckedParameterIdentity {
        match self {
            Self::Declared {
                callable,
                parameter,
                ..
            } => CheckedParameterIdentity::Declared(callable, parameter),
            Self::Anonymous(parameter) => CheckedParameterIdentity::Anonymous(parameter),
            Self::Indirect(ordinal) => CheckedParameterIdentity::Indirect(ordinal),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableAbi, CallableDefinitionId, CallableInstanceData, CallableInstanceId,
        CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, FunctionSymbolId,
        GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        ReceiverParameterSymbolId, SymbolId, SymbolOrdinal,
    };

    use super::{
        CheckedArgumentMapping, CheckedArgumentMappingError, CheckedCallableReference,
        CheckedCallableTarget, CheckedDefaultArgument, CheckedExplicitArgument,
        CheckedParameterTarget, CheckedReceiverArgument,
    };
    use crate::{BoundExpressionId, BoundUnitId, CheckedConversion, CheckedConversionKind};

    #[test]
    fn argument_mappings_reject_duplicate_explicit_and_default_parameters() {
        let callable = callable_instance(1);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let provider = CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(4));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(2), 0);
        let conversion = identity_conversion();

        let result = CheckedArgumentMapping::try_new(
            CallableAbi::Bray,
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Declared {
                    callable,
                    parameter,
                    ordinal: SymbolOrdinal::new(0),
                },
                expression,
                conversion,
            )],
            [CheckedDefaultArgument::new(
                callable,
                parameter,
                provider,
                SymbolOrdinal::new(0),
                None,
            )],
        );

        assert_eq!(
            result,
            Err(CheckedArgumentMappingError::DuplicateParameter(
                CheckedParameterTarget::Declared {
                    callable,
                    parameter,
                    ordinal: SymbolOrdinal::new(0),
                }
            ))
        );
    }

    #[test]
    fn indirect_calls_require_parameter_ordinals_without_receivers_or_defaults() {
        let unit = BoundUnitId::new(7);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let ty = crate::test_support::error_type();
        let indirect = match CheckedArgumentMapping::try_new(
            CallableAbi::Bray,
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

        assert!(indirect.is_valid_for(
            CheckedCallableReference::new(CheckedCallableTarget::Indirect(ty), CallableAbi::Bray),
            unit,
        ));
    }

    #[test]
    fn argument_mappings_reject_structurally_invalid_conversions() {
        let callable = callable_instance(1);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(8));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(9), 0);
        let result = CheckedArgumentMapping::try_new(
            CallableAbi::Bray,
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Declared {
                    callable,
                    parameter,
                    ordinal: SymbolOrdinal::new(0),
                },
                expression,
                invalid_identity_conversion(),
            )],
            [],
        );

        assert_eq!(
            result,
            Err(CheckedArgumentMappingError::InvalidConversion(
                CheckedParameterTarget::Declared {
                    callable,
                    parameter,
                    ordinal: SymbolOrdinal::new(0),
                }
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

    #[test]
    fn direct_calls_reject_foreign_callable_argument_plans_and_abi() {
        let first = callable_instance(1);
        let second = callable_instance(2);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let provider = CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(4));
        let expression = BoundExpressionId::from_slot(BoundUnitId::new(5), 0);
        let mapping = match CheckedArgumentMapping::try_new(
            CallableAbi::Bray,
            None,
            [CheckedExplicitArgument::new(
                CheckedParameterTarget::Declared {
                    callable: first,
                    parameter,
                    ordinal: SymbolOrdinal::new(0),
                },
                expression,
                identity_conversion(),
            )],
            [CheckedDefaultArgument::new(
                first,
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(6)),
                provider,
                SymbolOrdinal::new(1),
                None,
            )],
        ) {
            Ok(mapping) => mapping,
            Err(error) => panic!("test argument mapping must be structurally valid: {error:?}"),
        };

        assert!(!mapping.is_valid_for(
            CheckedCallableReference::new(CheckedCallableTarget::Direct(second), CallableAbi::Bray),
            BoundUnitId::new(5),
        ));
        assert!(!mapping.is_valid_for(
            CheckedCallableReference::new(CheckedCallableTarget::Direct(first), CallableAbi::C),
            BoundUnitId::new(5),
        ));

        let receiver_mapping = match CheckedArgumentMapping::try_new(
            CallableAbi::Bray,
            Some(CheckedReceiverArgument::new(
                first,
                ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(7)),
                expression,
            )),
            [],
            [],
        ) {
            Ok(mapping) => mapping,
            Err(error) => panic!("test receiver mapping must be structurally valid: {error:?}"),
        };

        assert!(!receiver_mapping.is_valid_for(
            CheckedCallableReference::new(CheckedCallableTarget::Direct(second), CallableAbi::Bray),
            BoundUnitId::new(5),
        ));
    }

    fn callable_instance(symbol: u32) -> CallableInstanceId {
        let values = crate::test_support::semantic_values();
        let definition = FunctionSymbolId::from_symbol_id(SymbolId::new(symbol));
        let Some(owner) = GenericOwnerId::try_new(definition.into()) else {
            panic!("function must be a generic owner");
        };
        let substitution = match GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        ) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty substitution must be valid: {error:?}"),
        };
        let substitution = match values.intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("substitution must be interned: {error:?}"),
        };
        let Some(definition) = CallableDefinitionId::try_new(definition.into()) else {
            panic!("function must be callable");
        };

        match values.intern_callable_instance(CallableInstanceData::new(definition, substitution)) {
            Ok(instance) => instance,
            Err(error) => panic!("callable instance must be interned: {error:?}"),
        }
    }
}
