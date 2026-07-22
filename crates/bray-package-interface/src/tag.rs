use bray_bound_tree::{CheckedTemplateKind, CheckedTemplateShortCircuitKind};
use bray_symbols::{
    BorrowKind, CallableAbi, CallableConstness, CallableExecution, CallableParameterMode,
    CallablePosition, CallableTrust, ConstantBinaryOperation, ConstantUnaryOperation,
    CurrentRunCancellation, LifecycleObligationKind, ReceiverMode, SymbolKind,
    SynthesizedSymbolRole, TargetSizedIntegerType,
};

use super::{
    ExportedLookupKind, InterfaceProductKind, InterfaceSemanticFactKind, SymbolRelationshipKind,
};

pub(crate) trait WireTag: Sized {
    fn from_wire(value: u32) -> Option<Self>;
    fn to_wire(self) -> u32;
}

macro_rules! wire_tags {
    ($type:ty { $($value:literal => $variant:path),+ $(,)? }) => {
        impl WireTag for $type {
            fn from_wire(value: u32) -> Option<Self> {
                match value {
                    $($value => Some($variant),)+
                    _ => None,
                }
            }

            fn to_wire(self) -> u32 {
                match self {
                    $($variant => $value,)+
                }
            }
        }
    };
}

wire_tags!(InterfaceProductKind {
    1 => InterfaceProductKind::Library,
    2 => InterfaceProductKind::Executable,
    3 => InterfaceProductKind::Test,
});

wire_tags!(ExportedLookupKind {
    1 => ExportedLookupKind::Direct,
    2 => ExportedLookupKind::ReExport,
});

wire_tags!(SymbolRelationshipKind {
    1 => SymbolRelationshipKind::ModuleMember,
    2 => SymbolRelationshipKind::StructField,
    3 => SymbolRelationshipKind::UnionVariant,
    4 => SymbolRelationshipKind::UnionPayloadField,
    5 => SymbolRelationshipKind::GenericParameter,
    6 => SymbolRelationshipKind::CallableParameter,
    7 => SymbolRelationshipKind::PredicateParameter,
    8 => SymbolRelationshipKind::OverloadArm,
    9 => SymbolRelationshipKind::ImplementationFulfillment,
    10 => SymbolRelationshipKind::DefaultProvider,
    11 => SymbolRelationshipKind::PackageModule,
    12 => SymbolRelationshipKind::TypeMember,
    13 => SymbolRelationshipKind::TraitMember,
    14 => SymbolRelationshipKind::ImplementationMember,
});

wire_tags!(SynthesizedSymbolRole {
    1 => SynthesizedSymbolRole::ReceiverParameter,
    2 => SynthesizedSymbolRole::InferredImplementationTypeParameter,
    3 => SynthesizedSymbolRole::InferredImplementationConstParameter,
    4 => SynthesizedSymbolRole::CallableParameterDefaultProvider,
    5 => SynthesizedSymbolRole::StructFieldDefaultProvider,
    6 => SynthesizedSymbolRole::UnionPayloadDefaultProvider,
    7 => SynthesizedSymbolRole::DeclaredGenericTypeParameter,
    8 => SynthesizedSymbolRole::DeclaredGenericConstParameter,
    9 => SynthesizedSymbolRole::CallableParameter,
});

wire_tags!(SymbolKind {
    1 => SymbolKind::CompilerKnownEnvironment,
    2 => SymbolKind::Package,
    3 => SymbolKind::Module,
    4 => SymbolKind::Constant,
    5 => SymbolKind::Function,
    6 => SymbolKind::Predicate,
    7 => SymbolKind::CallableContract,
    8 => SymbolKind::CallableOverload,
    9 => SymbolKind::ImplementationOverload,
    10 => SymbolKind::Struct,
    11 => SymbolKind::Union,
    12 => SymbolKind::Trait,
    13 => SymbolKind::InherentImplementation,
    14 => SymbolKind::UnnamedTraitImplementation,
    15 => SymbolKind::NamedTraitImplementation,
    16 => SymbolKind::StructField,
    17 => SymbolKind::UnionVariant,
    18 => SymbolKind::UnionPayloadField,
    19 => SymbolKind::TypeCallableMember,
    20 => SymbolKind::Constructor,
    21 => SymbolKind::Finalizer,
    22 => SymbolKind::Destructor,
    23 => SymbolKind::ScopeEnter,
    24 => SymbolKind::ScopeExit,
    25 => SymbolKind::InherentTypeMember,
    26 => SymbolKind::TraitCallableMember,
    27 => SymbolKind::TraitConstantMember,
    28 => SymbolKind::TraitTypeMember,
    29 => SymbolKind::TraitPredicateMember,
    30 => SymbolKind::TraitFinalizerRequirement,
    31 => SymbolKind::TraitDestructorRequirement,
    32 => SymbolKind::TraitScopeEnterRequirement,
    33 => SymbolKind::TraitScopeExitRequirement,
    34 => SymbolKind::TraitCallableFulfillment,
    35 => SymbolKind::TraitConstantFulfillment,
    36 => SymbolKind::TraitTypeFulfillment,
    37 => SymbolKind::TraitPredicateFulfillment,
    38 => SymbolKind::TraitScopeEnterFulfillment,
    39 => SymbolKind::TraitScopeExitFulfillment,
    40 => SymbolKind::GenericTypeParameter,
    41 => SymbolKind::GenericConstParameter,
    42 => SymbolKind::CallableParameter,
    43 => SymbolKind::PredicateParameter,
    44 => SymbolKind::ReceiverParameter,
    45 => SymbolKind::CallableParameterDefaultProvider,
    46 => SymbolKind::StructFieldDefaultProvider,
    47 => SymbolKind::UnionPayloadDefaultProvider,
    48 => SymbolKind::LocalBinding,
    49 => SymbolKind::LocalConstant,
    50 => SymbolKind::AnonymousCallable,
    51 => SymbolKind::AnonymousCallableParameter,
    52 => SymbolKind::PostconditionResult,
});

