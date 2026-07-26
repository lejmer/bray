use crate::{AnySymbolId, GenericOwnerId, SymbolKind};

/// A semantic completion boundary for one declaration symbol.
///
/// Executable bodies use separate checked-unit facts and are deliberately absent from this
/// symbol completion family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolCompletionLevel {
    /// Only the deterministic symbol identity skeleton is required.
    Identity,
    /// Every applicable fact needed to describe the declaration surface is required.
    DeclarationSurface,
}

/// An exact category of semantic fact owned by a declaration symbol.
///
/// Instance-specific facts with additional semantic inputs use their own typed keys rather than
/// discarding those inputs into this symbol-only category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolFactKind {
    /// Typed members and their ordinary-name index.
    Members,
    /// Imports contributing to a module surface.
    Imports,
    /// Decoded declaration directives.
    Directives,
    /// Ordered generic parameters and their lookup indexes.
    GenericParameters,
    /// Generic parameter identities and unevaluated constraint expressions.
    GenericDeclarationTemplate,
    /// Checked generic constraints.
    GenericConstraints,
    /// The callable signature excluding executable body checking.
    CallableSignature,
    /// Checked callable contracts, effects, and capabilities.
    CallableContracts,
    /// Unevaluated callable contract expressions and capability paths.
    CallableContractTemplate,
    /// A predicate declaration signature template.
    PredicateSignatureTemplate,
    /// The callable type template named by a callable-contract declaration.
    CallableContractType,
    /// The declared type of a constant.
    ConstantDeclaredType,
    /// The checked definition template of a constant.
    ConstantDefinition,
    /// A checked callable parameter default.
    CallableParameterDefault,
    /// An unevaluated declaration default expression.
    UnevaluatedDefaultTemplate,
    /// A struct field type template.
    StructFieldType,
    /// The type-value template supplied by an implementation member.
    TypeMemberValue,
    /// A checked struct field default.
    StructFieldDefault,
    /// A union payload field type template.
    UnionPayloadFieldType,
    /// A checked union payload field default.
    UnionPayloadFieldDefault,
    /// A checked predicate definition.
    PredicateDefinition,
    /// A union variant's payload surface.
    UnionVariantPayload,
    /// An implementation's subject type template.
    ImplementationSubject,
    /// The trait-application template implemented by an implementation.
    ImplementedTraitApplication,
    /// A complete implementation header template.
    ImplementationHeadTemplate,
    /// Stable implementation coherence keys.
    ImplementationCoherence,
    /// The resolved arms of an overload family.
    OverloadArms,
    /// Source or resolved overload arms in declaration order.
    OverloadSignatureTemplate,
}

pub(super) const SYMBOL_FACT_KINDS: [SymbolFactKind; 28] = [
    SymbolFactKind::Members,
    SymbolFactKind::Imports,
    SymbolFactKind::Directives,
    SymbolFactKind::GenericParameters,
    SymbolFactKind::GenericDeclarationTemplate,
    SymbolFactKind::GenericConstraints,
    SymbolFactKind::CallableSignature,
    SymbolFactKind::CallableContracts,
    SymbolFactKind::CallableContractTemplate,
    SymbolFactKind::PredicateSignatureTemplate,
    SymbolFactKind::CallableContractType,
    SymbolFactKind::ConstantDeclaredType,
    SymbolFactKind::ConstantDefinition,
    SymbolFactKind::CallableParameterDefault,
    SymbolFactKind::UnevaluatedDefaultTemplate,
    SymbolFactKind::StructFieldType,
    SymbolFactKind::TypeMemberValue,
    SymbolFactKind::StructFieldDefault,
    SymbolFactKind::UnionPayloadFieldType,
    SymbolFactKind::UnionPayloadFieldDefault,
    SymbolFactKind::PredicateDefinition,
    SymbolFactKind::UnionVariantPayload,
    SymbolFactKind::ImplementationSubject,
    SymbolFactKind::ImplementedTraitApplication,
    SymbolFactKind::ImplementationHeadTemplate,
    SymbolFactKind::ImplementationCoherence,
    SymbolFactKind::OverloadArms,
    SymbolFactKind::OverloadSignatureTemplate,
];

impl SymbolFactKind {
    /// Returns whether this fact participates in the requested completion boundary when applicable.
    pub const fn is_required_for(self, level: SymbolCompletionLevel) -> bool {
        match level {
            SymbolCompletionLevel::Identity => false,
            SymbolCompletionLevel::DeclarationSurface => !matches!(
                self,
                Self::GenericConstraints
                    | Self::CallableContracts
                    | Self::ConstantDefinition
                    | Self::CallableParameterDefault
                    | Self::StructFieldDefault
                    | Self::UnionPayloadFieldDefault
                    | Self::PredicateDefinition
                    | Self::ImplementationCoherence
                    | Self::OverloadArms
            ),
        }
    }

