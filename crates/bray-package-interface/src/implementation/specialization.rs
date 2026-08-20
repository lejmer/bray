use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::ExternalSymbolKey;

use crate::{ExecutableTemplateDecodeError, InterfaceDependency, InterfaceValidationError};

use super::{ImplementationTemplateSchemaRevision, PackageImplementationConfiguration};

/// Exact MIR schema revision supported by optional pre-specialized payloads.
pub const CURRENT_MIR_SCHEMA_REVISION: ImplementationMirSchemaRevision =
    ImplementationMirSchemaRevision::new(3);

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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationExternalSymbolIdentity(ExternalSymbolKey);

impl ImplementationExternalSymbolIdentity {
    /// Derives a stable identity from a complete external symbol key.
    pub fn new(key: &ExternalSymbolKey) -> Self {
        // External keys are immutable Arc-backed identities retained by the specialization key.
        Self(key.clone())
    }

    /// Returns the complete structured external symbol key.
    pub const fn key(&self) -> &ExternalSymbolKey {
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
    pub const fn new(kind: ImplementationSpecializationArgumentKind, identity: [u8; 32]) -> Self {
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
    pub const fn definition(&self) -> &ImplementationExternalSymbolIdentity {
        &self.definition
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
    pub const fn declaration(&self) -> &ImplementationExternalSymbolIdentity {
        &self.declaration
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
        super::codec::specialization_key_identity(self)
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
    pub(crate) fn new(
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

    pub(crate) fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub(crate) fn shared_payload(&self) -> Arc<[u8]> {
        Arc::clone(&self.payload)
    }
}

/// Failure while reconstructing a validated pre-specialized MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreSpecializedMirDecodeError {
    /// The implementation artifact or specialization envelope is invalid.
    Artifact(InterfaceValidationError),
    /// The schema-owned executable MIR payload is invalid for the selected consumer context.
    Executable(ExecutableTemplateDecodeError),
}

impl From<InterfaceValidationError> for PreSpecializedMirDecodeError {
    fn from(error: InterfaceValidationError) -> Self {
        Self::Artifact(error)
    }
}

impl From<ExecutableTemplateDecodeError> for PreSpecializedMirDecodeError {
    fn from(error: ExecutableTemplateDecodeError) -> Self {
        Self::Executable(error)
    }
}

#[cfg(test)]
mod tests {
    use super::CURRENT_MIR_SCHEMA_REVISION;

    #[test]
    fn task_observation_uses_mir_schema_revision_three() {
        assert_eq!(CURRENT_MIR_SCHEMA_REVISION.raw(), 3);
    }
}