wire_tags!(BorrowKind {
    1 => BorrowKind::Shared,
    2 => BorrowKind::Mutable,
});

wire_tags!(CallablePosition {
    1 => CallablePosition::NamedOnly,
    2 => CallablePosition::PositionalOrNamed,
});

wire_tags!(CallableParameterMode {
    1 => CallableParameterMode::Immutable,
    2 => CallableParameterMode::Mutable,
});

wire_tags!(ReceiverMode {
    1 => ReceiverMode::Shared,
    2 => ReceiverMode::Mutable,
    3 => ReceiverMode::Consuming,
    4 => ReceiverMode::ConsumingMutable,
});

wire_tags!(CallableConstness {
    1 => CallableConstness::Runtime,
    2 => CallableConstness::Constant,
});

wire_tags!(CallableExecution {
    1 => CallableExecution::Synchronous,
    2 => CallableExecution::Asynchronous,
});

wire_tags!(CallableTrust {
    1 => CallableTrust::Safe,
    2 => CallableTrust::Trusted,
});

wire_tags!(CallableAbi {
    1 => CallableAbi::Bray,
    2 => CallableAbi::C,
    3 => CallableAbi::System,
});

wire_tags!(ConstantUnaryOperation {
    1 => ConstantUnaryOperation::Identity,
    2 => ConstantUnaryOperation::Negate,
    3 => ConstantUnaryOperation::LogicalNot,
    4 => ConstantUnaryOperation::BitwiseNot,
});

wire_tags!(TargetSizedIntegerType {
    1 => TargetSizedIntegerType::Isize,
    2 => TargetSizedIntegerType::Usize,
});

wire_tags!(ConstantBinaryOperation {
    1 => ConstantBinaryOperation::Add,
    2 => ConstantBinaryOperation::Subtract,
    3 => ConstantBinaryOperation::Multiply,
    4 => ConstantBinaryOperation::Divide,
    5 => ConstantBinaryOperation::Remainder,
    6 => ConstantBinaryOperation::LogicalAnd,
    7 => ConstantBinaryOperation::LogicalOr,
    8 => ConstantBinaryOperation::BitwiseAnd,
    9 => ConstantBinaryOperation::BitwiseOr,
    10 => ConstantBinaryOperation::BitwiseXor,
    11 => ConstantBinaryOperation::ShiftLeft,
    12 => ConstantBinaryOperation::ShiftRight,
    13 => ConstantBinaryOperation::Equal,
    14 => ConstantBinaryOperation::NotEqual,
    15 => ConstantBinaryOperation::Less,
    16 => ConstantBinaryOperation::LessOrEqual,
    17 => ConstantBinaryOperation::Greater,
    18 => ConstantBinaryOperation::GreaterOrEqual,
    19 => ConstantBinaryOperation::Exponentiate,
});

wire_tags!(InterfaceSemanticFactKind {
    1 => InterfaceSemanticFactKind::GenericConstraint,
    2 => InterfaceSemanticFactKind::CallableContracts,
    3 => InterfaceSemanticFactKind::Implementation,
    4 => InterfaceSemanticFactKind::TargetFact,
    5 => InterfaceSemanticFactKind::Abi,
    6 => InterfaceSemanticFactKind::DeclarationTemplate,
    7 => InterfaceSemanticFactKind::CallableSignature,
    8 => InterfaceSemanticFactKind::GenericDeclaration,
    9 => InterfaceSemanticFactKind::CallableParameterDefault,
});

