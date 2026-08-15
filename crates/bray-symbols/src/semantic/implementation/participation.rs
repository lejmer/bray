use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;

use crate::{ImplementationSymbolId, PackageIdentity, SymbolKey};

/// Stable identity of one package coherence domain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCoherenceDomainKey(PackageIdentity);

impl ImplementationCoherenceDomainKey {
    /// Creates the coherence-domain key for one package.
    pub const fn new(package: PackageIdentity) -> Self {
        Self(package)
    }

    /// Returns the package that owns this coherence domain.
    pub const fn package(&self) -> &PackageIdentity {
        &self.0
    }
}

/// Stable evidence that makes one implementation participate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationParticipationEvidence(ImplementationParticipationEvidenceData);

/// How one implementation became visible in a coherence domain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationParticipationKind {
    /// The implementation is declared by the coherence domain.
    Declared,
    /// The implementation belongs to the ambient compiler-known environment.
    CompilerKnown,
    /// The implementation is named by one or more explicit `using` declarations.
    ExplicitUsing,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ImplementationParticipationEvidenceData {
    Declared,
    CompilerKnown,
    ExplicitUsing(Arc<[SyntaxAnchor]>),
}

impl ImplementationParticipationEvidence {
    /// Creates evidence for an implementation declared by the coherence domain.
    pub const fn declared() -> Self {
        Self(ImplementationParticipationEvidenceData::Declared)
    }

    /// Creates evidence for an ambient compiler-known implementation.
    pub const fn compiler_known() -> Self {
        Self(ImplementationParticipationEvidenceData::CompilerKnown)
    }

    /// Creates evidence from one or more explicit `using` declarations.
    pub fn explicit_using(
        using_declarations: impl IntoIterator<Item = SyntaxAnchor>,
    ) -> Option<Self> {
        let using_declarations = shared_slice(using_declarations);

        if using_declarations.is_empty()
            || !using_declarations.windows(2).all(|pair| pair[0] < pair[1])
        {
            return None;
        }

        Some(Self(
            ImplementationParticipationEvidenceData::ExplicitUsing(using_declarations),
        ))
    }

    /// Returns how this implementation became visible.
    pub const fn kind(&self) -> ImplementationParticipationKind {
        match &self.0 {
            ImplementationParticipationEvidenceData::Declared => {
                ImplementationParticipationKind::Declared
            }
            ImplementationParticipationEvidenceData::CompilerKnown => {
                ImplementationParticipationKind::CompilerKnown
            }
            ImplementationParticipationEvidenceData::ExplicitUsing(_) => {
                ImplementationParticipationKind::ExplicitUsing
            }
        }
    }

    /// Returns the explicit `using` declarations that made the implementation visible.
    pub fn using_declarations(&self) -> &[SyntaxAnchor] {
        match &self.0 {
            ImplementationParticipationEvidenceData::ExplicitUsing(declarations) => declarations,
            ImplementationParticipationEvidenceData::Declared
            | ImplementationParticipationEvidenceData::CompilerKnown => &[],
        }
    }
}

/// One implementation participating in a coherence domain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ParticipatingImplementation {
    key: SymbolKey,
    implementation: ImplementationSymbolId,
    evidence: ImplementationParticipationEvidence,
}

impl ParticipatingImplementation {
    /// Creates one participant when the identities have the same trait-implementation kind.
    ///
    /// The caller must ensure both identities refer to the same implementation.
    pub fn try_new(
        key: SymbolKey,
        implementation: ImplementationSymbolId,
        evidence: ImplementationParticipationEvidence,
    ) -> Option<Self> {
        if matches!(implementation, ImplementationSymbolId::Inherent(_))
            || key.kind() != implementation.into_any().kind()
        {
            return None;
        }

        Some(Self {
            key,
            implementation,
            evidence,
        })
    }

    /// Returns the implementation's stable semantic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the exact compilation-local implementation identity.
    pub const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    /// Returns the evidence that makes this implementation participate.
    pub const fn evidence(&self) -> &ImplementationParticipationEvidence {
        &self.evidence
    }
}

/// Reports malformed implementation participation input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplementationParticipationSetError {
    /// Two records use the same stable implementation key.
    DuplicateKey,
    /// Two records use the same compilation-local implementation identity.
    DuplicateImplementation,
}

/// The canonical implementations participating in one coherence domain.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImplementationParticipationSet {
    domain: ImplementationCoherenceDomainKey,
    implementations: Arc<[ParticipatingImplementation]>,
}

impl ImplementationParticipationSet {
    /// Creates a canonical participation set with unique stable and compilation-local identities.
    pub fn try_new(
        domain: ImplementationCoherenceDomainKey,
        implementations: impl IntoIterator<Item = ParticipatingImplementation>,
    ) -> Result<Self, ImplementationParticipationSetError> {
        let mut implementations = implementations.into_iter().collect::<Vec<_>>();

        implementations.sort_by(|left, right| left.key.cmp(&right.key));

        if implementations
            .windows(2)
            .any(|pair| pair[0].key == pair[1].key)
        {
            return Err(ImplementationParticipationSetError::DuplicateKey);
        }

        let mut identities = BTreeSet::new();

        if implementations
            .iter()
            .any(|participant| !identities.insert(participant.implementation))
        {
            return Err(ImplementationParticipationSetError::DuplicateImplementation);
        }

        Ok(Self {
            domain,
            implementations: Arc::from(implementations),
        })
    }

    /// Returns the coherence domain represented by this set.
    pub const fn domain(&self) -> &ImplementationCoherenceDomainKey {
        &self.domain
    }

