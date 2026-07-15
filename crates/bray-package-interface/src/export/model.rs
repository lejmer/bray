use std::sync::Arc;

use bray_symbols::{ExternalSymbolKey, SymbolName};

use crate::{
    DependencyInterfaceId, ExportedLookupKind, InterfaceArtifactHash, InterfaceContentHash,
    InterfaceLanguageRevision, InterfaceSemanticFacts, InterfaceSymbolReference,
    InterfaceValidationError, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    SymbolRelationshipKind,
};

/// One symbol selected for a library product's public identity surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportSymbolInput {
    key: ExternalSymbolKey,
    containing_symbol: Option<ExternalSymbolKey>,
}

impl ExportSymbolInput {
    /// Creates one selected symbol using stable identities rather than table positions.
    pub const fn new(key: ExternalSymbolKey, containing_symbol: Option<ExternalSymbolKey>) -> Self {
        Self {
            key,
            containing_symbol,
        }
    }

    /// Returns the selected symbol's stable external identity.
    pub const fn key(&self) -> &ExternalSymbolKey {
        &self.key
    }

    /// Returns the stable identity of the selected containing symbol.
    pub const fn containing_symbol(&self) -> Option<&ExternalSymbolKey> {
        self.containing_symbol.as_ref()
    }
}

/// One selected typed relationship expressed without artifact-local IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportRelationshipInput {
    kind: SymbolRelationshipKind,
    owner: ExternalSymbolKey,
    member: ExternalSymbolKey,
    ordinal: u32,
}

impl ExportRelationshipInput {
    /// Creates one owner-relative relationship between selected stable identities.
    pub const fn new(
        kind: SymbolRelationshipKind,
        owner: ExternalSymbolKey,
        member: ExternalSymbolKey,
        ordinal: u32,
    ) -> Self {
        Self {
            kind,
            owner,
            member,
            ordinal,
        }
    }

    pub(super) const fn kind(&self) -> SymbolRelationshipKind {
        self.kind
    }

    pub(super) const fn owner(&self) -> &ExternalSymbolKey {
        &self.owner
    }

    pub(super) const fn member(&self) -> &ExternalSymbolKey {
        &self.member
    }

    pub(super) const fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

/// Stable target identity projected through one exported lookup edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportSymbolReferenceInput {
    /// A selected symbol defined by the current package.
    Local(ExternalSymbolKey),
    /// A symbol retaining its identity from one dependency input.
    Dependency {
        /// Position in the caller-supplied dependency sequence.
        dependency: DependencyInterfaceId,
        /// Stable identity in the selected dependency interface.
        key: ExternalSymbolKey,
    },
}

/// One selected exported ordinary name expressed without artifact-local symbol IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportLookupInput {
    owner: ExternalSymbolKey,
    name: SymbolName,
    kind: ExportedLookupKind,
    target: ExportSymbolReferenceInput,
}

impl ExportLookupInput {
    /// Creates one direct export or re-export from stable symbol identities.
    pub const fn new(
        owner: ExternalSymbolKey,
        name: SymbolName,
        kind: ExportedLookupKind,
        target: ExportSymbolReferenceInput,
    ) -> Self {
        Self {
            owner,
            name,
            kind,
            target,
        }
    }

    pub(super) const fn owner(&self) -> &ExternalSymbolKey {
        &self.owner
    }

    pub(super) const fn name(&self) -> &SymbolName {
        &self.name
    }

    pub(super) const fn kind(&self) -> ExportedLookupKind {
        self.kind
    }

    pub(super) const fn target(&self) -> &ExportSymbolReferenceInput {
        &self.target
    }
}

/// Failure while assigning canonical package-interface identity-table positions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageInterfaceExportSurfaceError {
    /// The selected symbol sequence contains the same stable identity more than once.
    DuplicateSymbol(ExternalSymbolKey),
    /// A container, relationship, or local export references an unselected symbol.
    MissingSymbol(ExternalSymbolKey),
    /// The selected public graph cannot use compact artifact-local symbol IDs.
    SymbolCountOverflow,
    /// Canonical surface validation rejected the selected graph.
    Surface(PackageInterfaceSurfaceBuildError),
}