wire_tags!(CheckedTemplateKind {
    1 => CheckedTemplateKind::RuntimeDefault,
    2 => CheckedTemplateKind::ConstantDefinition,
    3 => CheckedTemplateKind::PredicateDefinition,
    4 => CheckedTemplateKind::GenericConstraint,
    5 => CheckedTemplateKind::CallableContract,
});

wire_tags!(CheckedTemplateShortCircuitKind {
    1 => CheckedTemplateShortCircuitKind::And,
    2 => CheckedTemplateShortCircuitKind::Or,
});

wire_tags!(LifecycleObligationKind {
    1 => LifecycleObligationKind::Destruction,
    2 => LifecycleObligationKind::Finalization,
    3 => LifecycleObligationKind::Cancellation,
    4 => LifecycleObligationKind::Joining,
});

wire_tags!(CurrentRunCancellation {
    1 => CurrentRunCancellation::NotEntered,
    2 => CurrentRunCancellation::MayEnter,
});

#[cfg(test)]
mod tests {
    use bray_bound_tree::{CheckedTemplateKind, CheckedTemplateShortCircuitKind};
    use bray_symbols::{CurrentRunCancellation, LifecycleObligationKind, SymbolKind};

    use super::WireTag;
    use crate::SymbolRelationshipKind;

    #[test]
    fn symbol_kind_tags_are_exact_and_closed() {
        for value in 1..=52 {
            let Some(kind) = SymbolKind::from_wire(value) else {
                panic!("known symbol kind tag was rejected: {value}");
            };

            assert_eq!(kind.to_wire(), value);
        }

        assert_eq!(SymbolKind::from_wire(0), None);
        assert_eq!(SymbolKind::from_wire(53), None);
    }

    #[test]
    fn relationship_tags_are_exact_and_closed() {
        for value in 1..=14 {
            let Some(kind) = SymbolRelationshipKind::from_wire(value) else {
                panic!("known relationship tag was rejected: {value}");
            };

            assert_eq!(kind.to_wire(), value);
        }

        assert_eq!(SymbolRelationshipKind::from_wire(0), None);
        assert_eq!(SymbolRelationshipKind::from_wire(15), None);
    }

    #[test]
    fn checked_template_tags_are_exact_and_closed() {
        let kinds = [
            CheckedTemplateKind::RuntimeDefault,
            CheckedTemplateKind::ConstantDefinition,
            CheckedTemplateKind::PredicateDefinition,
            CheckedTemplateKind::GenericConstraint,
            CheckedTemplateKind::CallableContract,
        ];

        for (index, kind) in kinds.into_iter().enumerate() {
            let wire = index_u32(index + 1);

            assert_eq!(CheckedTemplateKind::from_wire(wire), Some(kind));
            assert_eq!(kind.to_wire(), wire);
        }

        assert_eq!(CheckedTemplateKind::from_wire(0), None);
        assert_eq!(CheckedTemplateKind::from_wire(6), None);

        assert_eq!(CheckedTemplateShortCircuitKind::And.to_wire(), 1);
        assert_eq!(CheckedTemplateShortCircuitKind::Or.to_wire(), 2);
        assert_eq!(CheckedTemplateShortCircuitKind::from_wire(3), None);
    }

    #[test]
    fn lifecycle_obligation_tags_are_shared_by_contracts_and_templates() {
        let obligations = [
            LifecycleObligationKind::Destruction,
            LifecycleObligationKind::Finalization,
            LifecycleObligationKind::Cancellation,
            LifecycleObligationKind::Joining,
        ];

        for (index, obligation) in obligations.into_iter().enumerate() {
            let wire = index_u32(index + 1);

            assert_eq!(LifecycleObligationKind::from_wire(wire), Some(obligation));
            assert_eq!(obligation.to_wire(), wire);
        }

        assert_eq!(LifecycleObligationKind::from_wire(0), None);
        assert_eq!(LifecycleObligationKind::from_wire(5), None);
    }

    #[test]
    fn current_run_cancellation_tags_are_shared_by_contracts_and_templates() {
        assert_eq!(CurrentRunCancellation::NotEntered.to_wire(), 1);
        assert_eq!(CurrentRunCancellation::MayEnter.to_wire(), 2);

        assert_eq!(
            CurrentRunCancellation::from_wire(2),
            Some(CurrentRunCancellation::MayEnter)
        );

        assert_eq!(CurrentRunCancellation::from_wire(0), None);
        assert_eq!(CurrentRunCancellation::from_wire(3), None);
    }

    fn index_u32(index: usize) -> u32 {
        match u32::try_from(index) {
            Ok(index) => index,
            Err(_) => panic!("small test index must fit in u32"),
        }
    }
}
