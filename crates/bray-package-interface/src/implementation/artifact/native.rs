use std::collections::BTreeSet;

use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeIndexError, NativeUnit, NativeUnitKind, NativeUnitSummary,
    NativeUnitResolver, WireError,
};
use bray_symbols::InterfaceSymbolId;

use crate::implementation::identity::CURRENT_TEMPLATE_SCHEMA_REVISION;
use crate::implementation::payload::decode_native_binding;
use crate::InterfaceValidationError;

use super::{ImplementationPayloadKind, PackageImplementationArtifact};

impl PackageImplementationArtifact {
    /// Reads an alternate native route from the same authenticated package container.
    pub fn native_variant(
        &self,
        kind: NativeUnitKind,
    ) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        let Some(bytes) = self.native_index_bytes_at(super::native_index_discriminator(kind))? else {
            return Ok(None);
        };

        let target = bray_target::NativeTarget::for_identity(
            self.identity().configuration().target(),
        ).ok_or(PackageNativeArtifactError::UnsupportedTarget)?;

        let digest = NativeContentDigest::new(
            bray_base::sha256_reader(bytes.as_ref())
                .expect("reading in-memory native index bytes cannot fail"),
        );

        let index = NativeArtifactIndex::decode(&bytes, digest, target)?;

        let embedded = self.directory.iter()
            .filter(|entry| entry.kind == Some(ImplementationPayloadKind::NativeUnit))
            .map(|entry| entry.discriminator)
            .collect::<BTreeSet<_>>();

        for unit in index.units() {
            if unit.kind() != kind && unit.kind() != NativeUnitKind::OpaqueArchive {
                return Err(PackageNativeArtifactError::Index(
                    NativeIndexError::InvalidSummary(unit.digest()),
                ));
            }

            if unit.kind() != NativeUnitKind::OpaqueArchive
                && !embedded.contains(&unit.digest().bytes())
            {
                return Err(PackageNativeArtifactError::MissingUnit(unit.digest()));
            }
        }

        Ok(Some(index))
    }

    /// Reuses the validated provider index and demand closure for this artifact identity.
    pub fn native_resolver(&self) -> Result<Option<&NativeUnitResolver>, PackageNativeArtifactError> {
        if let Some(cached) = self.native_resolver.get() {
            return Ok(cached.as_ref());
        }

        let resolver = self.native_artifact()?.map(NativeUnitResolver::new);
        let _ = self.native_resolver.set(resolver);

        Ok(self.native_resolver.get()
            .expect("successful native resolver construction must be cached")
            .as_ref())
    }

    /// Validates the optional native index and its embedded unit directory.
    pub fn native_artifact(&self) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        let Some(bytes) = self.native_index_bytes()? else {
            if self.directory.iter().any(|entry| {
                matches!(entry.kind, Some(ImplementationPayloadKind::NativeUnit | ImplementationPayloadKind::NativeBinding))
            }) {
                return Err(PackageNativeArtifactError::MissingIndex);
            }

            return Ok(None);
        };

        let target = bray_target::NativeTarget::for_identity(
            self.identity().configuration().target(),
        )
        .ok_or(PackageNativeArtifactError::UnsupportedTarget)?;

        let digest = NativeContentDigest::new(
            bray_base::sha256_reader(bytes.as_ref())
                .expect("reading in-memory native index bytes cannot fail"),
        );

        let index = NativeArtifactIndex::decode(&bytes, digest, target)?;

        let seen = index.units().iter()
            .map(|unit| unit.digest().bytes())
            .collect::<BTreeSet<_>>();

        let embedded = self.directory.iter()
            .filter(|entry| entry.kind == Some(ImplementationPayloadKind::NativeUnit))
            .map(|entry| entry.discriminator)
            .collect::<BTreeSet<_>>();

        for unit in index.units() {
            if !embedded.contains(&unit.digest().bytes()) {
                return Err(PackageNativeArtifactError::MissingUnit(unit.digest()));
            }
        }

        for entry in self.directory.iter() {
            if entry.kind == Some(ImplementationPayloadKind::NativeUnit)
                && !seen.contains(&entry.discriminator)
            {
                return Err(PackageNativeArtifactError::UnindexedUnit(entry.discriminator));
            }
        }

        for binding in self.native_bindings()? {
            let Some(unit) = index.units()
                .binary_search_by_key(&NativeContentDigest::new(binding.unit()), NativeUnit::digest)
                .ok()
                .and_then(|position| index.units().get(position)) else {
                return Err(PackageNativeArtifactError::InvalidBinding(binding.owner()));
            };

            if binding.key().template_schema_revision() != CURRENT_TEMPLATE_SCHEMA_REVISION
                || binding.key().configuration() != self.identity().configuration()
                || binding.key().dependencies() != self.identity().dependencies()
                || !symbol_in_unit(unit, binding.symbol())
            {
                return Err(PackageNativeArtifactError::InvalidBinding(binding.owner()));
            }
        }

        Ok(Some(index))
    }

    /// Decodes all source-definition bindings for native import validation and later selection.
    pub fn native_bindings(&self) -> Result<Vec<crate::InterfaceNativeBinding>, PackageNativeArtifactError> {
        let mut bindings = Vec::new();

        for (index, entry) in self.directory.iter().enumerate() {
            if entry.kind != Some(ImplementationPayloadKind::NativeBinding) {
                continue;
            }

            let payload = self.payload(index, entry)?;
            let binding = decode_native_binding(entry.owner, &payload, self.limits)?;

            if binding.key().cache_identity() != entry.discriminator {
                return Err(PackageNativeArtifactError::InvalidBinding(entry.owner));
            }

            bindings.push(binding);
        }

        Ok(bindings)
    }
}

