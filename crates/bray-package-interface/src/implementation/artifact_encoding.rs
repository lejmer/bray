use std::sync::Arc;

use bray_symbols::InterfaceSymbolId;

use crate::semantic::encode_template_payload;
use crate::wire::WireEncoder;
use crate::{InterfaceLanguageRevision, InterfaceValidationError};

use super::artifact::{
    ARTIFACT_HASH_OFFSET, BYTE_ORDER_MARKER, CONTENT_HASH_OFFSET, DIRECTORY_ENTRY_LENGTH,
    HEADER_LENGTH, ImplementationDirectoryEntry, ImplementationPayloadKind, MAGIC, REQUIRED_FLAGS,
    executable_discriminator,
};
use super::codec::encode_identity;
use super::hash::{
    compute_artifact_hash, compute_content_hash, compute_payload_content_hash, compute_payload_hash,
};
use super::payload::{
    encode_native_binding, encode_native_boundary, encode_pre_specialized_mir,
    specialization_discriminator,
};
use super::{
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBinding, InterfaceNativeBoundary,
    InterfacePreSpecializedMir, PackageImplementationArtifactBuildError,
    PackageImplementationIdentity,
};

pub(super) fn encode_artifact(
    identity: &PackageImplementationIdentity,
    bodies: &[InterfaceConstantCallableBody],
    templates: &[InterfaceExecutableTemplate],
    boundaries: &[InterfaceNativeBoundary],
    pre_specialized_mir: &[InterfacePreSpecializedMir],
    native_index: Option<&[u8]>,
    native_units: &[([u8; 32], Arc<[u8]>)],
    native_bindings: &[InterfaceNativeBinding],
) -> Result<Arc<[u8]>, PackageImplementationArtifactBuildError> {
    let mut payloads = vec![EncodedImplementationPayload::new(
        InterfaceSymbolId::new(0),
        ImplementationPayloadKind::Identity,
        [0; 32],
        0,
        encode_identity(identity),
    )];

    payloads.extend(bodies.iter().map(|body| {
        EncodedImplementationPayload::new(
            body.owner(),
            ImplementationPayloadKind::ConstantCallableBody,
            [0; 32],
            0,
            encode_template_payload(body.template()),
        )
    }));

    payloads.extend(templates.iter().map(|template| {
        EncodedImplementationPayload::new(
            template.owner(),
            ImplementationPayloadKind::ExecutableTemplate,
            executable_discriminator(template.identity().raw()),
            template.family_size(),
            template.payload().to_vec(),
        )
        .with_platform_service(template.platform_service())
    }));

    payloads.extend(boundaries.iter().map(|boundary| {
        EncodedImplementationPayload::new(
            boundary.owner(),
            ImplementationPayloadKind::NativeBoundary,
            [0; 32],
            0,
            encode_native_boundary(boundary),
        )
    }));

    payloads.extend(pre_specialized_mir.iter().map(|mir| {
        EncodedImplementationPayload::new(
            InterfaceSymbolId::new(0),
            ImplementationPayloadKind::PreSpecializedMir,
            specialization_discriminator(mir.key()),
            0,
            encode_pre_specialized_mir(mir),
        )
    }));

    if let Some(index) = native_index {
        payloads.push(EncodedImplementationPayload::new(
            InterfaceSymbolId::new(0),
            ImplementationPayloadKind::NativeIndex,
            [0; 32],
            0,
            index.to_vec(),
        ));
    }

    payloads.extend(native_units.iter().map(|(digest, bytes)| {
        EncodedImplementationPayload::new(
            InterfaceSymbolId::new(0),
            ImplementationPayloadKind::NativeUnit,
            *digest,
            0,
            bytes.to_vec(),
        )
    }));

    payloads.extend(native_bindings.iter().map(|binding| {
        EncodedImplementationPayload::new(
            binding.owner(),
            ImplementationPayloadKind::NativeBinding,
            specialization_discriminator(binding.key()),
            0,
            encode_native_binding(binding),
        )
    }));

    payloads.sort_by_key(EncodedImplementationPayload::directory_key);

    let payloads = payloads
        .into_iter()
        .map(StoredImplementationPayload::try_from_decoded)
        .collect::<Result<Vec<_>, _>>()
        .map_err(PackageImplementationArtifactBuildError::InvalidArtifact)?;

    let payload_length = payloads.iter().try_fold(0_usize, |total, payload| {
        total.checked_add(payload.encoded.len())
    });

    let directory_length = payloads.len().checked_mul(DIRECTORY_ENTRY_LENGTH).ok_or(
        PackageImplementationArtifactBuildError::InvalidArtifact(numeric_overflow(
            crate::InterfaceValidationField::DirectoryLength,
            payloads.len() as u64,
            crate::InterfaceIntegerTarget::Usize,
        )),
    )?;

    let directory_offset = HEADER_LENGTH
        .checked_add(payload_length.ok_or(
            PackageImplementationArtifactBuildError::InvalidArtifact(range_overflow(
                HEADER_LENGTH as u64,
                u64::MAX,
            )),
        )?)
        .ok_or(PackageImplementationArtifactBuildError::InvalidArtifact(
            range_overflow(HEADER_LENGTH as u64, u64::MAX),
        ))?;

    let file_length = directory_offset.checked_add(directory_length).ok_or(
        PackageImplementationArtifactBuildError::InvalidArtifact(range_overflow(
            directory_offset as u64,
            directory_length as u64,
        )),
    )?;

    let mut encoder = WireEncoder::new();

    encode_header(
        &mut encoder,
        identity.language_revision(),
        file_length,
        directory_offset,
        directory_length,
    );

    let mut entries = Vec::with_capacity(payloads.len());
    let mut offset = HEADER_LENGTH;

    for (index, payload) in payloads.iter().enumerate() {
        let entry = payload.directory_entry(index as u64, offset)?;

        encoder.write_bytes(&payload.encoded);
        offset = entry.payload.end;
        entries.push(entry);
    }

    for entry in &entries {
        encode_directory_entry(&mut encoder, entry);
    }

    let mut bytes = encoder.into_bytes();
    let content_hash = compute_content_hash(identity.language_revision(), &entries);

    bytes[CONTENT_HASH_OFFSET..CONTENT_HASH_OFFSET + 32].copy_from_slice(&content_hash);

    let artifact_hash = compute_artifact_hash(&bytes).ok_or(
        PackageImplementationArtifactBuildError::InvalidArtifact(
            InterfaceValidationError::DigestUnavailable {
                context: crate::InterfaceValidationContext::Artifact,
                field: crate::InterfaceValidationField::ArtifactHash,
            },
        ),
    )?;

    bytes[ARTIFACT_HASH_OFFSET..ARTIFACT_HASH_OFFSET + 32].copy_from_slice(&artifact_hash);

    Ok(bytes.into())
}

