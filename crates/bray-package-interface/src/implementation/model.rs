use std::ops::Range;
use std::sync::Arc;

use bray_bound_tree::CheckedTemplateKind;
use bray_symbols::InterfaceSymbolId;

use crate::decode::map_wire_error;
use crate::semantic::{decode_template_payload, encode_template_payload};
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceCheckedTemplate, InterfaceContentHash, InterfaceLanguageRevision, InterfaceLimit,
    InterfaceSemanticFacts, InterfaceValidationError, InterfaceValidationLimits,
    PackageInterfaceSurface, ValidatedPackageInterface,
};

const MAGIC: [u8; 8] = *b"BRAYIMPL";
const FORMAT_VERSION: u16 = 1;
const HEADER_LENGTH: usize = 48;
const DIRECTORY_ENTRY_LENGTH: usize = 20;

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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImplementationDirectoryEntry {
    owner: InterfaceSymbolId,
    payload: Range<usize>,
}

/// An immutable package implementation artifact associated with one semantic interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageImplementationArtifact {
    bytes: Arc<[u8]>,
    interface_content_hash: InterfaceContentHash,
    language_revision: InterfaceLanguageRevision,
    constant_callable_bodies: Arc<[ImplementationDirectoryEntry]>,
}

impl PackageImplementationArtifact {
    /// Validates and encodes implementation payloads for one exact package interface.
    pub fn try_new(
        interface: &ValidatedPackageInterface,
        surface: &PackageInterfaceSurface,
        semantic_facts: &InterfaceSemanticFacts,
        constant_callable_bodies: impl IntoIterator<Item = InterfaceConstantCallableBody>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, PackageImplementationArtifactBuildError> {
        let mut bodies = constant_callable_bodies.into_iter().collect::<Vec<_>>();

        bodies.sort_by_key(InterfaceConstantCallableBody::owner);

        for pair in bodies.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateCallableBody(pair[0].owner()),
                );
            }
        }

        for body in &bodies {
            validate_body_owner(surface, body.owner(), body.template())?;

            semantic_facts
                .validate_implementation_template(surface, body.template(), limits)
                .map_err(PackageImplementationArtifactBuildError::InvalidBody)?;
        }

        let bytes = encode_artifact(
            interface.header().content_hash(),
            interface.header().language_revision(),
            &bodies,
        )?;

        Self::try_from_bytes(bytes, limits)
            .map_err(PackageImplementationArtifactBuildError::InvalidArtifact)
    }

    /// Validates the fixed header and canonical directory without decoding body payloads.
    pub fn try_from_bytes(
        bytes: impl Into<Arc<[u8]>>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        let bytes = bytes.into();
        let mut reader = WireReader::new(&bytes);

        if reader.read_array::<8>().map_err(map_wire_error)? != MAGIC {
            return Err(InterfaceValidationError::Malformed);
        }

        if reader.read_u16().map_err(map_wire_error)? != FORMAT_VERSION {
            return Err(InterfaceValidationError::Malformed);
        }

        let language_revision =
            InterfaceLanguageRevision::new(reader.read_u16().map_err(map_wire_error)?);

        let interface_content_hash =
            InterfaceContentHash::from_bytes(reader.read_array::<32>().map_err(map_wire_error)?);

        let count = reader.read_u32().map_err(map_wire_error)?;

        limits.check(InterfaceLimit::RecordCount, u64::from(count))?;

        let count = usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed)?;

        let directory_length = count
            .checked_mul(DIRECTORY_ENTRY_LENGTH)
            .ok_or(InterfaceValidationError::Malformed)?;

        let payload_start = HEADER_LENGTH
            .checked_add(directory_length)
            .ok_or(InterfaceValidationError::Malformed)?;

        if payload_start > bytes.len() {
            return Err(InterfaceValidationError::Truncated);
        }

        let mut directory = Vec::with_capacity(count);
        let mut expected_offset = 0_u64;

        for _ in 0..count {
            let owner = InterfaceSymbolId::new(reader.read_u32().map_err(map_wire_error)?);
            let offset = reader.read_u64().map_err(map_wire_error)?;
            let length = reader.read_u64().map_err(map_wire_error)?;

            if offset != expected_offset {
                return Err(InterfaceValidationError::Malformed);
            }

            let end = offset
                .checked_add(length)
                .ok_or(InterfaceValidationError::Malformed)?;

            let start = payload_start
                .checked_add(
                    usize::try_from(offset).map_err(|_| InterfaceValidationError::Malformed)?,
                )
                .ok_or(InterfaceValidationError::Malformed)?;

            let end_index = payload_start
                .checked_add(usize::try_from(end).map_err(|_| InterfaceValidationError::Malformed)?)
                .ok_or(InterfaceValidationError::Malformed)?;

            if end_index > bytes.len() {
                return Err(InterfaceValidationError::Truncated);
            }

            if directory
                .last()
                .is_some_and(|previous: &ImplementationDirectoryEntry| previous.owner >= owner)
            {
                return Err(InterfaceValidationError::Malformed);
            }

            directory.push(ImplementationDirectoryEntry {
                owner,
                payload: start..end_index,
            });

            expected_offset = end;
        }

        let expected_length = payload_start
            .checked_add(
                usize::try_from(expected_offset)
                    .map_err(|_| InterfaceValidationError::Malformed)?,
            )
            .ok_or(InterfaceValidationError::Malformed)?;

        if expected_length != bytes.len() {
            return Err(InterfaceValidationError::Malformed);
        }

        Ok(Self {
            bytes,
            interface_content_hash,
            language_revision,
            constant_callable_bodies: directory.into(),
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

    /// Returns the canonical encoded implementation artifact.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Decodes and validates only the requested checked body payload.
    pub fn constant_callable_body(
        &self,
        owner: InterfaceSymbolId,
        surface: &PackageInterfaceSurface,
        limits: InterfaceValidationLimits,
    ) -> Result<Option<InterfaceConstantCallableBody>, InterfaceValidationError> {
        let Ok(index) = self
            .constant_callable_bodies
            .binary_search_by_key(&owner, |entry| entry.owner)
        else {
            return Ok(None);
        };

        let entry = self
            .constant_callable_bodies
            .get(index)
            .ok_or(InterfaceValidationError::Malformed)?;

        let payload = self
            .bytes
            .get(entry.payload.clone())
            .ok_or(InterfaceValidationError::Malformed)?;

        let template = decode_template_payload(payload, limits)?;

        validate_body_owner(surface, owner, &template)
            .map_err(|_| InterfaceValidationError::Malformed)?;

        Ok(Some(InterfaceConstantCallableBody::new(owner, template)))
    }
}

fn validate_body_owner(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    template: &InterfaceCheckedTemplate,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidCallableOwner(owner));
    };

    if !owner_symbol.kind().is_callable()
        || template.kind() != CheckedTemplateKind::ConstantCallableBody
    {
        return Err(PackageImplementationArtifactBuildError::InvalidCallableOwner(owner));
    }

    Ok(())
}