    /// Returns participating implementations in stable semantic-key order.
    pub fn implementations(&self) -> &[ParticipatingImplementation] {
        &self.implementations
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::{DeclarationId, SyntaxAnchor};
    use bray_testing::{test_source_at, test_source_store};

    use super::{
        ImplementationCoherenceDomainKey, ImplementationParticipationEvidence,
        ImplementationParticipationKind, ImplementationParticipationSet,
        ImplementationParticipationSetError, ParticipatingImplementation,
    };
    use crate::{
        ImplementationSymbolId, ModulePathKey, NamedTraitImplementationSymbolId, PackageIdentity,
        SymbolId, SymbolKey, SymbolKind, SymbolRootKey, UnnamedTraitImplementationSymbolId,
    };

    #[test]
    fn participation_sets_canonicalize_origin_neutral_implementation_records() {
        let domain = domain();
        let named = named_participant(2, ImplementationParticipationEvidence::compiler_known());
        let unnamed = unnamed_participant(1, ImplementationParticipationEvidence::declared());

        let set = ImplementationParticipationSet::try_new(domain.clone(), [named, unnamed])
            .unwrap_or_else(|error| panic!("participation set must be valid: {error:?}"));

        assert_eq!(set.domain(), &domain);

        assert_eq!(
            set.implementations()
                .iter()
                .map(ParticipatingImplementation::implementation)
                .collect::<Vec<_>>(),
            [
                ImplementationSymbolId::UnnamedTrait(
                    UnnamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(1))
                ),
                ImplementationSymbolId::NamedTrait(
                    NamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(2))
                )
            ]
        );

        assert_eq!(
            set.implementations()[0].evidence().kind(),
            ImplementationParticipationKind::Declared
        );

        assert_eq!(
            set.implementations()[1].evidence().kind(),
            ImplementationParticipationKind::CompilerKnown
        );
    }

    #[test]
    fn participation_sets_reject_duplicate_keys_and_identities() {
        let first = named_participant(1, ImplementationParticipationEvidence::declared());

        let duplicate_key = ParticipatingImplementation::try_new(
            first.key().clone(),
            ImplementationSymbolId::NamedTrait(NamedTraitImplementationSymbolId::from_symbol_id(
                SymbolId::new(2),
            )),
            ImplementationParticipationEvidence::declared(),
        )
        .unwrap_or_else(|| panic!("duplicate-key participant must be structurally valid"));

        assert_eq!(
            ImplementationParticipationSet::try_new(domain(), [first.clone(), duplicate_key]),
            Err(ImplementationParticipationSetError::DuplicateKey)
        );

        let duplicate_identity = ParticipatingImplementation::try_new(
            implementation_key(SymbolKind::NamedTraitImplementation, 3),
            first.implementation(),
            ImplementationParticipationEvidence::compiler_known(),
        )
        .unwrap_or_else(|| panic!("duplicate-identity participant must be structurally valid"));

        assert_eq!(
            ImplementationParticipationSet::try_new(domain(), [first, duplicate_identity]),
            Err(ImplementationParticipationSetError::DuplicateImplementation)
        );
    }

    #[test]
    fn explicit_using_evidence_retains_ordered_source_anchors() {
        let sources = test_source_store([concat!(
            "module app;\n",
            "using dependency.First;\n",
            "using dependency.Second;\n",
        )]);

        let parsed = bray_parser::parse_source_unit(test_source_at(&sources, 0));

        let anchors = parsed
            .source_unit()
            .using_declarations()
            .map(|declaration| SyntaxAnchor::from_node(&declaration))
            .collect::<Vec<_>>();

        let evidence = ImplementationParticipationEvidence::explicit_using(anchors.clone())
            .unwrap_or_else(|| panic!("ordered using declarations must produce evidence"));

        assert_eq!(
            evidence.kind(),
            ImplementationParticipationKind::ExplicitUsing
        );

        assert_eq!(evidence.using_declarations(), anchors);
    }

    fn named_participant(
        declaration: u32,
        evidence: ImplementationParticipationEvidence,
    ) -> ParticipatingImplementation {
        let implementation =
            NamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(declaration));

        ParticipatingImplementation::try_new(
            implementation_key(SymbolKind::NamedTraitImplementation, declaration),
            ImplementationSymbolId::NamedTrait(implementation),
            evidence,
        )
        .unwrap_or_else(|| panic!("named participant must be valid"))
    }

    fn unnamed_participant(
        declaration: u32,
        evidence: ImplementationParticipationEvidence,
    ) -> ParticipatingImplementation {
        let implementation =
            UnnamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(declaration));

        ParticipatingImplementation::try_new(
            implementation_key(SymbolKind::UnnamedTraitImplementation, declaration),
            ImplementationSymbolId::UnnamedTrait(implementation),
            evidence,
        )
        .unwrap_or_else(|| panic!("unnamed participant must be valid"))
    }

    fn implementation_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
        SymbolKey::source_declaration(module_key(), kind, DeclarationId::new(declaration))
            .unwrap_or_else(|| panic!("implementation key must be valid"))
    }

    fn module_key() -> SymbolKey {
        let path =
            ModulePathKey::try_new(["app"]).unwrap_or_else(|| panic!("module path must be valid"));

        SymbolKey::module(SymbolRootKey::Package(package()), path)
    }

    fn domain() -> ImplementationCoherenceDomainKey {
        ImplementationCoherenceDomainKey::new(package())
    }

    fn package() -> PackageIdentity {
        PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("package identity must be valid"))
    }
}
