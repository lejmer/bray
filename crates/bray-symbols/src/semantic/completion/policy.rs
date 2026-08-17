use crate::{AnySymbolId, GenericOwnerId, SymbolKind};

/// A semantic completion boundary for one declaration symbol.
///
/// Executable bodies use separate checked-unit analyses and are deliberately absent from this
/// symbol completion family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolCompletionLevel {
    /// Only the deterministic symbol identity skeleton is required.
    Identity,
    /// Every applicable query needed to describe the declaration surface is required.
    DeclarationSurface,
}

/// An exact category of semantic query owned by a declaration symbol.
///
/// Instance-specific queries with additional semantic inputs use their own typed keys rather than
/// discarding those inputs into this symbol-only category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolQueryKind {
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
    /// The complete checked open template of a static declaration.
    StaticInstanceTemplate,
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

pub(super) const SYMBOL_QUERY_KINDS: [SymbolQueryKind; 29] = [
    SymbolQueryKind::Members,
    SymbolQueryKind::Imports,
    SymbolQueryKind::Directives,
    SymbolQueryKind::GenericParameters,
    SymbolQueryKind::GenericDeclarationTemplate,
    SymbolQueryKind::GenericConstraints,
    SymbolQueryKind::CallableSignature,
    SymbolQueryKind::CallableContracts,
    SymbolQueryKind::CallableContractTemplate,
    SymbolQueryKind::PredicateSignatureTemplate,
    SymbolQueryKind::CallableContractType,
    SymbolQueryKind::ConstantDeclaredType,
    SymbolQueryKind::ConstantDefinition,
    SymbolQueryKind::StaticInstanceTemplate,
    SymbolQueryKind::CallableParameterDefault,
    SymbolQueryKind::UnevaluatedDefaultTemplate,
    SymbolQueryKind::StructFieldType,
    SymbolQueryKind::TypeMemberValue,
    SymbolQueryKind::StructFieldDefault,
    SymbolQueryKind::UnionPayloadFieldType,
    SymbolQueryKind::UnionPayloadFieldDefault,
    SymbolQueryKind::PredicateDefinition,
    SymbolQueryKind::UnionVariantPayload,
    SymbolQueryKind::ImplementationSubject,
    SymbolQueryKind::ImplementedTraitApplication,
    SymbolQueryKind::ImplementationHeadTemplate,
    SymbolQueryKind::ImplementationCoherence,
    SymbolQueryKind::OverloadArms,
    SymbolQueryKind::OverloadSignatureTemplate,
];

impl SymbolQueryKind {
    /// Returns whether this query participates in the requested completion boundary when applicable.
    pub const fn is_required_for(self, level: SymbolCompletionLevel) -> bool {
        match level {
            SymbolCompletionLevel::Identity => false,
            SymbolCompletionLevel::DeclarationSurface => !matches!(
                self,
                Self::GenericConstraints
                    | Self::CallableContracts
                    | Self::ConstantDefinition
                    | Self::StaticInstanceTemplate
                    | Self::CallableParameterDefault
                    | Self::StructFieldDefault
                    | Self::UnionPayloadFieldDefault
                    | Self::PredicateDefinition
                    | Self::ImplementationCoherence
                    | Self::OverloadArms
            ),
        }
    }

    /// Returns whether this symbol category can own the query.
    ///
    /// Presence-dependent queries such as runtime defaults are filtered against the exact symbol
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
                supports_callable_queries(kind)
            }
            Self::PredicateSignatureTemplate => supports_predicate_queries(kind),
            Self::CallableContractType => matches!(kind, SymbolKind::CallableContract),
            Self::ConstantDeclaredType => supports_declared_constant_type(kind),
            Self::ConstantDefinition => supports_constant_queries(kind),
            Self::StaticInstanceTemplate => matches!(kind, SymbolKind::Static),
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
            Self::PredicateDefinition => supports_predicate_queries(kind),
            Self::UnionVariantPayload => matches!(kind, SymbolKind::UnionVariant),
            Self::ImplementationSubject
            | Self::ImplementedTraitApplication
            | Self::ImplementationHeadTemplate
            | Self::ImplementationCoherence => supports_implementation_semantics(kind),
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
            | SymbolKind::Static
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

const fn supports_callable_queries(kind: SymbolKind) -> bool {
    kind.is_callable()
}

const fn supports_constant_queries(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Constant
            | SymbolKind::TraitConstantMember
            | SymbolKind::TraitConstantFulfillment
    )
}

const fn supports_declared_constant_type(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::GenericConstParameter | SymbolKind::Static)
        || supports_constant_queries(kind)
}

const fn supports_predicate_queries(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Predicate
            | SymbolKind::TraitPredicateMember
            | SymbolKind::TraitPredicateFulfillment
    )
}

const fn supports_implementation_semantics(kind: SymbolKind) -> bool {
    kind.is_implementation()
}

#[cfg(test)]
mod tests {
    use super::{SymbolCompletionLevel, SymbolQueryKind};
    use crate::{
        AnySymbolId, CallableContractSymbolId, CallableParameterSymbolId,
        CompilerKnownEnvironmentSymbolId, ConstantSymbolId, FunctionSymbolId, PackageSymbolId,
        SymbolId, TraitTypeFulfillmentSymbolId,
    };

    #[test]
    fn completion_levels_select_query_work() {
        assert!(
            SymbolQueryKind::CallableSignature
                .is_required_for(SymbolCompletionLevel::DeclarationSurface)
        );

        assert!(
            !SymbolQueryKind::CallableSignature.is_required_for(SymbolCompletionLevel::Identity)
        );
    }

    #[test]
    fn query_applicability_uses_exact_symbol_categories() {
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

        assert!(SymbolQueryKind::Directives.is_applicable_to(function));
        assert!(!SymbolQueryKind::Directives.is_applicable_to(parameter));
        assert!(SymbolQueryKind::CallableSignature.is_applicable_to(function));
        assert!(!SymbolQueryKind::CallableSignature.is_applicable_to(constant));
        assert!(SymbolQueryKind::ConstantDefinition.is_applicable_to(constant));
        assert!(SymbolQueryKind::GenericConstraints.is_applicable_to(function));
        assert!(SymbolQueryKind::Members.is_applicable_to(compiler_known));
        assert!(!SymbolQueryKind::Members.is_applicable_to(package));
        assert!(SymbolQueryKind::CallableContractType.is_applicable_to(contract));
        assert!(SymbolQueryKind::TypeMemberValue.is_applicable_to(type_fulfillment));
    }

    #[test]
    fn policy_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolCompletionLevel>();
        assert_send_sync::<SymbolQueryKind>();
    }
}