fn symbol_in_unit(unit: &NativeUnit, symbol: &str) -> bool {
    match unit.summary() {
        NativeUnitSummary::Exact { definitions, .. } => definitions
            .iter()
            .any(|definition| definition.symbol().identity().name() == Some(symbol)),
        NativeUnitSummary::Opaque => false,
    }
}

/// Exact corruption or mismatch in embedded native package metadata.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageNativeArtifactError {
    /// The enclosing package implementation has invalid bytes.
    Validation(InterfaceValidationError),
    /// The neutral native index is invalid.
    Index(NativeIndexError),
    /// The package target has no native artifact representation.
    UnsupportedTarget,
    /// Native payloads are present without their index.
    MissingIndex,
    /// An indexed physical unit is absent.
    MissingUnit(NativeContentDigest),
    /// A physical unit is not named by the index.
    UnindexedUnit([u8; 32]),
    /// A source binding does not match the indexed unit.
    InvalidBinding(InterfaceSymbolId),
}

impl From<InterfaceValidationError> for PackageNativeArtifactError {
    fn from(error: InterfaceValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<NativeIndexError> for PackageNativeArtifactError {
    fn from(error: NativeIndexError) -> Self {
        Self::Index(error)
    }
}

impl PackageNativeArtifactError {
    /// Converts the exact native import failure to a locale-neutral validation cause.
    pub fn into_diagnostic_failure(self) -> bray_diagnostics::DiagnosticInterfaceValidationFailure {
        use bray_diagnostics::DiagnosticNativeArtifactCause as Cause;

        let mut unit = None;
        let mut expected = None;
        let mut actual = None;
        let mut owner = None;
        let mut expected_target = None;
        let mut actual_target = None;
        let mut path = None;
        let mut io_error_kind = None;

        let cause = match self {
            Self::Validation(error) => return error.into_diagnostic_failure(),
            Self::UnsupportedTarget => Cause::UnsupportedTarget,
            Self::MissingIndex => Cause::MissingIndex,
            Self::MissingUnit(digest) => { unit = Some(digest.bytes()); Cause::MissingUnit },
            Self::UnindexedUnit(digest) => { unit = Some(digest); Cause::UnindexedUnit },
            Self::InvalidBinding(symbol) => { owner = Some(symbol.raw()); Cause::InvalidBinding },
            Self::Index(error) => match error {
                NativeIndexError::SizeLimitExceeded => Cause::IndexSizeLimitExceeded,
                NativeIndexError::Malformed => Cause::IndexMalformed,
                NativeIndexError::Wire(wire) => match wire {
                    WireError::UnsupportedSchema => Cause::IndexUnsupportedSchema,
                    WireError::InvalidTarget => Cause::IndexInvalidTarget,
                    WireError::InvalidDigest => Cause::IndexInvalidDigest,
                    WireError::InvalidSymbol => Cause::IndexInvalidSymbol,
                    WireError::InvalidLink => Cause::IndexInvalidLink,
                },
                NativeIndexError::IndexDigestMismatch { expected: declared, actual: computed } => {
                    expected = Some(declared.bytes()); actual = Some(computed.bytes());

                    Cause::IndexDigestMismatch
                }
                NativeIndexError::PayloadDigestMismatch { expected: declared, actual: computed } => {
                    expected = Some(declared.bytes()); actual = Some(computed.bytes());

                    Cause::PayloadDigestMismatch
                }
                NativeIndexError::WrongTarget { expected, actual } => {
                    expected_target = Some(expected.as_str().to_owned());
                    actual_target = Some(actual.as_str().to_owned());

                    Cause::WrongTarget
                },
                NativeIndexError::WrongProducer { expected: required, actual: supplied } => {
                    expected = Some(required.bytes()); actual = Some(supplied.bytes());

                    Cause::WrongProducer
                }
                NativeIndexError::Read { path: failed_path, kind } => {
                    path = Some(failed_path);
                    io_error_kind = Some(bray_diagnostics::DiagnosticIoErrorKind::from(kind));

                    Cause::ReadFailure
                },
                NativeIndexError::DuplicateUnit(digest) => { unit = Some(digest.bytes()); Cause::DuplicateUnit },
                NativeIndexError::InvalidSummary(digest) => { unit = Some(digest.bytes()); Cause::InvalidSummary },
                NativeIndexError::DuplicateDefinition(digest) => { unit = Some(digest.bytes()); Cause::DuplicateDefinition },
                NativeIndexError::InvalidAssociation(digest) => { unit = Some(digest.bytes()); Cause::InvalidAssociation },
                NativeIndexError::InvalidLinkOption(digest) => { unit = Some(digest.bytes()); Cause::InvalidLinkOption },
                NativeIndexError::NoncanonicalSummary(digest) => { unit = Some(digest.bytes()); Cause::NoncanonicalSummary },
                NativeIndexError::MissingCoRetentionMember(digest) => { unit = Some(digest.bytes()); Cause::MissingCoRetentionMember },
                NativeIndexError::DuplicateCoRetentionGroup => Cause::DuplicateCoRetentionGroup,
                NativeIndexError::InvalidCoRetentionGroup => Cause::InvalidCoRetentionGroup,
            },
        };

        bray_diagnostics::DiagnosticInterfaceValidationFailure::NativeArtifact {
            cause, unit, expected, actual, owner,
            expected_target, actual_target, path, io_error_kind,
        }
    }
}
