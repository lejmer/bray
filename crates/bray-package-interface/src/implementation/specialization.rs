use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bray_base::{StableDigestHasher, shared_slice, sorted_unique_shared_slice};
use bray_symbols::ExternalSymbolKey;

use crate::InterfaceDependency;

use super::{
    ImplementationTemplateSchemaRevision, PackageImplementationConfiguration,
};

/// Exact MIR schema revision supported by optional pre-specialized payloads.
pub const CURRENT_MIR_SCHEMA_REVISION: ImplementationMirSchemaRevision =
    ImplementationMirSchemaRevision::new(1);

/// Exact schema used to interpret a pre-specialized MIR payload.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationMirSchemaRevision(u16);

impl ImplementationMirSchemaRevision {
    /// Creates a MIR schema revision from its stable wire value.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the stable wire value.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Stable structural identity of one external declaration key.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationExternalSymbolIdentity([u8; 32]);

impl ImplementationExternalSymbolIdentity {
    /// Derives a stable identity from a complete external symbol key.
    pub fn new(key: &ExternalSymbolKey) -> Self {
        let mut digest = StableDigestHasher::new();

        digest.write(b"bray.package-implementation.external-symbol.v1");
        key.hash(&mut digest);

        Self(digest.finalize())
    }

    /// Creates an identity from canonical digest bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Semantic category of one concrete generic specialization argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationSpecializationArgumentKind {
    /// A fully resolved semantic type.
    Type,
    /// A fully evaluated constant value.
    Constant,
}

/// Stable structural identity of one concrete generic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSpecializationArgument {
    kind: ImplementationSpecializationArgumentKind,
    identity: [u8; 32],
}

impl ImplementationSpecializationArgument {
    /// Creates one concrete argument identity.
    pub const fn new(
        kind: ImplementationSpecializationArgumentKind,
        identity: [u8; 32],
    ) -> Self {
        Self { kind, identity }
    }

    /// Returns the semantic argument category.
    pub const fn kind(self) -> ImplementationSpecializationArgumentKind {
        self.kind
    }

    /// Returns the canonical structural identity.
    pub const fn identity(self) -> [u8; 32] {
        self.identity
    }
}

/// Exact selected implementation witness in one specialization key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSpecializationWitness {
    definition: ImplementationExternalSymbolIdentity,
    substitution: Arc<[ImplementationSpecializationArgument]>,
}

impl ImplementationSpecializationWitness {
    /// Creates one exact selected witness.
    pub fn new(
        definition: ImplementationExternalSymbolIdentity,
        substitution: impl IntoIterator<Item = ImplementationSpecializationArgument>,
    ) -> Self {
        Self {
            definition,
            substitution: shared_slice(substitution),
        }
    }

    /// Returns the witness implementation declaration.
    pub const fn definition(&self) -> ImplementationExternalSymbolIdentity {
        self.definition
    }

    /// Returns the witness's ordered concrete substitution.
    pub fn substitution(&self) -> &[ImplementationSpecializationArgument] {
        &self.substitution
    }
}

/// Complete identity of one checked executable specialization and cache entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageImplementationSpecializationKey {
    declaration: ImplementationExternalSymbolIdentity,
    substitution: Arc<[ImplementationSpecializationArgument]>,
    witnesses: Arc<[ImplementationSpecializationWitness]>,
    configuration: PackageImplementationConfiguration,
    template_schema_revision: ImplementationTemplateSchemaRevision,
    dependencies: Arc<[InterfaceDependency]>,
}

impl PackageImplementationSpecializationKey {
    /// Creates a specialization key from every semantic and code-generation input.
    pub fn new(
        declaration: ImplementationExternalSymbolIdentity,
        substitution: impl IntoIterator<Item = ImplementationSpecializationArgument>,
        witnesses: impl IntoIterator<Item = ImplementationSpecializationWitness>,
        configuration: PackageImplementationConfiguration,
        template_schema_revision: ImplementationTemplateSchemaRevision,
        dependencies: impl IntoIterator<Item = InterfaceDependency>,
    ) -> Self {
        Self {
            declaration,
            substitution: shared_slice(substitution),
            witnesses: sorted_unique_shared_slice(witnesses),
            configuration,
            template_schema_revision,
            dependencies: sorted_unique_shared_slice(dependencies),
        }
    }

