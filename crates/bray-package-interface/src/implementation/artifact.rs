use std::ops::Range;
use std::sync::Arc;
use std::sync::OnceLock;

use bray_bound_tree::CheckedTemplateKind;
use bray_ir::{MirExecutableTemplateId, MirTargetContract, MirUnit, MirUnitId};
use bray_symbols::{AnySymbolId, InterfaceSymbolId};

use crate::decode::{DecodeBudget, map_wire_error};
use crate::semantic::decode_template_payload;
use crate::wire::WireReader;
use crate::{
    ImportedSemantics, InterfaceArtifact, InterfaceCheckedTemplate, InterfaceContentHash,
    InterfaceLanguageRevision, InterfaceLimit, InterfaceSemantics, InterfaceSymbolResolver,
    InterfaceValidationError, InterfaceValidationLimits, InterfaceValidationPolicy,
    PackageInterfaceExportBundle, PackageInterfaceSurface, ValidatedPackageInterface,
};

use super::artifact_decoding::{decode_directory_entry, decode_entry_payload};
use super::artifact_encoding::encode_artifact;
use super::codec::decode_identity;
#[cfg(test)]
use super::hash::compute_payload_hash;
use super::hash::{compute_artifact_hash, compute_content_hash};
use super::payload::{
    decode_native_boundary, decode_pre_specialized_mir, specialization_discriminator,
};
use super::{
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBoundary,
    InterfacePreSpecializedMir, PackageImplementationArtifactBuildError,
    PackageImplementationConfiguration, PackageImplementationIdentity,
    PackageImplementationSpecializationKey, PreSpecializedMirDecodeError,
    invalid_executable_template_family,
};

pub(super) const ARTIFACT_HASH_OFFSET: usize = 80;
pub(super) const BYTE_ORDER_MARKER: u32 = 0x0102_0304;
pub(super) const CONTENT_HASH_OFFSET: usize = 48;
pub(super) const DIRECTORY_ENTRY_LENGTH: usize = 148;
pub(super) const HEADER_LENGTH: usize = 112;
pub(super) const MAGIC: [u8; 8] = *b"BRAYM\0\r\n";
pub(super) const REQUIRED_FLAGS: u64 = 0;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(super) enum ImplementationPayloadKind {
    Identity = 0,
    ConstantCallableBody = 1,
    ExecutableTemplate = 2,
    NativeBoundary = 3,
    PreSpecializedMir = 4,
}

impl ImplementationPayloadKind {
    pub(super) const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Identity),
            1 => Some(Self::ConstantCallableBody),
            2 => Some(Self::ExecutableTemplate),
            3 => Some(Self::NativeBoundary),
            4 => Some(Self::PreSpecializedMir),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ImplementationDirectoryEntry {
    pub(super) owner: InterfaceSymbolId,
    pub(super) raw_kind: u8,
    pub(super) kind: Option<ImplementationPayloadKind>,
    pub(super) compatibility: crate::InterfaceSectionCompatibility,
    pub(super) encoding: crate::InterfaceSectionEncoding,
    pub(super) discriminator: [u8; 32],
    pub(super) family_size: u32,
    pub(super) decoded_length: u64,
    pub(super) record_count: u64,
    pub(super) checksum: [u8; 32],
    pub(super) content_hash: [u8; 32],
    pub(super) payload: Range<usize>,
}

/// An immutable package implementation artifact associated with one semantic interface.
#[derive(Clone, Debug)]
pub struct PackageImplementationArtifact {
    bytes: Arc<[u8]>,
    identity: PackageImplementationIdentity,
    content_hash: [u8; 32],
    artifact_hash: [u8; 32],
    directory: Arc<[ImplementationDirectoryEntry]>,
    decoded: Arc<[OnceLock<Result<Arc<[u8]>, InterfaceValidationError>>]>,
    limits: InterfaceValidationLimits,
}

impl PartialEq for PackageImplementationArtifact {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.identity == other.identity && self.limits == other.limits
    }
}

impl Eq for PackageImplementationArtifact {}