/// Failure while validating a package-interface export bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageInterfaceExportBuildError {
    /// One exported declaration has no completed semantic fact in the bundle.
    MissingSemanticFacts(ExternalSymbolKey),
    /// Structural semantic or support-graph validation rejected the bundle.
    Validation(InterfaceValidationError),
}

/// One validated immutable library surface ready for deterministic interface encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInterfaceExportBundle {
    surface: PackageInterfaceSurface,
    semantic_facts: InterfaceSemanticFacts,
    language_revision: InterfaceLanguageRevision,
}

impl PackageInterfaceExportBundle {
    /// Creates an export bundle after validating structural and semantic completeness.
    pub fn try_new(
        surface: PackageInterfaceSurface,
        semantic_facts: InterfaceSemanticFacts,
        language_revision: InterfaceLanguageRevision,
    ) -> Result<Self, PackageInterfaceExportBuildError> {
        let semantic_facts = canonicalize_owner_addressed_facts(semantic_facts);

        validate_semantic_coverage(&surface, &semantic_facts)?;

        semantic_facts
            .validate(&surface, crate::InterfaceValidationLimits::default())
            .map_err(PackageInterfaceExportBuildError::Validation)?;

        Ok(Self {
            surface,
            semantic_facts,
            language_revision,
        })
    }

    /// Returns the canonical exported package and symbol surface.
    pub const fn surface(&self) -> &PackageInterfaceSurface {
        &self.surface
    }

    /// Returns the complete semantic and private support graph.
    pub const fn semantic_facts(&self) -> &InterfaceSemanticFacts {
        &self.semantic_facts
    }

    /// Returns the language semantic revision used to interpret the surface.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.language_revision
    }
}

/// Complete deterministic bytes and identities of one encoded package interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedPackageInterface {
    bytes: Arc<[u8]>,
    content_hash: InterfaceContentHash,
    artifact_hash: InterfaceArtifactHash,
}

impl EncodedPackageInterface {
    pub(crate) fn new(
        bytes: Vec<u8>,
        content_hash: InterfaceContentHash,
        artifact_hash: InterfaceArtifactHash,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            content_hash,
            artifact_hash,
        }
    }

    /// Returns the complete canonical artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the semantic content identity of the encoded interface.
    pub const fn content_hash(&self) -> InterfaceContentHash {
        self.content_hash
    }

    /// Returns the identity of the exact encoded artifact bytes.
    pub const fn artifact_hash(&self) -> InterfaceArtifactHash {
        self.artifact_hash
    }
}

impl AsRef<[u8]> for EncodedPackageInterface {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

fn validate_semantic_coverage(
    surface: &PackageInterfaceSurface,
    semantic_facts: &InterfaceSemanticFacts,
) -> Result<(), PackageInterfaceExportBuildError> {
    let fact_directory = semantic_facts.fact_directory();

    for symbol in surface.symbols().symbols() {
        if !requires_owned_semantic_fact(symbol.kind()) {
            continue;
        }

        if !fact_directory.iter().any(|fact| {
            matches!(fact.owner(), InterfaceSymbolReference::Local(owner) if *owner == symbol.id())
        }) {
            // External keys are Arc-backed and make the failure independent of local table IDs.
            return Err(PackageInterfaceExportBuildError::MissingSemanticFacts(
                symbol.key().clone(),
            ));
        }
    }

    Ok(())
}

fn canonicalize_owner_addressed_facts(mut facts: InterfaceSemanticFacts) -> InterfaceSemanticFacts {
    Arc::make_mut(&mut facts.constraints).sort();
    Arc::make_mut(&mut facts.callable_contracts).sort();
    Arc::make_mut(&mut facts.declaration_templates).sort();
    Arc::make_mut(&mut facts.implementations).sort();
    Arc::make_mut(&mut facts.coherence).sort();
    Arc::make_mut(&mut facts.target_dependencies).sort();
    Arc::make_mut(&mut facts.abi_dependencies).sort();
    Arc::make_mut(&mut facts.provenance).sort();

    facts
}

const fn requires_owned_semantic_fact(kind: bray_symbols::SymbolKind) -> bool {
    !matches!(
        kind,
        bray_symbols::SymbolKind::Package
            | bray_symbols::SymbolKind::Module
            | bray_symbols::SymbolKind::GenericTypeParameter
            | bray_symbols::SymbolKind::GenericConstParameter
            | bray_symbols::SymbolKind::CallableParameter
            | bray_symbols::SymbolKind::PredicateParameter
            | bray_symbols::SymbolKind::ReceiverParameter
            | bray_symbols::SymbolKind::CallableParameterDefaultProvider
            | bray_symbols::SymbolKind::StructFieldDefaultProvider
            | bray_symbols::SymbolKind::UnionPayloadDefaultProvider
    )
}

#[cfg(test)]
mod tests {
    use bray_symbols::{CallableAbi, ExternalSymbolKey, SymbolKind};

