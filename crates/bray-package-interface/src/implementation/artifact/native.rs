use std::collections::{BTreeMap, BTreeSet};

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
        Ok(self
            .native_indexes()?
            .get(&super::native_index_discriminator(kind))
            .cloned())
    }

    /// Returns the primary native index used by source-definition bindings.
    pub fn native_artifact(
        &self,
    ) -> Result<Option<NativeArtifactIndex>, PackageNativeArtifactError> {
        Ok(self.native_indexes()?.get(&[0; 32]).cloned())
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

    fn native_indexes(
        &self,
    ) -> Result<&BTreeMap<[u8; 32], NativeArtifactIndex>, PackageNativeArtifactError> {
        if let Some(indexes) = self.native_indexes.get() {
            return Ok(indexes);
        }

        let indexes = self.decode_native_indexes()?;
        let _ = self.native_indexes.set(indexes);

        Ok(self
            .native_indexes
            .get()
            .expect("validated native indexes must be cached"))
    }

    fn decode_native_indexes(
        &self,
    ) -> Result<BTreeMap<[u8; 32], NativeArtifactIndex>, PackageNativeArtifactError> {
        let embedded = self
            .directory
            .iter()
            .filter(|entry| entry.kind == Some(ImplementationPayloadKind::NativeUnit))
            .map(|entry| entry.discriminator)
            .collect::<BTreeSet<_>>();

        let mut indexes = BTreeMap::new();
        let mut seen = BTreeSet::new();

        let target =
            bray_target::NativeTarget::for_identity(self.identity().configuration().target());

        for kind in [
            None,
            Some(NativeUnitKind::Object),
            Some(NativeUnitKind::Bitcode),
        ] {
            let discriminator = kind.map_or([0; 32], super::native_index_discriminator);

            let Some(bytes) = self.native_index_bytes_at(discriminator)? else {
                continue;
            };

            let target = target.ok_or(PackageNativeArtifactError::UnsupportedTarget)?;

            let digest = NativeContentDigest::new(
                bray_base::sha256_reader(bytes.as_ref())
                    .expect("reading in-memory native index bytes cannot fail"),
            );

            let index = NativeArtifactIndex::decode(&bytes, digest, target)?;

            if let Some(previous) = indexes.values().next() {
                let previous: &NativeArtifactIndex = previous;

                if index.producer() != previous.producer() {
                    return Err(NativeIndexError::WrongProducer {
                        expected: previous.producer(),
                        actual: index.producer(),
                    }
                    .into());
                }
            }

            for unit in index.units() {
                if kind.is_some_and(|kind| {
                    unit.kind() != kind && unit.kind() != NativeUnitKind::OpaqueArchive
                }) {
                    return Err(PackageNativeArtifactError::Index(
                        NativeIndexError::InvalidSummary(unit.digest()),
                    ));
                }

                if !embedded.contains(&unit.digest().bytes()) {
                    return Err(PackageNativeArtifactError::MissingUnit(unit.digest()));
                }

                seen.insert(unit.digest().bytes());
            }

            indexes.insert(discriminator, index);
        }

        if indexes.is_empty() && !embedded.is_empty() {
            return Err(PackageNativeArtifactError::MissingIndex);
        }

        if let Some(unindexed) = embedded.difference(&seen).next() {
            return Err(PackageNativeArtifactError::UnindexedUnit(*unindexed));
        }

        for binding in self.native_bindings()? {
            let index = indexes
                .get(&[0; 32])
                .ok_or(PackageNativeArtifactError::MissingIndex)?;

            let Some(unit) = index
                .units()
                .binary_search_by_key(
                    &NativeContentDigest::new(binding.unit()),
                    NativeUnit::digest,
                )
                .ok()
                .and_then(|position| index.units().get(position))
            else {
                return Err(PackageNativeArtifactError::InvalidBinding(binding.owner()));
            };

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
        }

        Ok(indexes)
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
