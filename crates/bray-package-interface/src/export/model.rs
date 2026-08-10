use std::sync::Arc;

use bray_symbols::{
    CallablePosition, ExternalSymbolKey, InterfaceSymbolId, SymbolKind, SymbolName,
};

use crate::{
    DependencyInterfaceId, ExportedLookupKind, InterfaceLanguageRevision,
    InterfaceSemanticFactEntry, InterfaceSemanticFactKind, InterfaceSemanticFacts,
    InterfaceSymbolReference, InterfaceValidationError, PackageInterfaceSurface,
    PackageInterfaceSurfaceBuildError, SymbolRelationshipKind,
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
    position: CallablePosition,
    allows_mutation: bool,
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
            position: CallablePosition::NamedOnly,
            allows_mutation: false,
        }
    }

    /// Marks the related field as permitting mutation after initialization.
    pub const fn with_mutation(mut self) -> Self {
        self.allows_mutation = true;

        self
    }

    /// Sets the related payload field's call-position permission.
    pub const fn with_position(mut self, position: CallablePosition) -> Self {
        self.position = position;

        self
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

    pub(super) const fn position(&self) -> CallablePosition {
        self.position
    }

    pub(super) const fn allows_mutation(&self) -> bool {
        self.allows_mutation
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceExportBuildError {
    /// One exported declaration has no completed semantic fact in the bundle.
    MissingSemanticFacts(ExternalSymbolKey),
    /// Structural semantic or support-graph validation rejected the bundle.
    Validation(InterfaceValidationError),
    /// Two executable templates claim the same callable owner.
    DuplicateExecutableTemplate(bray_symbols::InterfaceSymbolId),
    /// Two native boundaries claim the same function owner.
    DuplicateNativeBoundary(bray_symbols::InterfaceSymbolId),
}

/// One validated immutable library surface ready for deterministic interface encoding.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageInterfaceExportBundle {
    surface: PackageInterfaceSurface,
    semantic_facts: InterfaceSemanticFacts,
    executable_templates: Arc<[crate::InterfaceExecutableTemplate]>,
    native_boundaries: Arc<[crate::InterfaceNativeBoundary]>,
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

        crate::semantic::validate_constraint_templates(&semantic_facts)
            .map_err(PackageInterfaceExportBuildError::Validation)?;

        Ok(Self {
            surface,
            semantic_facts,
            executable_templates: Arc::from([]),
            native_boundaries: Arc::from([]),
            language_revision,
        })
    }

    /// Attaches executable templates in canonical owner order.
    pub fn with_executable_templates(
        mut self,
        templates: impl IntoIterator<Item = crate::InterfaceExecutableTemplate>,
    ) -> Result<Self, PackageInterfaceExportBuildError> {
        let mut templates = templates.into_iter().collect::<Vec<_>>();

        templates.sort_by_key(crate::InterfaceExecutableTemplate::owner);

        for pair in templates.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageInterfaceExportBuildError::DuplicateExecutableTemplate(pair[0].owner()),
                );
            }
        }

        self.executable_templates = templates.into();

        Ok(self)
    }

    /// Returns the canonical exported package and symbol surface.
    pub const fn surface(&self) -> &PackageInterfaceSurface {
        &self.surface
    }

    /// Returns the complete semantic and private support graph.
    pub const fn semantic_facts(&self) -> &InterfaceSemanticFacts {
        &self.semantic_facts
    }

    /// Returns executable templates in canonical owner order.
    pub fn executable_templates(&self) -> &[crate::InterfaceExecutableTemplate] {
        &self.executable_templates
    }

    /// Attaches native boundaries in canonical owner order.
    pub fn with_native_boundaries(
        mut self,
        boundaries: impl IntoIterator<Item = crate::InterfaceNativeBoundary>,
    ) -> Result<Self, PackageInterfaceExportBuildError> {
        let mut boundaries = boundaries.into_iter().collect::<Vec<_>>();

        boundaries.sort_by_key(crate::InterfaceNativeBoundary::owner);

        for pair in boundaries.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(PackageInterfaceExportBuildError::DuplicateNativeBoundary(
                    pair[0].owner(),
                ));
            }
        }

        self.native_boundaries = boundaries.into();

        Ok(self)
    }

    /// Returns native boundaries in canonical owner order.
    pub fn native_boundaries(&self) -> &[crate::InterfaceNativeBoundary] {
        &self.native_boundaries
    }

    /// Returns the language semantic revision used to interpret the surface.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.language_revision
    }
}