    use crate::test_support::package_interface_export_bundle;
    use crate::{
        InterfaceAbiDependency, InterfaceLanguageRevision, InterfaceSemanticFacts,
        InterfaceSymbolReference, InterfaceValidationError, PackageInterfaceExportBuildError,
        PackageInterfaceExportBundle, encode_package_interface,
    };

    #[test]
    fn bundles_reject_incomplete_support_graphs_before_encoding() {
        let complete = package_interface_export_bundle();

        let facts = complete.semantic_facts().clone().with_templates(
            complete.semantic_facts().checked_templates.iter().cloned(),
            complete
                .semantic_facts()
                .declaration_templates
                .iter()
                .cloned(),
            [],
        );

        assert_eq!(
            PackageInterfaceExportBundle::try_new(
                complete.surface().clone(),
                facts,
                InterfaceLanguageRevision::new(0),
            ),
            Err(PackageInterfaceExportBuildError::Validation(
                InterfaceValidationError::Malformed
            ))
        );
    }

    #[test]
    fn bundles_reject_declaration_identities_without_semantic_facts() {
        let complete = package_interface_export_bundle();
        let function = complete
            .surface()
            .symbols()
            .symbols()
            .iter()
            .find(|symbol| symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.key().clone())
            .unwrap_or_else(|| panic!("test surface must contain one function"));

        assert_eq!(
            PackageInterfaceExportBundle::try_new(
                complete.surface().clone(),
                InterfaceSemanticFacts::new(),
                InterfaceLanguageRevision::new(0),
            ),
            Err(PackageInterfaceExportBuildError::MissingSemanticFacts(
                function
            ))
        );
    }

    #[test]
    fn owner_addressed_semantic_fact_order_does_not_affect_encoded_artifacts() {
        let complete = package_interface_export_bundle();
        let symbols = complete.surface().symbols().symbols();

        let [first_owner, second_owner] = symbols
            .iter()
            .filter(|symbol| {
                matches!(
                    symbol.kind(),
                    SymbolKind::Function | SymbolKind::GenericTypeParameter
                )
            })
            .map(|symbol| InterfaceSymbolReference::Local(symbol.id()))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_else(|_| panic!("test surface must contain two semantic fact owners"));

        let first_facts = complete.semantic_facts().clone().with_target_dependencies(
            [],
            [
                InterfaceAbiDependency::new(first_owner.clone(), CallableAbi::Bray),
                InterfaceAbiDependency::new(second_owner.clone(), CallableAbi::C),
            ],
        );

        let second_facts = complete.semantic_facts().clone().with_target_dependencies(
            [],
            [
                InterfaceAbiDependency::new(second_owner, CallableAbi::C),
                InterfaceAbiDependency::new(first_owner, CallableAbi::Bray),
            ],
        );

        let first = PackageInterfaceExportBundle::try_new(
            complete.surface().clone(),
            first_facts,
            InterfaceLanguageRevision::new(0),
        )
        .unwrap_or_else(|error| panic!("forward semantic facts must build: {error:?}"));

        let second = PackageInterfaceExportBundle::try_new(
            complete.surface().clone(),
            second_facts,
            InterfaceLanguageRevision::new(0),
        )
        .unwrap_or_else(|error| panic!("reversed semantic facts must build: {error:?}"));

        assert_eq!(first, second);

        let first = encode_package_interface(&first)
            .unwrap_or_else(|error| panic!("forward semantic facts must encode: {error:?}"));

        let second = encode_package_interface(&second)
            .unwrap_or_else(|error| panic!("reversed semantic facts must encode: {error:?}"));

        assert_eq!(first, second);
    }

    #[test]
    fn export_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<PackageInterfaceExportBundle>();
        assert_send_sync::<super::EncodedPackageInterface>();
        assert_send_sync::<ExternalSymbolKey>();
    }
}