fn encode_header(
    encoder: &mut WireEncoder,
    language_revision: InterfaceLanguageRevision,
    file_length: usize,
    directory_offset: usize,
    directory_length: usize,
) {
    encoder.write_bytes(&MAGIC);
    encoder.write_u16(crate::CURRENT_FORMAT_REVISION.raw());
    encoder.write_u16(language_revision.raw());
    encoder.write_u32(BYTE_ORDER_MARKER);
    encoder.write_u64(REQUIRED_FLAGS);
    encoder.write_u64(u64::try_from(file_length).unwrap_or(u64::MAX));
    encoder.write_u64(u64::try_from(directory_offset).unwrap_or(u64::MAX));
    encoder.write_u64(u64::try_from(directory_length).unwrap_or(u64::MAX));
    encoder.write_bytes(&[0; 32]);
    encoder.write_bytes(&[0; 32]);
}

fn encode_directory_entry(encoder: &mut WireEncoder, entry: &ImplementationDirectoryEntry) {
    encoder.write_u32(entry.owner.raw());
    encoder.write_u8(entry.raw_kind);
    encoder.write_u8(entry.compatibility.wire_value());
    encoder.write_u8(entry.encoding.wire_value());
    encoder.write_u8(0);
    encoder.write_u16(crate::InterfaceSectionRevision::CURRENT.raw());
    encoder.write_u16(0);
    encoder.write_bytes(&entry.discriminator);
    encoder.write_u32(entry.family_size);
    encoder.write_u32(entry.platform_service.map_or(0, |role| role.id()));
    encoder.write_u64(u64::try_from(entry.payload.start).unwrap_or(u64::MAX));
    encoder.write_u64(u64::try_from(entry.payload.len()).unwrap_or(u64::MAX));
    encoder.write_u64(entry.decoded_length);
    encoder.write_u64(entry.record_count);
    encoder.write_bytes(&entry.checksum);
    encoder.write_bytes(&entry.content_hash);
}

