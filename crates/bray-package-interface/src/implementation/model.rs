use std::ops::Range;
use std::sync::Arc;

use bray_bound_tree::CheckedTemplateKind;
use bray_symbols::InterfaceSymbolId;

use crate::decode::{DecodeBudget, map_wire_error};
use crate::semantic::{decode_template_payload, encode_template_payload};
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceArtifact, InterfaceCheckedTemplate, InterfaceContentHash, InterfaceLanguageRevision,
    InterfaceLimit, InterfaceSemanticFacts, InterfaceValidationError, InterfaceValidationLimits,
    InterfaceValidationPolicy, PackageInterfaceExportBundle, PackageInterfaceSurface,
    ValidatedPackageInterface,
};

const MAGIC: [u8; 8] = *b"BRAYIMPL";
const FORMAT_VERSION: u16 = 1;
const HEADER_LENGTH: usize = 48;
const DIRECTORY_ENTRY_LENGTH: usize = 24;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum ImplementationPayloadKind {
    ConstantCallableBody = 0,
    ExecutableTemplate = 1,
}

impl ImplementationPayloadKind {
    const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::ConstantCallableBody),
            1 => Some(Self::ExecutableTemplate),
            _ => None,
        }
    }
}

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

/// One source-independent generic executable body addressed by interface identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceExecutableTemplate {
    owner: InterfaceSymbolId,
    payload: Arc<[u8]>,
}

impl InterfaceExecutableTemplate {
    /// Creates one encoded executable template for a generic callable declaration.
    pub fn new(owner: InterfaceSymbolId, payload: impl Into<Arc<[u8]>>) -> Option<Self> {
        let payload = payload.into();

        (!payload.is_empty()).then_some(Self { owner, payload })
    }

    /// Returns the callable declaration that owns this template.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the canonical source-independent executable payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImplementationDirectoryEntry {
    owner: InterfaceSymbolId,
    kind: ImplementationPayloadKind,
    payload: Range<usize>,
}

/// An immutable package implementation artifact associated with one semantic interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageImplementationArtifact {
    bytes: Arc<[u8]>,
    interface_content_hash: InterfaceContentHash,
    language_revision: InterfaceLanguageRevision,
    directory: Arc<[ImplementationDirectoryEntry]>,
    limits: InterfaceValidationLimits,
}

