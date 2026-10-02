use std::sync::Arc;

use bray_ir::{MirExecutableTemplateId, MirTargetContract, MirUnit, MirUnitId};
use bray_symbols::{AnySymbolId, InterfaceSymbolId};

use crate::implementation::codec::configuration_identity;
use crate::implementation::payload::{
    decode_native_binding, decode_native_boundary, decode_pre_specialized_mir,
    specialization_discriminator,
};
use crate::implementation::{
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBinding,
    InterfaceNativeBoundary, PackageImplementationConfiguration,
    PackageImplementationSpecializationKey, PreSpecializedMirDecodeError,
};
use crate::semantic::decode_template_payload;
use crate::{
    ImportedSemantics, InterfaceContentHash, InterfaceLanguageRevision, InterfaceSymbolResolver,
    InterfaceValidationError, PackageInterfaceSurface, ValidatedPackageInterface,
};

use super::construction::validate_body_owner;
use super::decoding::decode_entry_payload;
use super::{
    ImplementationDirectoryEntry, ImplementationPayloadKind, PackageImplementationArtifact,
    executable_discriminator,
};

impl PackageImplementationArtifact {
    /// Returns the complete package, interface, dependency, target, and runtime identity.
    pub const fn identity(&self) -> &crate::implementation::PackageImplementationIdentity {
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

    /// Returns the metadata commitment, including every stored payload checksum.
    pub const fn artifact_hash(&self) -> &[u8; 32] {
        &self.artifact_hash
    }

    /// Returns cumulative reads and payload authentication work shared by artifact clones.
    pub fn access_statistics(&self) -> super::storage::ImplementationAccessStatistics {
        self.storage.statistics()
    }

    /// Computes the packed metadata commitment without reading or authenticating payloads.
    /// Returns no commitment when the bytes do not contain packed implementation framing.
    pub fn metadata_digest(bytes: &[u8]) -> Option<[u8; 32]> {
        if !bytes.starts_with(&super::MAGIC) {
            return None;
        }

        crate::implementation::hash::compute_artifact_hash(bytes)
    }

    /// Reads the complete encoded artifact for publication or explicit acquisition verification.
    pub fn shared_bytes(&self) -> Result<Arc<[u8]>, InterfaceValidationError> {
        self.storage.read_all(self.limits)
    }

    pub(crate) fn verify_digest(&self, expected: [u8; 32]) -> Result<(), InterfaceValidationError> {
        let identity = self.directory.first().expect("validated artifacts contain an identity");
        let directory_offset = self.storage.len() - self.directory.len() * super::DIRECTORY_ENTRY_LENGTH;
        let actual = self.storage.digest(directory_offset, self.artifact_hash, identity)?;

        if actual != expected {
            return Err(InterfaceValidationError::ArtifactHashMismatch {
                expected: crate::InterfaceArtifactHash::from_bytes(expected),
                actual: crate::InterfaceArtifactHash::from_bytes(actual),
            });
        }

        Ok(())
    }

    /// Authenticates every payload, including otherwise unread implementation.
    pub fn verify_all(&self) -> Result<(), super::native::PackageNativeArtifactError> {
        self.storage.verify_length()?;

        let header = self.storage.read(0..super::HEADER_LENGTH)?;

        let directory_offset =
            self.storage.len() - self.directory.len() * super::DIRECTORY_ENTRY_LENGTH;

        let directory = self.storage.read(directory_offset..self.storage.len())?;

        let actual = crate::implementation::hash::compute_metadata_hash(&header, &directory)
            .expect("validated packed metadata has complete hash framing");

        if actual != self.artifact_hash {
            return Err(InterfaceValidationError::ArtifactHashMismatch {
                expected: crate::InterfaceArtifactHash::from_bytes(self.artifact_hash),
                actual: crate::InterfaceArtifactHash::from_bytes(actual),
            }
            .into());
        }

        for entry in self.directory.iter() {
            self.read_payload(entry)?;
        }

        self.validate_native_inventory()?;

        for binding in self.native_bindings()? {
            self.validate_native_binding(&binding)?;
        }

        Ok(())
    }

    /// Returns the native unit index embedded in this implementation, when present.
    pub fn native_index_bytes(&self) -> Result<Option<Arc<[u8]>>, InterfaceValidationError> {
        self.native_index_bytes_at([0; 32])
    }

    pub(super) fn native_index_bytes_at(
        &self,
        discriminator: [u8; 32],
    ) -> Result<Option<Arc<[u8]>>, InterfaceValidationError> {
        self.entry(
            InterfaceSymbolId::new(0),
            ImplementationPayloadKind::NativeIndex,
            discriminator,
        )
        .map(|(index, entry)| self.payload(index, entry))
        .transpose()
    }

    /// Returns one native unit by its exact content identity.
    pub fn native_unit_bytes(
        &self,
        digest: [u8; 32],
    ) -> Result<Option<Arc<[u8]>>, super::native::PackageNativeArtifactError> {
        let Some((index, entry)) = self.entry(
            InterfaceSymbolId::new(0),
            ImplementationPayloadKind::NativeUnit,
            digest,
        ) else {
            return Ok(None);
        };

        self.payload(index, entry).map(Some).map_err(Into::into)
    }

    /// Resolves a native specialization only when its producer policy matches the request.
    pub fn native_binding(
        &self,
        owner: InterfaceSymbolId,
        key: &PackageImplementationSpecializationKey,
        requested_options: bray_codegen::CodegenOptions,
    ) -> Result<Option<InterfaceNativeBinding>, super::native::PackageNativeArtifactError> {
        let Some((index, entry)) = self.entry(
            owner,
            ImplementationPayloadKind::NativeBinding,
            specialization_discriminator(key),
        ) else {
            return Ok(None);
        };

        let binding = decode_native_binding(owner, &self.payload(index, entry)?, self.limits)?;

        if binding.key() != key {
            return Err(InterfaceValidationError::SpecializationKeyMismatch {
                expected: key.cache_identity(),
                actual: binding.key().cache_identity(),
            }
            .into());
        }

        self.validate_native_binding(&binding)?;

        Ok((binding.producer_options() == requested_options).then_some(binding))
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

        validate_body_owner(surface, owner, &template).map_err(|_| {
            crate::implementation::invalid_value(crate::InterfaceValidationField::Value)
        })?;

        Ok(Some(InterfaceConstantCallableBody::new(owner, template)))
    }

    /// Returns the independently encoded executable template for one declaration.
    pub fn executable_template(
        &self,
        owner: InterfaceSymbolId,
        identity: MirExecutableTemplateId,
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
            .map(|template| template.with_platform_service(entry.platform_service))
            .map(Some)
            .ok_or(crate::implementation::invalid_value(
                crate::InterfaceValidationField::Value,
            ))
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
            return Err(InterfaceValidationError::SpecializationKeyMismatch {
                expected: key.cache_identity(),
                actual: mir.key().cache_identity(),
            }
            .into());
        }

        let template = InterfaceExecutableTemplate::new(
            InterfaceSymbolId::new(0),
            MirExecutableTemplateId::ROOT,
            1,
            mir.shared_payload(),
        )
        .ok_or(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Value,
        ))?;