fn validate_semantic_coverage(
    surface: &PackageInterfaceSurface,
    semantic_facts: &InterfaceSemanticFacts,
) -> Result<(), PackageInterfaceExportBuildError> {
    let fact_directory = semantic_facts.fact_directory();

    for symbol in surface.symbols().symbols() {
        if symbol.kind().is_callable() {
            require_owned_semantic_fact(
                &fact_directory,
                symbol.id(),
                symbol.key(),
                InterfaceSemanticFactKind::CallableSignature,
            )?;
        }

        if surface.relationships().iter().any(|relationship| {
            relationship.kind() == SymbolRelationshipKind::GenericParameter
                && relationship.owner() == symbol.id()
        }) {
            require_owned_semantic_fact(
                &fact_directory,
                symbol.id(),
                symbol.key(),
                InterfaceSemanticFactKind::GenericDeclaration,
            )?;
        }

        if symbol.kind() == SymbolKind::CallableParameter {
            require_owned_semantic_fact(
                &fact_directory,
                symbol.id(),
                symbol.key(),
                InterfaceSemanticFactKind::CallableParameterDefault,
            )?;
        }

        if matches!(
            symbol.kind(),
            SymbolKind::Predicate
                | SymbolKind::TraitPredicateMember
                | SymbolKind::TraitPredicateFulfillment
        ) {
            require_owned_semantic_fact(
                &fact_directory,
                symbol.id(),
                symbol.key(),
                InterfaceSemanticFactKind::PredicateDefinition,
            )?;
        }

        if symbol.kind().is_implementation() {
            require_owned_semantic_fact(
                &fact_directory,
                symbol.id(),
                symbol.key(),
                InterfaceSemanticFactKind::Implementation,
            )?;
        }

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

fn require_owned_semantic_fact(
    fact_directory: &[InterfaceSemanticFactEntry],
    owner: InterfaceSymbolId,
    key: &ExternalSymbolKey,
    kind: InterfaceSemanticFactKind,
) -> Result<(), PackageInterfaceExportBuildError> {
    let present = fact_directory.iter().any(|fact| {
        fact.kind() == kind
            && matches!(fact.owner(), InterfaceSymbolReference::Local(id) if *id == owner)
    });

    if present {
        Ok(())
    } else {
        Err(PackageInterfaceExportBuildError::MissingSemanticFacts(
            key.clone(),
        ))
    }
}

fn canonicalize_owner_addressed_facts(mut facts: InterfaceSemanticFacts) -> InterfaceSemanticFacts {
    for contract in Arc::make_mut(&mut facts.dependency_contracts) {
        Arc::make_mut(&mut contract.requirements).sort();
    }

    Arc::make_mut(&mut facts.constraints).sort();
    Arc::make_mut(&mut facts.callable_contracts).sort();
    Arc::make_mut(&mut facts.callable_signatures).sort();
    Arc::make_mut(&mut facts.generic_declarations).sort();
    Arc::make_mut(&mut facts.callable_parameter_defaults).sort();
    Arc::make_mut(&mut facts.predicate_definitions).sort();
    Arc::make_mut(&mut facts.declared_types).sort();
    Arc::make_mut(&mut facts.type_representations).sort();
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
            | bray_symbols::SymbolKind::Trait
            | bray_symbols::SymbolKind::CallableOverload
            | bray_symbols::SymbolKind::ImplementationOverload
            | bray_symbols::SymbolKind::GenericTypeParameter
            | bray_symbols::SymbolKind::GenericConstParameter
            | bray_symbols::SymbolKind::CallableParameter
            | bray_symbols::SymbolKind::PredicateParameter
            | bray_symbols::SymbolKind::ReceiverParameter
            | bray_symbols::SymbolKind::CallableParameterDefaultProvider
            | bray_symbols::SymbolKind::StructFieldDefaultProvider
            | bray_symbols::SymbolKind::UnionPayloadDefaultProvider
            | bray_symbols::SymbolKind::UnionVariant
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::CheckedTemplateKind;
    use bray_symbols::{CallableAbi, ExternalSymbolKey, SymbolKind};

    use crate::test_support::package_interface_export_bundle;
    use crate::{
        InterfaceAbiDependency, InterfaceLanguageRevision, InterfaceSemanticFactKind,
        InterfaceSemanticFacts, InterfaceSymbolReference, InterfaceValidationError,
        PackageInterfaceExportBuildError, PackageInterfaceExportBundle, encode_package_interface,
    };

    #[test]
    fn overload_sets_are_fully_described_by_surface_relationships() {
        assert!(!super::requires_owned_semantic_fact(
            SymbolKind::CallableOverload
        ));

        assert!(!super::requires_owned_semantic_fact(
            SymbolKind::ImplementationOverload
        ));
    }

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
    fn bundles_require_checked_templates_for_predicate_constraints() {
        let complete = package_interface_export_bundle();

        let facts = complete.semantic_facts().clone().with_templates(
            complete
                .semantic_facts()
                .checked_templates
                .iter()
                .filter(|template| template.kind() != CheckedTemplateKind::GenericConstraint)
                .cloned(),
            complete
                .semantic_facts()
                .declaration_templates
                .iter()
                .filter(|template| template.kind() != CheckedTemplateKind::GenericConstraint)
                .cloned(),
            complete
                .semantic_facts()
                .support_entities
                .iter()
                .take(2)
                .cloned(),
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

        let first_declaration = complete
            .surface()
            .symbols()
            .symbols()
            .iter()
            .find(|symbol| super::requires_owned_semantic_fact(symbol.kind()))
            .map(|symbol| symbol.key().clone())
            .unwrap_or_else(|| panic!("test surface must contain one declaration"));

        assert_eq!(
            PackageInterfaceExportBundle::try_new(
                complete.surface().clone(),
                InterfaceSemanticFacts::new(),
                InterfaceLanguageRevision::new(0),
            ),
            Err(PackageInterfaceExportBuildError::MissingSemanticFacts(
                first_declaration
            ))
        );
    }

    #[test]
    fn implementation_auxiliary_facts_do_not_replace_the_header() {
        let complete = package_interface_export_bundle();

        let implementation = complete
            .surface()
            .symbols()
            .symbols()
            .iter()
            .find(|symbol| symbol.kind().is_implementation())
            .map(|symbol| symbol.key().clone())
            .unwrap_or_else(|| panic!("test surface must contain one implementation"));

        let facts = complete
            .semantic_facts()
            .clone()
            .with_implementations([], []);

        assert_eq!(
            PackageInterfaceExportBundle::try_new(
                complete.surface().clone(),
                facts,
                InterfaceLanguageRevision::new(0),
            ),
            Err(PackageInterfaceExportBuildError::MissingSemanticFacts(
                implementation
            ))
        );
    }

    #[test]
    fn declaration_facts_are_each_required() {
        const REQUIRED_FACTS: &[InterfaceSemanticFactKind] = &[
            InterfaceSemanticFactKind::CallableSignature,
            InterfaceSemanticFactKind::GenericDeclaration,
            InterfaceSemanticFactKind::CallableParameterDefault,
            InterfaceSemanticFactKind::PredicateDefinition,
        ];

        for &missing in REQUIRED_FACTS {
            let complete = package_interface_export_bundle();
            let facts = complete.semantic_facts();

            let incomplete = facts.clone().with_declarations(
                facts
                    .callable_signatures()
                    .iter()
                    .filter(|_| missing != InterfaceSemanticFactKind::CallableSignature)
                    .cloned(),
                facts
                    .generic_declarations()
                    .iter()
                    .filter(|_| missing != InterfaceSemanticFactKind::GenericDeclaration)
                    .cloned(),
                facts
                    .callable_parameter_defaults()
                    .iter()
                    .filter(|_| missing != InterfaceSemanticFactKind::CallableParameterDefault)
                    .cloned(),
                facts
                    .predicate_definitions()
                    .iter()
                    .filter(|_| missing != InterfaceSemanticFactKind::PredicateDefinition)
                    .cloned(),
            );

            let owner_kind = match missing {
                InterfaceSemanticFactKind::CallableParameterDefault => {
                    SymbolKind::CallableParameter
                }
                InterfaceSemanticFactKind::PredicateDefinition => SymbolKind::Predicate,
                _ => SymbolKind::Function,
            };

            let owner = complete
                .surface()
                .symbols()
                .symbols()
                .iter()
                .find(|symbol| symbol.kind() == owner_kind)
                .map(|symbol| symbol.key().clone())
                .unwrap_or_else(|| panic!("test surface must contain {owner_kind:?}"));

            assert_eq!(
                PackageInterfaceExportBundle::try_new(
                    complete.surface().clone(),
                    incomplete,
                    InterfaceLanguageRevision::new(0),
                ),
                Err(PackageInterfaceExportBuildError::MissingSemanticFacts(
                    owner
                ))
            );
        }
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
            complete
                .semantic_facts()
                .target_dependencies()
                .iter()
                .cloned(),
            [
                InterfaceAbiDependency::new(first_owner.clone(), CallableAbi::Bray),
                InterfaceAbiDependency::new(second_owner.clone(), CallableAbi::C),
            ],
        );

        let second_facts = complete.semantic_facts().clone().with_target_dependencies(
            complete
                .semantic_facts()
                .target_dependencies()
                .iter()
                .cloned(),
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
        assert_send_sync::<crate::InterfaceArtifact>();
        assert_send_sync::<ExternalSymbolKey>();
    }
}