impl PackageImplementationArtifact {
    /// Encodes the implementation payloads associated with one interface export.
    pub fn try_from_export_bundle(
        interface: &InterfaceArtifact,
        bundle: &PackageInterfaceExportBundle,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, PackageImplementationArtifactBuildError> {
        let validated = ValidatedPackageInterface::try_new(
            interface.shared_bytes(),
            InterfaceValidationPolicy::new(bundle.language_revision()).with_limits(limits),
        )
        .map_err(PackageImplementationArtifactBuildError::InvalidArtifact)?;

        Self::try_new(
            &validated,
            bundle.surface(),
            bundle.semantic_facts(),
            [],
            bundle.executable_templates().iter().cloned(),
            limits,
        )
    }

    /// Validates and encodes implementation payloads for one exact package interface.
    pub fn try_new(
        interface: &ValidatedPackageInterface,
        surface: &PackageInterfaceSurface,
        semantic_facts: &InterfaceSemanticFacts,
        constant_callable_bodies: impl IntoIterator<Item = InterfaceConstantCallableBody>,
        executable_templates: impl IntoIterator<Item = InterfaceExecutableTemplate>,
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

        let mut templates = executable_templates.into_iter().collect::<Vec<_>>();

        templates.sort_by_key(InterfaceExecutableTemplate::owner);

        for pair in templates.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateExecutableTemplate(
                        pair[0].owner(),
                    ),
                );
            }
        }

        for template in &templates {
            validate_executable_owner(surface, semantic_facts, template.owner())?;
        }

        let bytes = encode_artifact(
            interface.header().content_hash(),
            interface.header().language_revision(),
            &bodies,
            &templates,
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

        limits.check(
            InterfaceLimit::FileSize,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        )?;

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

        let mut budget = DecodeBudget::new(limits);
        let mut directory = budget.allocate_items(&reader, count)?;
        let mut expected_offset = 0_u64;

        for _ in 0..count {
            let owner = InterfaceSymbolId::new(reader.read_u32().map_err(map_wire_error)?);

            let kind = ImplementationPayloadKind::from_raw(
                reader.read_array::<1>().map_err(map_wire_error)?[0],
            )
            .ok_or(InterfaceValidationError::Malformed)?;

            if reader.read_array::<3>().map_err(map_wire_error)? != [0; 3] {
                return Err(InterfaceValidationError::Malformed);
            }

            let offset = reader.read_u64().map_err(map_wire_error)?;
            let length = reader.read_u64().map_err(map_wire_error)?;

            limits.check(InterfaceLimit::BlobLength, length)?;

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
                .is_some_and(|previous: &ImplementationDirectoryEntry| {
                    (previous.owner, previous.kind) >= (owner, kind)
                })
            {
                return Err(InterfaceValidationError::Malformed);
            }

            directory.push(ImplementationDirectoryEntry {
                owner,
                kind,
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
            directory: directory.into(),
            limits,
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

    /// Returns the canonical encoded implementation artifact bytes.
    pub fn shared_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }

    /// Decodes and validates only the requested checked body payload.
    pub fn constant_callable_body(
        &self,
        owner: InterfaceSymbolId,
        surface: &PackageInterfaceSurface,
    ) -> Result<Option<InterfaceConstantCallableBody>, InterfaceValidationError> {
        let Some(entry) = self.entry(owner, ImplementationPayloadKind::ConstantCallableBody) else {
            return Ok(None);
        };

        let payload = self
            .bytes
            .get(entry.payload.clone())
            .ok_or(InterfaceValidationError::Malformed)?;

        let template = decode_template_payload(payload, self.limits)?;

        validate_body_owner(surface, owner, &template)
            .map_err(|_| InterfaceValidationError::Malformed)?;

        Ok(Some(InterfaceConstantCallableBody::new(owner, template)))
    }

    /// Returns the independently encoded executable template for one generic callable.
    pub fn executable_template(
        &self,
        owner: InterfaceSymbolId,
    ) -> Result<Option<InterfaceExecutableTemplate>, InterfaceValidationError> {
        let Some(entry) = self.entry(owner, ImplementationPayloadKind::ExecutableTemplate) else {
            return Ok(None);
        };

        let payload = self
            .bytes
            .get(entry.payload.clone())
            .ok_or(InterfaceValidationError::Malformed)?;

        self.limits.check(
            InterfaceLimit::DecodedAllocation,
            u64::try_from(payload.len()).unwrap_or(u64::MAX),
        )?;

        InterfaceExecutableTemplate::new(owner, Arc::<[u8]>::from(payload))
            .map(Some)
            .ok_or(InterfaceValidationError::Malformed)
    }

    fn entry(
        &self,
        owner: InterfaceSymbolId,
        kind: ImplementationPayloadKind,
    ) -> Option<&ImplementationDirectoryEntry> {
        self.directory
            .binary_search_by_key(&(owner, kind), |entry| (entry.owner, entry.kind))
            .ok()
            .and_then(|index| self.directory.get(index))
    }
}

fn validate_executable_owner(
    surface: &PackageInterfaceSurface,
    semantic_facts: &InterfaceSemanticFacts,
    owner: InterfaceSymbolId,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidCallableOwner(owner));
    };

    let generic = semantic_facts
        .generic_declarations()
        .iter()
        .find(|declaration| {
            matches!(declaration.owner(), crate::InterfaceSymbolReference::Local(id) if *id == owner)
        });

    if !owner_symbol.kind().is_callable()
        || generic.is_none_or(|generic| generic.parameters().is_empty())
    {
        return Err(PackageImplementationArtifactBuildError::InvalidCallableOwner(owner));
    }

    Ok(())
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
    templates: &[InterfaceExecutableTemplate],
) -> Result<Arc<[u8]>, PackageImplementationArtifactBuildError> {
    let mut payloads = bodies
        .iter()
        .map(|body| {
            (
                body.owner(),
                ImplementationPayloadKind::ConstantCallableBody,
                encode_template_payload(body.template()),
            )
        })
        .collect::<Vec<_>>();

    payloads.extend(templates.iter().map(|template| {
        (
            template.owner(),
            ImplementationPayloadKind::ExecutableTemplate,
            template.payload().to_vec(),
        )
    }));

    payloads.sort_by_key(|(owner, kind, _)| (*owner, *kind));

    let count = u32::try_from(payloads.len()).map_err(|_| {
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

    for (owner, kind, payload) in &payloads {
        let length = u64::try_from(payload.len()).map_err(|_| {
            PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::Malformed,
            )
        })?;

        encoder.write_u32(owner.raw());
        encoder.write_bytes(&[*kind as u8]);
        encoder.write_bytes(&[0; 3]);
        encoder.write_u64(offset);
        encoder.write_u64(length);

        offset = offset.checked_add(length).ok_or(
            PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::Malformed,
            ),
        )?;
    }

    for (_, _, payload) in payloads {
        encoder.write_bytes(&payload);
    }

    Ok(encoder.into_bytes().into())
}