fn encode_artifact(
    interface_content_hash: InterfaceContentHash,
    language_revision: InterfaceLanguageRevision,
    bodies: &[InterfaceConstantCallableBody],
) -> Result<Arc<[u8]>, PackageImplementationArtifactBuildError> {
    let payloads = bodies
        .iter()
        .map(|body| encode_template_payload(body.template()))
        .collect::<Vec<_>>();

    let count = u32::try_from(bodies.len()).map_err(|_| {
        PackageImplementationArtifactBuildError::InvalidArtifact(
            InterfaceValidationError::Malformed,
        )
    })?;

    let mut encoder = WireEncoder::new();

    encoder.write_bytes(&MAGIC);
    encoder.write_u16(FORMAT_VERSION);
    encoder.write_u16(language_revision.raw());
    encoder.write_bytes(interface_content_hash.as_bytes());
    encoder.write_u32(count);

    let mut offset = 0_u64;

    for (body, payload) in bodies.iter().zip(&payloads) {
        let length = u64::try_from(payload.len()).map_err(|_| {
            PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::Malformed,
            )
        })?;

        encoder.write_u32(body.owner().raw());
        encoder.write_u64(offset);
        encoder.write_u64(length);

        offset = offset.checked_add(length).ok_or(
            PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::Malformed,
            ),
        )?;
    }

    for payload in payloads {
        encoder.write_bytes(&payload);
    }

    Ok(encoder.into_bytes().into())
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
    /// The encoded artifact does not form a canonical demand-addressable container.
    InvalidArtifact(InterfaceValidationError),
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
    fn artifacts_decode_only_the_requested_constant_body() {
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

        let body = artifact
            .constant_callable_body(
                fixture.body.owner(),
                fixture.bundle.surface(),
                InterfaceValidationLimits::default(),
            )
            .unwrap_or_else(|error| panic!("requested body must decode: {error:?}"));

        assert_eq!(body, Some(fixture.body));
    }

    #[test]
    fn malformed_unrequested_payloads_do_not_block_other_body_lookups() {
        let fixture = artifact_fixture();

        let second_owner =
            bray_symbols::InterfaceSymbolId::new(fixture.body.owner().raw().saturating_add(100));

        let second =
            InterfaceConstantCallableBody::new(second_owner, fixture.body.template.clone());

        let mut bytes = super::encode_artifact(
            fixture.interface.header().content_hash(),
            fixture.interface.header().language_revision(),
            &[fixture.body.clone(), second],
        )
        .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"))
        .to_vec();

        let last = bytes
            .last_mut()
            .unwrap_or_else(|| panic!("artifact must contain a body payload"));

        *last ^= 0xff;

        let artifact = PackageImplementationArtifact::try_from_bytes(
            bytes,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("directory validation must remain lazy: {error:?}"));

        let first = artifact.constant_callable_body(
            fixture.body.owner(),
            fixture.bundle.surface(),
            InterfaceValidationLimits::default(),
        );

        assert_eq!(first.map(|body| body.is_some()), Ok(true));

        assert!(
            artifact
                .constant_callable_body(
                    second_owner,
                    fixture.bundle.surface(),
                    InterfaceValidationLimits::default(),
                )
                .is_err()
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
