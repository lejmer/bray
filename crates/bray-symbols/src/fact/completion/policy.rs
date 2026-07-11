use crate::{AnySymbolId, GenericOwnerId, SymbolKind};

/// A semantic completion boundary for one declaration symbol.
///
/// Executable bodies use separate checked-unit facts and are deliberately absent from this
/// symbol completion family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolCompletionLevel {
    /// Only the deterministic symbol identity skeleton is required.
    Identity,
    /// Every applicable fact needed to describe and use the declaration surface is required.
    DeclarationSurface,
}

/// An exact category of lazy semantic fact owned by a declaration symbol.
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
    /// Checked generic constraints.
    GenericConstraints,
    /// The callable signature excluding executable body checking.
    CallableSignature,
    /// Checked callable contracts, effects, and capabilities.
    CallableContracts,
    /// The declared type of a constant.
    ConstantDeclaredType,
    /// The checked definition template of a constant.
    ConstantDefinition,
    /// A checked callable parameter default.
    CallableParameterDefault,
    /// A checked struct field type.
    StructFieldType,
    /// A checked struct field default.
    StructFieldDefault,
    /// A checked union payload field type.
    UnionPayloadFieldType,
    /// A checked union payload field default.
    UnionPayloadFieldDefault,
    /// A checked predicate definition.
    PredicateDefinition,
    /// A union variant's payload surface.
    UnionVariantPayload,
    /// An implementation's checked subject.
    ImplementationSubject,
    /// The checked trait application implemented by an implementation.
    ImplementedTraitApplication,
    /// Stable implementation coherence keys.
    ImplementationCoherence,
    /// The resolved arms of an overload family.
    OverloadArms,
}

pub(super) const SYMBOL_FACT_KINDS: [SymbolFactKind; 20] = [
    SymbolFactKind::Members,
    SymbolFactKind::Imports,
    SymbolFactKind::Directives,
    SymbolFactKind::GenericParameters,
    SymbolFactKind::GenericConstraints,
    SymbolFactKind::CallableSignature,
    SymbolFactKind::CallableContracts,
    SymbolFactKind::ConstantDeclaredType,
    SymbolFactKind::ConstantDefinition,
    SymbolFactKind::CallableParameterDefault,
    SymbolFactKind::StructFieldType,
    SymbolFactKind::StructFieldDefault,
    SymbolFactKind::UnionPayloadFieldType,
    SymbolFactKind::UnionPayloadFieldDefault,
    SymbolFactKind::PredicateDefinition,
    SymbolFactKind::UnionVariantPayload,
    SymbolFactKind::ImplementationSubject,
    SymbolFactKind::ImplementedTraitApplication,
    SymbolFactKind::ImplementationCoherence,
    SymbolFactKind::OverloadArms,
];

impl SymbolFactKind {
    /// Returns whether this fact participates in the requested completion boundary when applicable.
    pub const fn is_required_for(self, level: SymbolCompletionLevel) -> bool {
        match level {
            SymbolCompletionLevel::Identity => false,
            SymbolCompletionLevel::DeclarationSurface => true,
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
            Self::GenericParameters | Self::GenericConstraints => {
                GenericOwnerId::try_new(symbol).is_some()
            }
            Self::CallableSignature | Self::CallableContracts => supports_callable_facts(kind),
            Self::ConstantDeclaredType | Self::ConstantDefinition => supports_constant_facts(kind),
            Self::CallableParameterDefault => matches!(kind, SymbolKind::CallableParameter),
            Self::StructFieldType | Self::StructFieldDefault => {
                matches!(kind, SymbolKind::StructField)
            }
            Self::UnionPayloadFieldType | Self::UnionPayloadFieldDefault => {
                matches!(kind, SymbolKind::UnionPayloadField)
            }
            Self::PredicateDefinition => supports_predicate_facts(kind),
            Self::UnionVariantPayload => matches!(kind, SymbolKind::UnionVariant),
            Self::ImplementationSubject
            | Self::ImplementedTraitApplication
            | Self::ImplementationCoherence => supports_implementation_facts(kind),
            Self::OverloadArms => matches!(
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
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::TypeCallableMember
            | SymbolKind::TraitCallableMember
            | SymbolKind::TraitCallableFulfillment
            | SymbolKind::Constructor
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::TraitFinalizerRequirement
            | SymbolKind::TraitDestructorRequirement
            | SymbolKind::TraitScopeEnterRequirement
            | SymbolKind::TraitScopeExitRequirement
            | SymbolKind::TraitScopeEnterFulfillment
            | SymbolKind::TraitScopeExitFulfillment
    )
}

const fn supports_constant_facts(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Constant
            | SymbolKind::TraitConstantMember
            | SymbolKind::TraitConstantFulfillment
    )
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
    matches!(
        kind,
        SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::NamedTraitImplementation
    )
}