    /// Returns whether this symbol category can own the fact.
    ///
    /// Presence-dependent facts such as runtime defaults are filtered against the exact symbol
    /// record while constructing a completion plan.
    pub const fn is_applicable_to(self, symbol: AnySymbolId) -> bool {
        let kind = symbol.kind();

        match self {
            Self::Members => supports_members(kind),
            Self::Imports => matches!(kind, SymbolKind::Module),
            Self::Directives => supports_directives(kind),
            Self::GenericParameters
            | Self::GenericDeclarationTemplate
            | Self::GenericConstraints => GenericOwnerId::try_new(symbol).is_some(),
            Self::CallableSignature | Self::CallableContracts | Self::CallableContractTemplate => {
                supports_callable_facts(kind)
            }
            Self::PredicateSignatureTemplate => supports_predicate_facts(kind),
            Self::CallableContractType => matches!(kind, SymbolKind::CallableContract),
            Self::ConstantDeclaredType => supports_declared_constant_type(kind),
            Self::ConstantDefinition => supports_constant_facts(kind),
            Self::CallableParameterDefault => matches!(kind, SymbolKind::CallableParameter),
            Self::UnevaluatedDefaultTemplate => matches!(
                kind,
                SymbolKind::CallableParameter
                    | SymbolKind::StructField
                    | SymbolKind::UnionPayloadField
            ),
            Self::StructFieldType | Self::StructFieldDefault => {
                matches!(kind, SymbolKind::StructField)
            }
            Self::TypeMemberValue => matches!(
                kind,
                SymbolKind::InherentTypeMember | SymbolKind::TraitTypeFulfillment
            ),
            Self::UnionPayloadFieldType | Self::UnionPayloadFieldDefault => {
                matches!(kind, SymbolKind::UnionPayloadField)
            }
            Self::PredicateDefinition => supports_predicate_facts(kind),
            Self::UnionVariantPayload => matches!(kind, SymbolKind::UnionVariant),
            Self::ImplementationSubject
            | Self::ImplementedTraitApplication
            | Self::ImplementationHeadTemplate
            | Self::ImplementationCoherence => supports_implementation_facts(kind),
            Self::OverloadArms | Self::OverloadSignatureTemplate => matches!(
                kind,
                SymbolKind::CallableOverload | SymbolKind::ImplementationOverload
            ),
        }
    }
}

const fn supports_directives(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Module
            | SymbolKind::Function
            | SymbolKind::Struct
            | SymbolKind::Union
            | SymbolKind::UnionVariant
    )
}

const fn supports_members(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::CompilerKnownEnvironment
            | SymbolKind::Module
            | SymbolKind::Struct
            | SymbolKind::Union
            | SymbolKind::Trait
            | SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::NamedTraitImplementation
    )
}

const fn supports_callable_facts(kind: SymbolKind) -> bool {
    kind.is_callable()
}

const fn supports_constant_facts(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Constant
            | SymbolKind::TraitConstantMember
            | SymbolKind::TraitConstantFulfillment
    )
}

const fn supports_declared_constant_type(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::GenericConstParameter) || supports_constant_facts(kind)
}

const fn supports_predicate_facts(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Predicate
            | SymbolKind::TraitPredicateMember
            | SymbolKind::TraitPredicateFulfillment
    )
}

const fn supports_implementation_facts(kind: SymbolKind) -> bool {
    kind.is_implementation()
}

#[cfg(test)]
mod tests {
    use super::{SymbolCompletionLevel, SymbolFactKind};
    use crate::{
        AnySymbolId, CallableContractSymbolId, CallableParameterSymbolId,
        CompilerKnownEnvironmentSymbolId, ConstantSymbolId, FunctionSymbolId, PackageSymbolId,
        SymbolId, TraitTypeFulfillmentSymbolId,
    };

    #[test]
    fn completion_levels_select_fact_work() {
        assert!(
            SymbolFactKind::CallableSignature
                .is_required_for(SymbolCompletionLevel::DeclarationSurface)
        );

        assert!(
            !SymbolFactKind::CallableSignature.is_required_for(SymbolCompletionLevel::Identity)
        );
    }

    #[test]
    fn fact_applicability_uses_exact_symbol_categories() {
        let function = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));
        let constant = AnySymbolId::from(ConstantSymbolId::from_symbol_id(SymbolId::new(2)));

        let parameter =
            AnySymbolId::from(CallableParameterSymbolId::from_symbol_id(SymbolId::new(3)));

        let compiler_known = AnySymbolId::from(CompilerKnownEnvironmentSymbolId::from_symbol_id(
            SymbolId::new(4),
        ));

        let package = AnySymbolId::from(PackageSymbolId::from_symbol_id(SymbolId::new(5)));

        let contract =
            AnySymbolId::from(CallableContractSymbolId::from_symbol_id(SymbolId::new(6)));

        let type_fulfillment = AnySymbolId::from(TraitTypeFulfillmentSymbolId::from_symbol_id(
            SymbolId::new(7),
        ));

        assert!(SymbolFactKind::Directives.is_applicable_to(function));
        assert!(!SymbolFactKind::Directives.is_applicable_to(parameter));
        assert!(SymbolFactKind::CallableSignature.is_applicable_to(function));
        assert!(!SymbolFactKind::CallableSignature.is_applicable_to(constant));
        assert!(SymbolFactKind::ConstantDefinition.is_applicable_to(constant));
        assert!(SymbolFactKind::GenericConstraints.is_applicable_to(function));
        assert!(SymbolFactKind::Members.is_applicable_to(compiler_known));
        assert!(!SymbolFactKind::Members.is_applicable_to(package));
        assert!(SymbolFactKind::CallableContractType.is_applicable_to(contract));
        assert!(SymbolFactKind::TypeMemberValue.is_applicable_to(type_fulfillment));
    }

    #[test]
    fn policy_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolCompletionLevel>();
        assert_send_sync::<SymbolFactKind>();
    }
}