impl std::hash::Hash for PackageImplementationArtifact {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
        self.identity.hash(state);
        self.limits.hash(state);
    }
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
            bundle.semantics(),
            bundle.implementation_configuration().clone(),
            [],
            bundle.executable_templates().iter().cloned(),
            bundle.native_boundaries().iter().cloned(),
            [],
            limits,
        )
    }

    /// Validates and encodes implementation payloads for one exact package interface.
    pub fn try_new(
        interface: &ValidatedPackageInterface,
        surface: &PackageInterfaceSurface,
        semantics: &InterfaceSemantics,
        configuration: PackageImplementationConfiguration,
        constant_callable_bodies: impl IntoIterator<Item = InterfaceConstantCallableBody>,
        executable_templates: impl IntoIterator<Item = InterfaceExecutableTemplate>,
        native_boundaries: impl IntoIterator<Item = InterfaceNativeBoundary>,
        pre_specialized_mir: impl IntoIterator<Item = InterfacePreSpecializedMir>,
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

            semantics
                .validate_implementation_template(surface, body.template(), limits)
                .map_err(PackageImplementationArtifactBuildError::InvalidBody)?;
        }

        let mut templates = executable_templates.into_iter().collect::<Vec<_>>();

        templates.sort_by_key(|template| (template.owner(), template.identity()));

        for pair in templates.windows(2) {
            if (pair[0].owner(), pair[0].identity()) == (pair[1].owner(), pair[1].identity()) {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateExecutableTemplate(
                        pair[0].owner(),
                    ),
                );
            }
        }

        for template in &templates {
            validate_executable_owner(surface, template.owner())?;
        }

        validate_executable_template_families(&templates)?;

        let mut boundaries = native_boundaries.into_iter().collect::<Vec<_>>();

        boundaries.sort_by_key(InterfaceNativeBoundary::owner);

        for pair in boundaries.windows(2) {
            if pair[0].owner() == pair[1].owner() {
                return Err(
                    PackageImplementationArtifactBuildError::DuplicateNativeBoundary(
                        pair[0].owner(),
                    ),
                );
            }
        }

        for boundary in &boundaries {
            validate_native_boundary_owner(surface, boundary.owner())?;
        }

        let mut pre_specialized_mir = pre_specialized_mir.into_iter().collect::<Vec<_>>();

        pre_specialized_mir.sort_by_key(|mir| mir.key().cache_identity());

        for pair in pre_specialized_mir.windows(2) {
            if pair[0].key() == pair[1].key() {
                return Err(PackageImplementationArtifactBuildError::DuplicateSpecialization);
            }
        }

        if pre_specialized_mir.iter().any(|mir| {
            mir.key().configuration() != &configuration
                || mir.key().template_schema_revision() != super::CURRENT_TEMPLATE_SCHEMA_REVISION
                || mir.key().dependencies() != surface.dependencies()
                || mir.mir_schema_revision() != super::CURRENT_MIR_SCHEMA_REVISION
        }) {
            return Err(PackageImplementationArtifactBuildError::SpecializationIdentityMismatch);
        }

        let identity = implementation_identity(interface, surface, semantics, configuration);

        let bytes = encode_artifact(
            &identity,
            &bodies,
            &templates,
            &boundaries,
            &pre_specialized_mir,
        )?;

        Self::try_from_bytes(bytes, limits)
            .map_err(PackageImplementationArtifactBuildError::InvalidArtifact)
    }

    /// Validates canonical framing and identity without decoding unrelated body payloads.
    pub fn try_from_bytes(
        bytes: impl Into<Arc<[u8]>>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        let bytes = bytes.into();

        limits.check(
            InterfaceLimit::FileSize,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        )?;

        let header = bytes
            .get(..HEADER_LENGTH)
            .ok_or(InterfaceValidationError::Truncated)?;

        let mut reader = WireReader::new(header);

        if reader.read_array::<8>().map_err(map_wire_error)? != MAGIC {
            return Err(InterfaceValidationError::Malformed);
        }

        if reader.read_u16().map_err(map_wire_error)? != crate::CURRENT_FORMAT_REVISION.raw() {
            return Err(InterfaceValidationError::Malformed);
        }

        let language_revision =
            InterfaceLanguageRevision::new(reader.read_u16().map_err(map_wire_error)?);

        if reader.read_u32().map_err(map_wire_error)? != BYTE_ORDER_MARKER
            || reader.read_u64().map_err(map_wire_error)? != REQUIRED_FLAGS
        {
            return Err(InterfaceValidationError::Malformed);
        }

        let declared_file_length = reader.read_u64().map_err(map_wire_error)?;
        let directory_offset = reader.read_u64().map_err(map_wire_error)?;
        let directory_length = reader.read_u64().map_err(map_wire_error)?;
        let content_hash = reader.read_array::<32>().map_err(map_wire_error)?;
        let artifact_hash = reader.read_array::<32>().map_err(map_wire_error)?;

        reader.finish().map_err(map_wire_error)?;

        if declared_file_length != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
            || compute_artifact_hash(&bytes) != Some(artifact_hash)
        {
            return Err(InterfaceValidationError::HashMismatch);
        }

        let directory_offset =
            usize::try_from(directory_offset).map_err(|_| InterfaceValidationError::Malformed)?;

        let directory_length =
            usize::try_from(directory_length).map_err(|_| InterfaceValidationError::Malformed)?;

        if directory_offset < HEADER_LENGTH
            || directory_offset.checked_add(directory_length) != Some(bytes.len())
            || directory_length % DIRECTORY_ENTRY_LENGTH != 0
        {
            return Err(InterfaceValidationError::Malformed);
        }

        let count = directory_length / DIRECTORY_ENTRY_LENGTH;

        limits.check(
            InterfaceLimit::ImplementationEntryCount,
            u64::try_from(count).unwrap_or(u64::MAX),
        )?;

        let directory_bytes = bytes
            .get(directory_offset..)
            .ok_or(InterfaceValidationError::Truncated)?;

        let mut directory_reader = WireReader::new(directory_bytes);
        let mut budget = DecodeBudget::new(limits);
        let mut directory = budget.allocate_items(&directory_reader, count)?;
        let mut expected_offset = HEADER_LENGTH;
        let mut decoded_total = 0_u64;

        for _ in 0..count {
            let entry = decode_directory_entry(
                &mut directory_reader,
                &bytes,
                directory_offset,
                expected_offset,
                limits,
            )?;

            decoded_total = decoded_total
                .checked_add(entry.decoded_length)
                .ok_or(InterfaceValidationError::Malformed)?;

            limits.check(InterfaceLimit::DecodedAllocation, decoded_total)?;

            if directory
                .last()
                .is_some_and(|previous: &ImplementationDirectoryEntry| {
                    (previous.owner, previous.raw_kind, previous.discriminator)
                        >= (entry.owner, entry.raw_kind, entry.discriminator)
                })
            {
                return Err(InterfaceValidationError::Malformed);
            }

            expected_offset = entry.payload.end;
            directory.push(entry);
        }

        directory_reader.finish().map_err(map_wire_error)?;

        if expected_offset != directory_offset
            || compute_content_hash(language_revision, &directory) != content_hash
        {
            return Err(InterfaceValidationError::HashMismatch);
        }

        validate_encoded_executable_template_families(&directory)?;

        let identity_entries = directory
            .iter()
            .filter(|entry| entry.kind == Some(ImplementationPayloadKind::Identity))
            .collect::<Vec<_>>();

        let [identity_entry] = identity_entries.as_slice() else {
            return Err(InterfaceValidationError::Malformed);
        };

        let identity_payload = decode_entry_payload(&bytes, identity_entry, limits)?;
        let identity = decode_identity(&identity_payload, limits)?;

        if identity.language_revision() != language_revision {
            return Err(InterfaceValidationError::Malformed);
        }

        let decoded = (0..directory.len()).map(|_| OnceLock::new()).collect();

        Ok(Self {
            bytes,
            identity,
            content_hash,
            artifact_hash,
            directory: directory.into(),
            decoded,
            limits,
        })
    }

    /// Returns the complete package, interface, dependency, target, and runtime identity.
    pub const fn identity(&self) -> &PackageImplementationIdentity {
        &self.identity
    }

    /// Returns the semantic interface identity required by this artifact.
    pub const fn interface_content_hash(&self) -> InterfaceContentHash {
        self.identity.interface_content_hash()
    }

    /// Returns the language revision required by this artifact.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.identity.language_revision()
    }

    /// Returns the semantic content hash of the complete implementation bundle.
    pub const fn content_hash(&self) -> &[u8; 32] {
        &self.content_hash
    }

    /// Returns the exact canonical artifact hash.
    pub const fn artifact_hash(&self) -> &[u8; 32] {
        &self.artifact_hash
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
        let Some((index, entry)) = self.entry(
            owner,
            ImplementationPayloadKind::ConstantCallableBody,
            [0; 32],
        ) else {
            return Ok(None);
        };

        let payload = self.payload(index, entry)?;

        let template = decode_template_payload(&payload, self.limits)?;

        validate_body_owner(surface, owner, &template)
            .map_err(|_| InterfaceValidationError::Malformed)?;

        Ok(Some(InterfaceConstantCallableBody::new(owner, template)))
    }

    /// Returns the independently encoded executable template for one declaration.
    pub fn executable_template(
        &self,
        owner: InterfaceSymbolId,
        identity: bray_ir::MirExecutableTemplateId,
    ) -> Result<Option<InterfaceExecutableTemplate>, InterfaceValidationError> {
        let Some((index, entry)) = self.entry(
            owner,
            ImplementationPayloadKind::ExecutableTemplate,
            executable_discriminator(identity.raw()),
        ) else {
            return Ok(None);
        };

        let payload = self.payload(index, entry)?;

        InterfaceExecutableTemplate::new(owner, identity, entry.family_size, payload)
            .map(Some)
            .ok_or(InterfaceValidationError::Malformed)
    }

    /// Returns the native symbol boundary of one declaration, when present.
    pub fn native_boundary(
        &self,
        owner: InterfaceSymbolId,
    ) -> Result<Option<InterfaceNativeBoundary>, InterfaceValidationError> {
        let Some((index, entry)) =
            self.entry(owner, ImplementationPayloadKind::NativeBoundary, [0; 32])
        else {
            return Ok(None);
        };

        let payload = self.payload(index, entry)?;

        decode_native_boundary(owner, &payload, self.limits).map(Some)
    }

    /// Lazily reconstructs optional MIR only when its complete specialization key matches exactly.
    pub fn pre_specialized_mir(
        &self,
        key: &PackageImplementationSpecializationKey,
        owner: AnySymbolId,
        unit: MirUnitId,
        target: MirTargetContract,
        properties: &ImportedSemantics,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<MirUnit>, PreSpecializedMirDecodeError> {
        let discriminator = specialization_discriminator(key);

        let Some((index, entry)) = self.entry(
            InterfaceSymbolId::new(0),
            ImplementationPayloadKind::PreSpecializedMir,
            discriminator,
        ) else {
            return Ok(None);
        };

        let payload = self.payload(index, entry)?;
        let mir = decode_pre_specialized_mir(&payload, self.limits)?;

        if mir.key() != key {
            return Err(InterfaceValidationError::HashMismatch.into());
        }

        let template = InterfaceExecutableTemplate::new(
            InterfaceSymbolId::new(0),
            MirExecutableTemplateId::ROOT,
            1,
            mir.shared_payload(),
        )
        .ok_or(InterfaceValidationError::Malformed)?;

        let unit = super::decode_executable_template(
            &template,
            owner,
            unit,
            target,
            properties,
            symbols,
            self.limits,
        )?;

        Ok(Some(unit))
    }

    /// Rejects a bundle that does not match the selected interface and dependency graph exactly.
    pub fn validate_interface(
        &self,
        interface: &ValidatedPackageInterface,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        let identity = self.identity();

        if identity.interface() != surface.identity()
            || identity.interface_content_hash() != interface.header().content_hash()
            || identity.language_revision() != interface.header().language_revision()
            || identity.dependencies() != surface.dependencies()
        {
            return Err(InterfaceValidationError::HashMismatch);
        }

        Ok(())
    }

    /// Rejects use under any target, runtime, or panic configuration other than the encoded one.
    pub fn validate_configuration(
        &self,
        configuration: &PackageImplementationConfiguration,
    ) -> Result<(), InterfaceValidationError> {
        if self.identity.configuration() != configuration {
            return Err(InterfaceValidationError::HashMismatch);
        }

        Ok(())
    }

    fn payload(
        &self,
        index: usize,
        entry: &ImplementationDirectoryEntry,
    ) -> Result<Arc<[u8]>, InterfaceValidationError> {
        let cache = self
            .decoded
            .get(index)
            .ok_or(InterfaceValidationError::Malformed)?;

        cache
            .get_or_init(|| decode_entry_payload(&self.bytes, entry, self.limits))
            .clone()
    }

    fn entry(
        &self,
        owner: InterfaceSymbolId,
        kind: ImplementationPayloadKind,
        discriminator: [u8; 32],
    ) -> Option<(usize, &ImplementationDirectoryEntry)> {
        self.directory
            .binary_search_by_key(&(owner, kind as u8, discriminator), |entry| {
                (entry.owner, entry.raw_kind, entry.discriminator)
            })
            .ok()
            .and_then(|index| self.directory.get(index).map(|entry| (index, entry)))
    }
}