struct EncodedImplementationPayload {
    owner: InterfaceSymbolId,
    kind: ImplementationPayloadKind,
    discriminator: [u8; 32],
    family_size: u32,
    platform_service: Option<bray_runtime_interface::PlatformServiceRole>,
    decoded: Vec<u8>,
}

impl EncodedImplementationPayload {
    const fn new(
        owner: InterfaceSymbolId,
        kind: ImplementationPayloadKind,
        discriminator: [u8; 32],
        family_size: u32,
        decoded: Vec<u8>,
    ) -> Self {
        Self {
            owner,
            kind,
            discriminator,
            family_size,
            platform_service: None,
            decoded,
        }
    }

    const fn with_platform_service(
        mut self,
        role: Option<bray_runtime_interface::PlatformServiceRole>,
    ) -> Self {
        self.platform_service = role;

        self
    }

    const fn directory_key(&self) -> (InterfaceSymbolId, u8, [u8; 32]) {
        (self.owner, self.kind as u8, self.discriminator)
    }
}

struct StoredImplementationPayload {
    owner: InterfaceSymbolId,
    kind: ImplementationPayloadKind,
    discriminator: [u8; 32],
    family_size: u32,
    platform_service: Option<bray_runtime_interface::PlatformServiceRole>,
    encoding: crate::InterfaceSectionEncoding,
    decoded_length: u64,
    content_hash: [u8; 32],
    encoded: Vec<u8>,
}

impl StoredImplementationPayload {
    fn try_from_decoded(
        payload: EncodedImplementationPayload,
    ) -> Result<Self, InterfaceValidationError> {
        let decoded_length = u64::try_from(payload.decoded.len()).map_err(|_| {
            numeric_overflow(
                crate::InterfaceValidationField::DecodedLength,
                payload.decoded.len() as u64,
                crate::InterfaceIntegerTarget::U64,
            )
        })?;

        let content_hash = compute_payload_content_hash(
            payload.owner,
            payload.kind as u8,
            payload.discriminator,
            &payload.decoded,
        );

        let (encoding, encoded) = crate::encoding::encode_section(
            crate::InterfaceValidationContext::Artifact,
            &payload.decoded,
        )?;

        Ok(Self {
            owner: payload.owner,
            kind: payload.kind,
            discriminator: payload.discriminator,
            family_size: payload.family_size,
            platform_service: payload.platform_service,
            encoding,
            decoded_length,
            content_hash,
            encoded,
        })
    }

    fn directory_entry(
        &self,
        index: u64,
        offset: usize,
    ) -> Result<ImplementationDirectoryEntry, PackageImplementationArtifactBuildError> {
        let end = offset.checked_add(self.encoded.len()).ok_or(
            PackageImplementationArtifactBuildError::InvalidArtifact(range_overflow(
                offset as u64,
                self.encoded.len() as u64,
            )),
        )?;

        let mut entry = ImplementationDirectoryEntry {
            index,
            owner: self.owner,
            raw_kind: self.kind as u8,
            kind: Some(self.kind),
            compatibility: crate::InterfaceSectionCompatibility::Required,
            encoding: self.encoding,
            discriminator: self.discriminator,
            family_size: self.family_size,
            platform_service: self.platform_service,
            decoded_length: self.decoded_length,
            record_count: 1,
            checksum: [0; 32],
            content_hash: self.content_hash,
            payload: offset..end,
        };

        entry.checksum = compute_payload_hash(&entry, &self.encoded);

        Ok(entry)
    }
}

const fn range_overflow(offset: u64, length: u64) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: crate::InterfaceValidationContext::Artifact,
        cause: crate::InterfaceMalformedCause::RangeOverflow { offset, length },
    }
}

const fn numeric_overflow(
    field: crate::InterfaceValidationField,
    value: u64,
    target: crate::InterfaceIntegerTarget,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: crate::InterfaceValidationContext::Artifact,
        cause: crate::InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target,
        },
    }
}