        let unit = crate::implementation::decode_executable_template(
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

        if identity.interface() != surface.identity() {
            return Err(
                InterfaceValidationError::ImplementationInterfaceIdentityMismatch {
                    expected: Box::new(surface.identity().clone()),
                    actual: Box::new(identity.interface().clone()),
                },
            );
        }

        if identity.interface_content_hash() != interface.header().content_hash() {
            return Err(InterfaceValidationError::ContentHashMismatch {
                expected: interface.header().content_hash(),
                actual: identity.interface_content_hash(),
            });
        }

        if identity.language_revision() != interface.header().language_revision() {
            return Err(InterfaceValidationError::UnsupportedLanguageRevision {
                expected: interface.header().language_revision(),
                actual: identity.language_revision(),
            });
        }

        if identity.dependencies() != surface.dependencies() {
            let index = identity
                .dependencies()
                .iter()
                .zip(surface.dependencies())
                .position(|(actual, expected)| actual != expected)
                .unwrap_or_else(|| {
                    identity
                        .dependencies()
                        .len()
                        .min(surface.dependencies().len())
                });

            return Err(InterfaceValidationError::ImplementationDependencyMismatch {
                index: index as u64,
                expected: surface.dependencies().get(index).cloned().map(Box::new),
                actual: identity.dependencies().get(index).cloned().map(Box::new),
            });
        }

        Ok(())
    }

    /// Rejects use under any target, runtime, or panic configuration other than the encoded one.
    pub fn validate_configuration(
        &self,
        configuration: &PackageImplementationConfiguration,
    ) -> Result<(), InterfaceValidationError> {
        if self.identity.configuration() != configuration {
            return Err(
                InterfaceValidationError::ImplementationConfigurationMismatch {
                    expected: configuration_identity(configuration),
                    actual: configuration_identity(self.identity.configuration()),
                },
            );
        }

        Ok(())
    }

    pub(in crate::implementation) fn payload(
        &self,
        index: usize,
        entry: &ImplementationDirectoryEntry,
    ) -> Result<Arc<[u8]>, InterfaceValidationError> {
        let cache = self
            .decoded
            .get(index)
            .expect("validated implementation directory entry must have a payload cache slot");

        cache
            .get_or_init(|| {
                self.allocation
                    .lock()
                    .expect("implementation allocation budget poisoned")
                    .charge(usize::try_from(entry.decoded_length).unwrap_or(usize::MAX))?;

                self.read_payload(entry)
            })
            .clone()
    }

    fn read_payload(
        &self,
        entry: &ImplementationDirectoryEntry,
    ) -> Result<Arc<[u8]>, InterfaceValidationError> {
        let bytes = self.storage.read(entry.payload.clone())?;
        let decoded = decode_entry_payload(bytes, entry, self.limits)?;

        self.storage.authenticated(
            entry.payload.len(),
            entry.decoded_length,
            entry.encoding == crate::InterfaceSectionEncoding::ZstdFrame,
        );

        if entry.kind == Some(ImplementationPayloadKind::NativeUnit) {
            self.storage.hashed(decoded.len());
        }

        Ok(decoded)
    }

    pub(super) fn entry(
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
