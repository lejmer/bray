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

impl SymbolFactKind {
    /// Returns whether this fact participates in the requested completion boundary when applicable.
    pub const fn is_required_for(self, level: SymbolCompletionLevel) -> bool {
        match level {
            SymbolCompletionLevel::Identity => false,
            SymbolCompletionLevel::DeclarationSurface => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SymbolCompletionLevel, SymbolFactKind};

    #[test]
    fn declaration_surface_completion_includes_symbol_facts() {
        assert!(
            SymbolFactKind::CallableSignature
                .is_required_for(SymbolCompletionLevel::DeclarationSurface)
        );
        assert!(
            !SymbolFactKind::CallableSignature.is_required_for(SymbolCompletionLevel::Identity)
        );
    }

    #[test]
    fn fact_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolCompletionLevel>();
        assert_send_sync::<SymbolFactKind>();
    }
}
