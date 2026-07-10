use bray_symbols::{SymbolKind, SynthesizedSymbolRole};

use super::{ExportedLookupKind, InterfaceProductKind, SymbolRelationshipKind};

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

#[cfg(test)]
mod tests {
    use bray_symbols::SymbolKind;

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
}