    /// Returns the declaring external symbol identity.
    pub const fn declaration(&self) -> ImplementationExternalSymbolIdentity {
        self.declaration
    }

    /// Returns the canonical concrete substitution in parameter order.
    pub fn substitution(&self) -> &[ImplementationSpecializationArgument] {
        &self.substitution
    }

    /// Returns selected implementation witnesses in canonical order.
    pub fn witnesses(&self) -> &[ImplementationSpecializationWitness] {
        &self.witnesses
    }

    /// Returns the exact target, panic, and runtime configuration.
    pub const fn configuration(&self) -> &PackageImplementationConfiguration {
        &self.configuration
    }

    /// Returns the checked-template schema revision.
    pub const fn template_schema_revision(&self) -> ImplementationTemplateSchemaRevision {
        self.template_schema_revision
    }

    /// Returns every exact template dependency identity.
    pub fn dependencies(&self) -> &[InterfaceDependency] {
        &self.dependencies
    }

    /// Returns the content-addressed cache identity of this complete key.
    pub fn cache_identity(&self) -> [u8; 32] {
        let mut digest = StableDigestHasher::new();

        digest.write(b"bray.package-implementation.specialization.v1");
        encode_key(&mut digest, self);

        digest.finalize()
    }
}

/// Optional target-specific MIR bound to one complete specialization identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct InterfacePreSpecializedMir {
    key: PackageImplementationSpecializationKey,
    mir_schema_revision: ImplementationMirSchemaRevision,
    payload: Arc<[u8]>,
}

impl InterfacePreSpecializedMir {
    /// Creates one nonempty pre-specialized MIR payload.
    pub fn new(
        key: PackageImplementationSpecializationKey,
        mir_schema_revision: ImplementationMirSchemaRevision,
        payload: impl Into<Arc<[u8]>>,
    ) -> Option<Self> {
        let payload = payload.into();

        (!payload.is_empty()).then_some(Self {
            key,
            mir_schema_revision,
            payload,
        })
    }

    /// Returns the complete specialization and cache identity.
    pub const fn key(&self) -> &PackageImplementationSpecializationKey {
        &self.key
    }

    /// Returns the selected MIR schema revision.
    pub const fn mir_schema_revision(&self) -> ImplementationMirSchemaRevision {
        self.mir_schema_revision
    }

    /// Returns the canonical target-specific MIR bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

fn encode_key(digest: &mut StableDigestHasher, key: &PackageImplementationSpecializationKey) {
    digest.write(key.declaration.as_bytes());
    encode_arguments(digest, key.substitution());
    digest.write_usize(key.witnesses().len());

    for witness in key.witnesses() {
        digest.write(witness.definition().as_bytes());
        encode_arguments(digest, witness.substitution());
    }

    let configuration = key.configuration();

    encode_string(digest, configuration.target().as_str());
    digest.write(configuration.target_properties());

    match configuration.runtime() {
        Some(runtime) => {
            digest.write_u8(1);
            encode_string(digest, runtime.as_str());
        }
        None => digest.write_u8(0),
    }

    digest.write_u16(configuration.runtime_abi().major());
    digest.write_u16(configuration.runtime_abi().minor());
    encode_string(digest, configuration.panic_abi().as_str());
    digest.write_u16(key.template_schema_revision().raw());
    digest.write_usize(key.dependencies().len());

    for dependency in key.dependencies() {
        encode_string(digest, dependency.package().as_str());
        encode_string(digest, dependency.product().as_str());
        digest.write(dependency.content_hash().as_bytes());
    }
}

fn encode_arguments(
    digest: &mut StableDigestHasher,
    arguments: &[ImplementationSpecializationArgument],
) {
    digest.write_usize(arguments.len());

    for argument in arguments {
        digest.write_u8(match argument.kind() {
            ImplementationSpecializationArgumentKind::Type => 0,
            ImplementationSpecializationArgumentKind::Constant => 1,
        });

        digest.write(&argument.identity());
    }
}

fn encode_string(digest: &mut StableDigestHasher, value: &str) {
    value.len().hash(digest);
    digest.write(value.as_bytes());
}
