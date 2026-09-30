use std::collections::BTreeSet;

use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeIndexError, NativeUnit, NativeUnitKind,
    NativeUnitSummary, WireError,
};
use bray_symbols::InterfaceSymbolId;

use crate::InterfaceValidationError;
use crate::implementation::identity::CURRENT_TEMPLATE_SCHEMA_REVISION;
use crate::implementation::payload::decode_native_binding;

use super::{ImplementationPayloadKind, PackageImplementationArtifact};

impl PackageImplementationArtifact {
    /// Reads an alternate native route from the authenticated package container.
    pub fn native_variant(
        &self,
        kind: NativeUnitKind,
    ) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        self.native_index(Some(kind))
    }

    /// Returns the primary native index used by source-definition bindings.
    pub fn native_artifact(
        &self,
    ) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        self.native_index(None)
    }

    /// Selects compatible bitcode when requested, then falls back to native objects.
    /// Returns no representation when the package has only incompatible bitcode.
    pub fn native_representation(
        &self,
        bitcode_toolchain: Option<&str>,
    ) -> Result<Option<(NativeUnitKind, NativeArtifactIndex)>, PackageNativeArtifactError> {
        if let Some(revision) = bitcode_toolchain
            && let Some(index) = self.native_variant(NativeUnitKind::Bitcode)?
            && index.bitcode_toolchain() == Some(revision)
        {
            return Ok(Some((NativeUnitKind::Bitcode, index)));
        }

        if let Some(index) = self.native_variant(NativeUnitKind::Object)? {
            return Ok(Some((NativeUnitKind::Object, index)));
        }

        let Some(index) = self.native_artifact()? else {
            return Ok(None);
        };

        if let Some(revision) = index.bitcode_toolchain() {
            return Ok(
                (Some(revision) == bitcode_toolchain).then_some((NativeUnitKind::Bitcode, index))
            );
        }

        // Unstamped bitcode cannot be reused without its LLVM revision.
        if index
            .units()
            .iter()
            .any(|unit| unit.kind() == NativeUnitKind::Bitcode)
        {
            return Ok(None);
        }

        Ok(Some((NativeUnitKind::Object, index)))
    }

    pub(super) fn native_index(
        &self,
        kind: Option<NativeUnitKind>,
    ) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        let discriminator = kind.map_or([0; 32], super::native_index_discriminator);

        self.native_indexes[usize::from(discriminator[0])]
            .get_or_init(|| self.decode_native_index(kind, discriminator))
            .clone()
    }

    fn decode_native_index(
        &self,
        kind: Option<NativeUnitKind>,
        discriminator: [u8; 32],
    ) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        let Some(bytes) = self.native_index_bytes_at(discriminator)? else {
            return Ok(None);
        };

        let target =
            bray_target::NativeTarget::for_identity(self.identity().configuration().target())
                .ok_or(PackageNativeArtifactError::UnsupportedTarget)?;

        let index = NativeArtifactIndex::decode_authenticated(&bytes, target)?;

        for unit in index.units() {
            if kind.is_some_and(|kind| {
                unit.kind() != kind && unit.kind() != NativeUnitKind::OpaqueArchive
            }) {
                return Err(NativeIndexError::InvalidSummary(unit.digest()).into());
            }

            if self
                .entry(
                    InterfaceSymbolId::new(0),
                    ImplementationPayloadKind::NativeUnit,
                    unit.digest().bytes(),
                )
                .is_none()
            {
                return Err(PackageNativeArtifactError::MissingUnit(unit.digest()));
            }
        }

        Ok(Some(index))
    }

    pub(super) fn validate_native_inventory(&self) -> Result<(), PackageNativeArtifactError> {
        let mut seen = BTreeSet::new();
        let mut producer = None;

        for kind in [
            None,
            Some(NativeUnitKind::Object),
            Some(NativeUnitKind::Bitcode),
        ] {
            if let Some(index) = self.native_index(kind)? {
                if let Some(expected) = producer {
                    if index.producer() != expected {
                        return Err(NativeIndexError::WrongProducer {
                            expected,
                            actual: index.producer(),
                        }
                        .into());
                    }
                }

                producer = Some(index.producer());
                seen.extend(index.units().iter().map(|unit| unit.digest().bytes()));
            }
        }

        for entry in self
            .directory
            .iter()
            .filter(|entry| entry.kind == Some(ImplementationPayloadKind::NativeUnit))
        {
            if producer.is_none() {
                return Err(PackageNativeArtifactError::MissingIndex);
            }

            if !seen.contains(&entry.discriminator) {
                return Err(PackageNativeArtifactError::UnindexedUnit(
                    entry.discriminator,
                ));
            }
        }

        Ok(())
    }

    pub(super) fn validate_native_binding(
        &self,
        binding: &crate::InterfaceNativeBinding,
    ) -> Result<(), PackageNativeArtifactError> {
        let index = self
            .native_artifact()?
            .ok_or(PackageNativeArtifactError::MissingIndex)?;

        let unit = index
            .units()
            .binary_search_by_key(
                &NativeContentDigest::new(binding.unit()),
                NativeUnit::digest,
            )
            .ok()
            .and_then(|position| index.units().get(position))
            .ok_or(PackageNativeArtifactError::InvalidBinding(binding.owner()))?;

        if binding.key().template_schema_revision() != CURRENT_TEMPLATE_SCHEMA_REVISION
            || binding.key().configuration() != self.identity().configuration()
            || binding.key().dependencies() != self.identity().dependencies()
            || index
                .target()
                .codegen_symbol_name(binding.symbol())
                .is_none_or(str::is_empty)
            || !symbol_in_unit(unit, binding.symbol())
        {
            return Err(PackageNativeArtifactError::InvalidBinding(binding.owner()));
        }

        Ok(())
    }

    /// Decodes all source-definition bindings for native import validation and later selection.
    pub fn native_bindings(
        &self,
    ) -> Result<Vec<crate::InterfaceNativeBinding>, PackageNativeArtifactError> {
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
        NativeUnitSummary::Opaque { .. } => unit.kind() == NativeUnitKind::OpaqueArchive,
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
            Self::MissingUnit(digest) => {
                unit = Some(digest.bytes());

                Cause::MissingUnit
            }
            Self::UnindexedUnit(digest) => {
                unit = Some(digest);

                Cause::UnindexedUnit
            }
            Self::InvalidBinding(symbol) => {
                owner = Some(symbol.raw());

                Cause::InvalidBinding
            }
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
                NativeIndexError::IndexDigestMismatch {
                    expected: declared,
                    actual: computed,
                } => {
                    expected = Some(declared.bytes());
                    actual = Some(computed.bytes());

                    Cause::IndexDigestMismatch
                }
                NativeIndexError::PayloadDigestMismatch {
                    expected: declared,
                    actual: computed,
                } => {
                    expected = Some(declared.bytes());
                    actual = Some(computed.bytes());

                    Cause::PayloadDigestMismatch
                }
                NativeIndexError::WrongTarget { expected, actual } => {
                    expected_target = Some(expected.as_str().to_owned());
                    actual_target = Some(actual.as_str().to_owned());

                    Cause::WrongTarget
                }
                NativeIndexError::WrongProducer {
                    expected: required,
                    actual: supplied,
                } => {
                    expected = Some(required.bytes());
                    actual = Some(supplied.bytes());

                    Cause::WrongProducer
                }
                NativeIndexError::Read {
                    path: failed_path,
                    kind,
                } => {
                    path = Some(failed_path);
                    io_error_kind = Some(bray_diagnostics::DiagnosticIoErrorKind::from(kind));

                    Cause::ReadFailure
                }
                NativeIndexError::DuplicateUnit(digest) => {
                    unit = Some(digest.bytes());

                    Cause::DuplicateUnit
                }
                NativeIndexError::InvalidSummary(digest) => {
                    unit = Some(digest.bytes());

                    Cause::InvalidSummary
                }
                NativeIndexError::DuplicateDefinition(digest) => {
                    unit = Some(digest.bytes());

                    Cause::DuplicateDefinition
                }
                NativeIndexError::InvalidAssociation(digest) => {
                    unit = Some(digest.bytes());

                    Cause::InvalidAssociation
                }
                NativeIndexError::NoncanonicalSummary(digest) => {
                    unit = Some(digest.bytes());

                    Cause::NoncanonicalSummary
                }
                NativeIndexError::MissingCoRetentionMember(digest) => {
                    unit = Some(digest.bytes());

                    Cause::MissingCoRetentionMember
                }
                NativeIndexError::DuplicateCoRetentionGroup => Cause::DuplicateCoRetentionGroup,
                NativeIndexError::InvalidCoRetentionGroup => Cause::InvalidCoRetentionGroup,
            },
        };

        bray_diagnostics::DiagnosticInterfaceValidationFailure::NativeArtifact {
            cause,
            unit,
            expected,
            actual,
            owner,
            expected_target,
            actual_target,
            path,
            io_error_kind,
        }
    }
}