/// Failure while assembling a package implementation artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageImplementationArtifactBuildError {
    /// Two payloads claim the same callable identity.
    DuplicateCallableBody(InterfaceSymbolId),
    /// Two executable templates claim the same callable identity.
    DuplicateExecutableTemplate(InterfaceSymbolId),
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
    use bray_symbols::{InterfaceSymbolId, SymbolKind};

    use super::{
        InterfaceConstantCallableBody, InterfaceExecutableTemplate, PackageImplementationArtifact,
        PackageImplementationArtifactBuildError,
    };
    use crate::{
        InterfaceCheckedTemplate, InterfaceLanguageRevision, InterfaceValidationError,
        InterfaceValidationLimits, InterfaceValidationPolicy, ValidatedPackageInterface,
        encode_package_interface,
    };

    #[test]
    fn artifacts_decode_only_the_requested_constant_body() {
        let fixture = artifact_fixture();

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantic_facts(),
            [fixture.body.clone()],
            [],
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
            &[],
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
        );

        assert_eq!(first.map(|body| body.is_some()), Ok(true));

        assert!(
            artifact
                .constant_callable_body(
                    second_owner,
                    fixture.bundle.surface(),
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
            [],
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

    #[test]
    fn artifacts_load_executable_templates_independently() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let template = InterfaceExecutableTemplate::new(owner, [1_u8, 2, 3])
            .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantic_facts(),
            [],
            [template.clone()],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("executable template artifact must validate: {error:?}"));

        assert_eq!(artifact.executable_template(owner), Ok(Some(template)));
    }

    #[test]
    fn artifacts_bound_executable_payloads_by_the_validation_policy() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let template = InterfaceExecutableTemplate::new(owner, [1_u8, 2, 3])
            .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

        let result = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantic_facts(),
            [],
            [template],
            InterfaceValidationLimits::default().with_blob_length(2),
        );

        assert_eq!(
            result,
            Err(PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::ResourceLimitExceeded {
                    limit: crate::InterfaceLimit::BlobLength,
                    actual: 3,
                    maximum: 2,
                }
            ))
        );
    }

    struct ArtifactFixture {
        interface: ValidatedPackageInterface,
        bundle: crate::PackageInterfaceExportBundle,
        body: InterfaceConstantCallableBody,
    }

    fn generic_callable_owner(bundle: &crate::PackageInterfaceExportBundle) -> InterfaceSymbolId {
        bundle
            .semantic_facts()
            .generic_declarations()
            .iter()
            .find_map(|declaration| match declaration.owner() {
                crate::InterfaceSymbolReference::Local(owner)
                    if bundle
                        .surface()
                        .symbols()
                        .symbol(*owner)
                        .is_some_and(|symbol| symbol.kind().is_callable()) =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("test interface must export a generic callable"))
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
