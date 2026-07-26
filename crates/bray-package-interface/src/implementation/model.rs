use std::sync::Arc;

use bray_bound_tree::CheckedTemplateKind;
use bray_symbols::InterfaceSymbolId;

use crate::{
    InterfaceCheckedTemplate, InterfaceContentHash, InterfaceLanguageRevision,
    InterfaceSemanticFacts, InterfaceValidationError, InterfaceValidationLimits,
    PackageInterfaceSurface, ValidatedPackageInterface,
};

/// One checked const-callable body addressed by its public interface identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceConstantCallableBody {
    owner: InterfaceSymbolId,
    template: InterfaceCheckedTemplate,
}

impl InterfaceConstantCallableBody {
    /// Creates one source-independent const-callable body payload.
    pub const fn new(owner: InterfaceSymbolId, template: InterfaceCheckedTemplate) -> Self {
        Self { owner, template }
    }

    /// Returns the callable declaration that owns this body.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the checked body template.
    pub const fn template(&self) -> &InterfaceCheckedTemplate {
        &self.template
    }
}

/// An immutable package implementation artifact associated with one semantic interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageImplementationArtifact {
    interface_content_hash: InterfaceContentHash,
    language_revision: InterfaceLanguageRevision,
    constant_callable_bodies: Arc<[InterfaceConstantCallableBody]>,
}

impl PackageImplementationArtifact {
    /// Validates and assembles implementation payloads for one exact package interface.
    pub fn try_new(
        interface: &ValidatedPackageInterface,
        surface: &PackageInterfaceSurface,
        semantic_facts: &InterfaceSemanticFacts,
        constant_callable_bodies: impl IntoIterator<Item = InterfaceConstantCallableBody>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, PackageImplementationArtifactBuildError> {
        let mut constant_callable_bodies = constant_callable_bodies.into_iter().collect::<Vec<_>>();

        constant_callable_bodies.sort_by_key(InterfaceConstantCallableBody::owner);

        for pair in constant_callable_bodies.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateCallableBody(pair[0].owner()),
                );
            }
        }

        for body in &constant_callable_bodies {
            let Some(owner) = surface.symbols().symbol(body.owner()) else {
                return Err(
                    PackageImplementationArtifactBuildError::InvalidCallableOwner(body.owner()),
                );
            };

            if !owner.kind().is_callable()
                || body.template().kind() != CheckedTemplateKind::ConstantCallableBody
            {
                return Err(
                    PackageImplementationArtifactBuildError::InvalidCallableOwner(body.owner()),
                );
            }

            semantic_facts
                .validate_implementation_template(surface, body.template(), limits)
                .map_err(PackageImplementationArtifactBuildError::InvalidBody)?;
        }

        Ok(Self {
            interface_content_hash: interface.header().content_hash(),
            language_revision: interface.header().language_revision(),
            constant_callable_bodies: constant_callable_bodies.into(),
        })
    }

    /// Returns the semantic interface identity required by this artifact.
    pub const fn interface_content_hash(&self) -> InterfaceContentHash {
        self.interface_content_hash
    }

    /// Returns the language revision required by this artifact.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.language_revision
    }

    /// Returns the requested checked body without inspecting unrelated payloads.
    pub fn constant_callable_body(
        &self,
        owner: InterfaceSymbolId,
    ) -> Option<&InterfaceConstantCallableBody> {
        self.constant_callable_bodies
            .binary_search_by_key(&owner, InterfaceConstantCallableBody::owner)
            .ok()
            .and_then(|index| self.constant_callable_bodies.get(index))
    }

    /// Returns checked const-callable payloads in canonical owner order.
    pub fn constant_callable_bodies(&self) -> &[InterfaceConstantCallableBody] {
        &self.constant_callable_bodies
    }
}

/// Failure while assembling a package implementation artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageImplementationArtifactBuildError {
    /// Two payloads claim the same callable identity.
    DuplicateCallableBody(InterfaceSymbolId),
    /// A payload owner is missing, is not callable, or disagrees with the body category.
    InvalidCallableOwner(InterfaceSymbolId),
    /// A checked body does not form a valid source-independent template graph.
    InvalidBody(InterfaceValidationError),
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::CheckedTemplateKind;
    use bray_symbols::SymbolKind;

    use super::{
        InterfaceConstantCallableBody, PackageImplementationArtifact,
        PackageImplementationArtifactBuildError,
    };
    use crate::{
        InterfaceCheckedTemplate, InterfaceLanguageRevision, InterfaceValidationLimits,
        InterfaceValidationPolicy, ValidatedPackageInterface, encode_package_interface,
    };

    #[test]
    fn artifacts_retain_one_demand_addressable_constant_body() {
        let fixture = artifact_fixture();

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantic_facts(),
            [fixture.body.clone()],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("constant body artifact must validate: {error:?}"));

        assert_eq!(
            artifact.interface_content_hash(),
            fixture.interface.header().content_hash()
        );

        assert_eq!(
            artifact.language_revision(),
            fixture.interface.header().language_revision()
        );

        assert_eq!(
            artifact.constant_callable_body(fixture.body.owner()),
            Some(&fixture.body)
        );
    }

    #[test]
    fn artifacts_reject_duplicate_constant_body_owners() {
        let fixture = artifact_fixture();

        let result = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantic_facts(),
            [fixture.body.clone(), fixture.body.clone()],
            InterfaceValidationLimits::default(),
        );

        assert_eq!(
            result,
            Err(
                PackageImplementationArtifactBuildError::DuplicateCallableBody(
                    fixture.body.owner()
                )
            )
        );
    }

    struct ArtifactFixture {
        interface: ValidatedPackageInterface,
        bundle: crate::PackageInterfaceExportBundle,
        body: InterfaceConstantCallableBody,
    }

    fn artifact_fixture() -> ArtifactFixture {
        let bundle = crate::test_support::package_interface_export_bundle();

        let encoded = encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

        let interface = ValidatedPackageInterface::try_new(
            encoded.bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap_or_else(|error| panic!("test interface must validate: {error:?}"));

        let owner = bundle
            .surface()
            .symbols()
            .symbols()
            .iter()
            .find(|symbol| symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.id())
            .unwrap_or_else(|| panic!("test interface must export a function"));

        let template = bundle
            .semantic_facts()
            .checked_templates()
            .first()
            .unwrap_or_else(|| panic!("test interface must publish a checked template"));

        let template = InterfaceCheckedTemplate::new(
            CheckedTemplateKind::ConstantCallableBody,
            template.inputs().iter().cloned(),
            template.nodes().iter().cloned(),
            template.temporaries().iter().copied(),
            template.result(),
            template.behavior().clone(),
        );

        ArtifactFixture {
            interface,
            bundle,
            body: InterfaceConstantCallableBody::new(owner, template),
        }
    }
}