pub(super) const fn executable_discriminator(raw: u32) -> [u8; 32] {
    let bytes = raw.to_le_bytes();
    let mut discriminator = [0; 32];

    discriminator[0] = bytes[0];
    discriminator[1] = bytes[1];
    discriminator[2] = bytes[2];
    discriminator[3] = bytes[3];

    discriminator
}

fn implementation_identity(
    interface: &ValidatedPackageInterface,
    surface: &PackageInterfaceSurface,
    semantics: &InterfaceSemantics,
    configuration: PackageImplementationConfiguration,
) -> PackageImplementationIdentity {
    let runtime_requirements = semantics
        .runtime_requirements()
        .iter()
        .map(crate::InterfaceRuntimeRequirement::requirements)
        .cloned();

    PackageImplementationIdentity::new(
        surface.identity().clone(),
        interface.header().content_hash(),
        interface.header().language_revision(),
        surface.dependencies().iter().cloned(),
        configuration,
        runtime_requirements,
    )
}

fn validate_executable_template_families(
    templates: &[InterfaceExecutableTemplate],
) -> Result<(), PackageImplementationArtifactBuildError> {
    invalid_executable_template_family(templates).map_or(Ok(()), |owner| {
        Err(PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(owner))
    })
}

fn validate_encoded_executable_template_families(
    directory: &[ImplementationDirectoryEntry],
) -> Result<(), InterfaceValidationError> {
    let mut entries = directory
        .iter()
        .filter(|entry| entry.kind == Some(ImplementationPayloadKind::ExecutableTemplate))
        .peekable();

    while let Some(first) = entries.next() {
        let owner = first.owner;
        let family_size = first.family_size;

        for expected in 0..family_size {
            let entry = if expected == 0 {
                first
            } else {
                entries.next().ok_or(InterfaceValidationError::Malformed)?
            };

            if entry.owner != owner
                || entry.family_size != family_size
                || entry.discriminator != executable_discriminator(expected)
            {
                return Err(InterfaceValidationError::Malformed);
            }
        }

        if entries.peek().is_some_and(|entry| entry.owner == owner) {
            return Err(InterfaceValidationError::Malformed);
        }
    }

    Ok(())
}

fn validate_executable_owner(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner));
    };

    if !owner_symbol.kind().is_callable()
        && !matches!(
            owner_symbol.kind(),
            bray_symbols::SymbolKind::CallableParameterDefaultProvider
                | bray_symbols::SymbolKind::StructFieldDefaultProvider
                | bray_symbols::SymbolKind::UnionPayloadDefaultProvider
        )
    {
        return Err(PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner));
    }

    Ok(())
}

fn validate_native_boundary_owner(
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
) -> Result<(), PackageImplementationArtifactBuildError> {
    let Some(owner_symbol) = surface.symbols().symbol(owner) else {
        return Err(PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner));
    };

    if owner_symbol.kind() != bray_symbols::SymbolKind::Function {
        return Err(PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner));
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use bray_bound_tree::CheckedTemplateKind;
    use bray_runtime_interface::BinarySymbolName;
    use bray_symbols::{
        ExternalSymbolKey, ForeignCallableDirection, ImportedInterfaceId, InterfaceSymbolId,
        PackageIdentity, SemanticValueStore, SymbolId, SymbolKind,
    };

    use super::{
        ARTIFACT_HASH_OFFSET, DIRECTORY_ENTRY_LENGTH, ImplementationPayloadKind,
        InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBoundary,
        InterfacePreSpecializedMir, PackageImplementationArtifact,
        PackageImplementationArtifactBuildError, PackageImplementationConfiguration,
        PackageImplementationSpecializationKey, encode_artifact,
    };
    use crate::{
        CURRENT_MIR_SCHEMA_REVISION, CURRENT_TEMPLATE_SCHEMA_REVISION,
        ImplementationExternalSymbolIdentity, ImplementationSpecializationArgument,
        ImplementationSpecializationArgumentKind, InterfaceCheckedTemplate,
        InterfaceLanguageRevision, InterfaceValidationError, InterfaceValidationLimits,
        InterfaceValidationPolicy, LoadedInterfaceSurface, PreSpecializedMirDecodeError,
        ValidatedPackageInterface, construct_imported_symbol_skeletons, encode_package_interface,
    };

    #[test]
    fn artifacts_decode_only_the_requested_constant_body() {
        let fixture = artifact_fixture();

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [fixture.body.clone()],
            [],
            [],
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
            .constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
            .unwrap_or_else(|error| panic!("requested body must decode: {error:?}"));

        assert_eq!(body, Some(fixture.body));
    }

    #[test]
    fn malformed_unrequested_payloads_do_not_block_other_body_lookups() {
        let fixture = artifact_fixture();

        let second_owner =
            bray_symbols::InterfaceSymbolId::new(fixture.body.owner().raw().saturating_add(100));

        let second =
            InterfaceConstantCallableBody::new(second_owner, fixture.body.template().clone());

        let identity = super::implementation_identity(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
        );

        let encoded =
            super::encode_artifact(&identity, &[fixture.body.clone(), second], &[], &[], &[])
                .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"));

        let pristine = PackageImplementationArtifact::try_from_bytes(
            Arc::clone(&encoded),
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("test artifact must validate: {error:?}"));

        let entry = pristine
            .directory
            .iter()
            .find(|entry| entry.owner == second_owner)
            .unwrap_or_else(|| panic!("second body must have a directory entry"));

        let mut bytes = encoded.to_vec();

        let payload = bytes
            .get_mut(entry.payload.clone())
            .unwrap_or_else(|| panic!("second body payload must be in bounds"));

        payload[0] ^= 0xff;

        let checksum = super::compute_payload_hash(entry, payload);

        let directory_offset = u64::from_le_bytes(
            bytes[32..40]
                .try_into()
                .unwrap_or_else(|_| panic!("directory offset must be encoded")),
        );

        let directory_offset = usize::try_from(directory_offset)
            .unwrap_or_else(|_| panic!("directory offset must fit the test target"));

        let entry_index = pristine
            .directory
            .iter()
            .position(|candidate| candidate.owner == second_owner)
            .unwrap_or_else(|| panic!("second body entry must remain addressable"));

        let checksum_offset = directory_offset + entry_index * DIRECTORY_ENTRY_LENGTH + 84;

        bytes[checksum_offset..checksum_offset + 32].copy_from_slice(&checksum);

        let artifact_hash = super::compute_artifact_hash(&bytes)
            .unwrap_or_else(|| panic!("mutated artifact must remain hashable"));

        bytes[ARTIFACT_HASH_OFFSET..ARTIFACT_HASH_OFFSET + 32].copy_from_slice(&artifact_hash);

        let artifact = PackageImplementationArtifact::try_from_bytes(
            bytes,
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("directory validation must remain lazy: {error:?}"));

        let first = artifact.constant_callable_body(fixture.body.owner(), fixture.bundle.surface());

        assert_eq!(first.map(|body| body.is_some()), Ok(true));

        assert!(
            artifact
                .constant_callable_body(second_owner, fixture.bundle.surface(),)
                .is_err()
        );
    }

    #[test]
    fn artifacts_reject_duplicate_constant_body_owners() {
        let fixture = artifact_fixture();

        let result = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [fixture.body.clone(), fixture.body.clone()],
            [],
            [],
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

        let template = InterfaceExecutableTemplate::new(
            owner,
            bray_ir::MirExecutableTemplateId::ROOT,
            1,
            [1_u8, 2, 3],
        )
        .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [template.clone()],
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("executable template artifact must validate: {error:?}"));

        assert_eq!(
            artifact.executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT),
            Ok(Some(template))
        );
    }

    #[test]
    fn artifacts_reject_incomplete_executable_template_families() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let root = InterfaceExecutableTemplate::new(
            owner,
            bray_ir::MirExecutableTemplateId::ROOT,
            2,
            [1_u8, 2, 3],
        )
        .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

        let identity = super::implementation_identity(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
        );

        let bytes = encode_artifact(&identity, &[], std::slice::from_ref(&root), &[], &[])
            .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"));

        let result = PackageImplementationArtifact::try_from_bytes(
            bytes,
            InterfaceValidationLimits::default(),
        );

        assert_eq!(result, Err(InterfaceValidationError::Malformed));
    }

    #[test]
    fn artifacts_bound_executable_payloads_by_the_validation_policy() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let template = InterfaceExecutableTemplate::new(
            owner,
            bray_ir::MirExecutableTemplateId::ROOT,
            1,
            vec![1_u8; 1_200],
        )
        .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

        let result = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [template],
            [],
            [],
            InterfaceValidationLimits::default().with_blob_length(1_000),
        );

        assert_eq!(
            result,
            Err(PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::ResourceLimitExceeded {
                    limit: crate::InterfaceLimit::BlobLength,
                    actual: 1_200,
                    maximum: 1_000,
                }
            ))
        );
    }

    #[test]
    fn artifacts_bound_directory_entries_independently_of_interface_sections() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let template = InterfaceExecutableTemplate::new(
            owner,
            bray_ir::MirExecutableTemplateId::ROOT,
            1,
            [1_u8],
        )
        .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

        let accepted = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [template.clone()],
            [],
            [],
            InterfaceValidationLimits::default().with_section_count(0),
        );

        assert!(accepted.is_ok());

        let rejected = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [template],
            [],
            [],
            InterfaceValidationLimits::default().with_implementation_entry_count(1),
        );

        assert_eq!(
            rejected,
            Err(PackageImplementationArtifactBuildError::InvalidArtifact(
                InterfaceValidationError::ResourceLimitExceeded {
                    limit: crate::InterfaceLimit::ImplementationEntryCount,
                    actual: 2,
                    maximum: 1,
                }
            ))
        );
    }

    #[test]
    fn artifacts_load_native_boundaries_by_declaration_owner() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let symbol = BinarySymbolName::try_new("native_operation")
            .unwrap_or_else(|| panic!("test symbol must be nonempty"));

        let boundary =
            InterfaceNativeBoundary::new(owner, ForeignCallableDirection::Import, symbol);

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [],
            [boundary.clone()],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("native boundary artifact must validate: {error:?}"));

        assert_eq!(artifact.native_boundary(owner), Ok(Some(boundary)));
    }

    #[test]
    fn artifacts_publish_complete_inspectable_identity_and_reject_configuration_mismatch() {
        let fixture = artifact_fixture();

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [],
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("identity artifact must validate: {error:?}"));

        let mut expected_runtime_requirements = fixture
            .bundle
            .semantics()
            .runtime_requirements()
            .iter()
            .map(crate::InterfaceRuntimeRequirement::requirements)
            .cloned()
            .collect::<Vec<_>>();

        expected_runtime_requirements.sort_unstable();
        expected_runtime_requirements.dedup();

        assert_eq!(
            artifact.identity().interface(),
            fixture.bundle.surface().identity()
        );

        assert_eq!(
            artifact.identity().dependencies(),
            fixture.bundle.surface().dependencies()
        );

        assert_eq!(
            artifact.identity().runtime_requirements(),
            expected_runtime_requirements
        );

        assert_eq!(
            artifact.identity().configuration(),
            fixture.bundle.implementation_configuration()
        );

        let selected_runtime =
            bray_runtime_interface::RuntimeIdentity::try_new("bray.runtime.test")
                .unwrap_or_else(|| panic!("test runtime identity must be valid"));

        let mismatched = PackageImplementationConfiguration::new(
            fixture
                .bundle
                .implementation_configuration()
                .target_properties()
                .clone(),
            Some(selected_runtime),
            fixture.bundle.implementation_configuration().runtime_abi(),
            fixture
                .bundle
                .implementation_configuration()
                .panic_abi()
                .clone(),
        );

        assert_eq!(
            artifact.validate_configuration(&mismatched),
            Err(InterfaceValidationError::HashMismatch)
        );

        let selected_runtime_artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            mismatched.clone(),
            [],
            [],
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("selected runtime identity must round trip: {error:?}"));

        assert_eq!(
            selected_runtime_artifact.identity().configuration(),
            &mismatched
        );

        assert_eq!(
            selected_runtime_artifact
                .validate_configuration(fixture.bundle.implementation_configuration()),
            Err(InterfaceValidationError::HashMismatch)
        );

        let alternate_target = bray_ir::MirTargetContract::new(
            bray_target::NativeTarget::X86_64WindowsMsvc.profile(),
            fixture.bundle.implementation_configuration().runtime_abi(),
        );

        let alternate_target = PackageImplementationConfiguration::for_mir_target(
            &alternate_target,
            None,
            fixture
                .bundle
                .implementation_configuration()
                .panic_abi()
                .clone(),
        );

        assert_ne!(
            alternate_target.target_properties().machine(),
            artifact
                .identity()
                .configuration()
                .target_properties()
                .machine()
        );

        assert_ne!(
            alternate_target.target_properties().properties(),
            artifact
                .identity()
                .configuration()
                .target_properties()
                .properties()
        );

        assert_eq!(
            artifact.validate_configuration(&alternate_target),
            Err(InterfaceValidationError::HashMismatch)
        );
    }

    #[test]
    fn pre_specialized_mir_uses_the_complete_specialization_identity() {
        let fixture = artifact_fixture();
        let key = specialization_key(&fixture, [1; 32]);

        let mir =
            InterfacePreSpecializedMir::new(key.clone(), CURRENT_MIR_SCHEMA_REVISION, [3_u8, 5, 8])
                .unwrap_or_else(|| panic!("nonempty pre-specialized MIR must be valid"));

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [],
            [],
            [mir.clone()],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("pre-specialized MIR artifact must validate: {error:?}"));

        let entry = artifact
            .directory
            .iter()
            .find(|entry| entry.kind == Some(ImplementationPayloadKind::PreSpecializedMir))
            .unwrap_or_else(|| panic!("pre-specialized MIR must be addressable"));

        assert_eq!(entry.discriminator, key.cache_identity());

        let different_substitution = specialization_key(&fixture, [2; 32]);

        assert_ne!(
            key.cache_identity(),
            different_substitution.cache_identity()
        );

        assert_ne!(entry.discriminator, different_substitution.cache_identity());

        let loaded = LoadedInterfaceSurface::new(
            ImportedInterfaceId::new(0),
            fixture.interface.header().content_hash(),
            fixture.bundle.surface(),
        );

        let skeleton = construct_imported_symbol_skeletons(SymbolId::new(0), [loaded])
            .unwrap_or_else(|error| panic!("test interface symbols must import: {error:?}"));

        let compiler_known = BTreeMap::new();

        let resolver = crate::ImportedInterfaceSymbolResolver::try_new(
            loaded,
            [loaded],
            &skeleton,
            &compiler_known,
        )
        .unwrap_or_else(|error| panic!("test resolver must construct: {error:?}"));

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic store must construct: {error:?}"));

        let properties = fixture
            .bundle
            .semantics()
            .intern(&store, &resolver)
            .unwrap_or_else(|error| panic!("test imported semantics must load: {error:?}"));

        let owner_key = fixture
            .bundle
            .surface()
            .symbols()
            .symbol(generic_callable_owner(&fixture.bundle))
            .map(bray_symbols::ImportedSymbolIdentity::key)
            .unwrap_or_else(|| panic!("test callable identity must be present"));

        let owner = skeleton
            .symbol_by_external_key(owner_key)
            .unwrap_or_else(|| panic!("test callable must import"));

        let target = bray_ir::MirTargetContract::new(
            bray_target::NativeTarget::X86_64LinuxGnu.profile(),
            fixture.bundle.implementation_configuration().runtime_abi(),
        );

        assert_eq!(
            artifact.pre_specialized_mir(
                &key,
                owner,
                bray_ir::MirUnitId::new(0),
                target,
                &properties,
                &resolver,
            ),
            Err(PreSpecializedMirDecodeError::Executable(
                crate::ExecutableTemplateDecodeError::Validation(
                    InterfaceValidationError::Truncated,
                ),
            ))
        );
    }

    #[test]
    fn concurrent_demands_share_one_lazily_decompressed_payload() {
        let fixture = artifact_fixture();
        let owner = generic_callable_owner(&fixture.bundle);

        let template = InterfaceExecutableTemplate::new(
            owner,
            bray_ir::MirExecutableTemplateId::ROOT,
            1,
            vec![0_u8; 4_096],
        )
        .unwrap_or_else(|| panic!("compressed executable payload must be valid"));

        let artifact = PackageImplementationArtifact::try_new(
            &fixture.interface,
            fixture.bundle.surface(),
            fixture.bundle.semantics(),
            fixture.bundle.implementation_configuration().clone(),
            [],
            [template],
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("compressed artifact must validate: {error:?}"));

        let first_artifact = artifact.clone();
        let second_artifact = artifact.clone();

        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                first_artifact
                    .executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT)
                    .unwrap_or_else(|error| panic!("first demand must decode: {error:?}"))
                    .unwrap_or_else(|| panic!("first demand must find the template"))
            });

            let second = scope.spawn(|| {
                second_artifact
                    .executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT)
                    .unwrap_or_else(|error| panic!("second demand must decode: {error:?}"))
                    .unwrap_or_else(|| panic!("second demand must find the template"))
            });

            (
                first
                    .join()
                    .unwrap_or_else(|_| panic!("first demand must complete")),
                second
                    .join()
                    .unwrap_or_else(|_| panic!("second demand must complete")),
            )
        });

        assert!(std::ptr::eq(first.payload(), second.payload()));
    }

    struct ArtifactFixture {
        interface: ValidatedPackageInterface,
        bundle: crate::PackageInterfaceExportBundle,
        body: InterfaceConstantCallableBody,
    }

    fn generic_callable_owner(bundle: &crate::PackageInterfaceExportBundle) -> InterfaceSymbolId {
        bundle
            .semantics()
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

    fn specialization_key(
        fixture: &ArtifactFixture,
        argument: [u8; 32],
    ) -> PackageImplementationSpecializationKey {
        let package = PackageIdentity::try_new("example.specialization")
            .unwrap_or_else(|| panic!("specialization package identity must be valid"));

        PackageImplementationSpecializationKey::new(
            ImplementationExternalSymbolIdentity::new(&ExternalSymbolKey::package(package)),
            [ImplementationSpecializationArgument::new(
                ImplementationSpecializationArgumentKind::Type,
                argument,
            )],
            [],
            fixture.bundle.implementation_configuration().clone(),
            CURRENT_TEMPLATE_SCHEMA_REVISION,
            fixture.bundle.surface().dependencies().iter().cloned(),
        )
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
            .semantics()
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
