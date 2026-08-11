use std::sync::Arc;

use bray_base::shared_slice;
use bray_runtime_interface::{
    PanicAbiIdentity, RuntimeAbiVersion, RuntimeIdentity, RuntimeRequirements,
};
use bray_target::TargetIdentity;

use crate::{
    InterfaceContentHash, InterfaceDependency, InterfaceLanguageRevision, PackageInterfaceIdentity,
};

use super::codec::runtime_requirements_identity;

/// Exact compiler template-schema revision implemented by this crate.
pub const CURRENT_TEMPLATE_SCHEMA_REVISION: ImplementationTemplateSchemaRevision =
    ImplementationTemplateSchemaRevision::new(1);

/// Exact compiler schema used to interpret checked implementation templates.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationTemplateSchemaRevision(u16);

impl ImplementationTemplateSchemaRevision {
    /// Creates a template-schema revision from its stable wire value.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the stable wire value.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Exact target, panic, and runtime configuration used by implementation templates.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageImplementationConfiguration {
    target: TargetIdentity,
    target_properties: [u8; 32],
    runtime: Option<RuntimeIdentity>,
    runtime_abi: RuntimeAbiVersion,
    panic_abi: PanicAbiIdentity,
}

impl PackageImplementationConfiguration {
    /// Creates one complete implementation configuration.
    pub const fn new(
        target: TargetIdentity,
        target_properties: [u8; 32],
        runtime: Option<RuntimeIdentity>,
        runtime_abi: RuntimeAbiVersion,
        panic_abi: PanicAbiIdentity,
    ) -> Self {
        Self {
            target,
            target_properties,
            runtime,
            runtime_abi,
            panic_abi,
        }
    }

    /// Creates configuration identity from the target facts embedded in executable MIR.
    pub fn for_mir_target(
        target: &bray_ir::MirTargetFacts,
        runtime: Option<RuntimeIdentity>,
        panic_abi: PanicAbiIdentity,
    ) -> Self {
        Self::new(
            target.identity().clone(),
            target.compatibility_digest(),
            runtime,
            target.runtime_abi(),
            panic_abi,
        )
    }

    /// Returns the exact target identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the digest of every target property that can affect MIR.
    pub const fn target_properties(&self) -> &[u8; 32] {
        &self.target_properties
    }

    /// Returns the selected runtime implementation, when selection is exact.
    pub const fn runtime(&self) -> Option<&RuntimeIdentity> {
        self.runtime.as_ref()
    }

    /// Returns the selected private runtime ABI.
    pub const fn runtime_abi(&self) -> RuntimeAbiVersion {
        self.runtime_abi
    }

    /// Returns the exact panic ABI.
    pub const fn panic_abi(&self) -> &PanicAbiIdentity {
        &self.panic_abi
    }
}

/// Complete semantic identity required by one package implementation artifact.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageImplementationIdentity {
    interface: PackageInterfaceIdentity,
    interface_content_hash: InterfaceContentHash,
    language_revision: InterfaceLanguageRevision,
    template_schema_revision: ImplementationTemplateSchemaRevision,
    dependencies: Arc<[InterfaceDependency]>,
    configuration: PackageImplementationConfiguration,
    runtime_requirements: Arc<[RuntimeRequirements]>,
    runtime_requirements_identity: [u8; 32],
}

impl PackageImplementationIdentity {
    pub(crate) fn new(
        interface: PackageInterfaceIdentity,
        interface_content_hash: InterfaceContentHash,
        language_revision: InterfaceLanguageRevision,
        dependencies: impl IntoIterator<Item = InterfaceDependency>,
        configuration: PackageImplementationConfiguration,
        runtime_requirements: impl IntoIterator<Item = RuntimeRequirements>,
    ) -> Self {
        let mut runtime_requirements = runtime_requirements.into_iter().collect::<Vec<_>>();

        runtime_requirements.sort_unstable();
        runtime_requirements.dedup();

        let runtime_requirements_identity =
            runtime_requirements_identity(runtime_requirements.iter());

        Self {
            interface,
            interface_content_hash,
            language_revision,
            template_schema_revision: CURRENT_TEMPLATE_SCHEMA_REVISION,
            dependencies: shared_slice(dependencies),
            configuration,
            runtime_requirements: runtime_requirements.into(),
            runtime_requirements_identity,
        }
    }

    /// Returns the exact package and product identity.
    pub const fn interface(&self) -> &PackageInterfaceIdentity {
        &self.interface
    }

    /// Returns the exact public-interface semantic content hash.
    pub const fn interface_content_hash(&self) -> InterfaceContentHash {
        self.interface_content_hash
    }

    /// Returns the required language semantic revision.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.language_revision
    }

    /// Returns the exact checked-template schema revision.
    pub const fn template_schema_revision(&self) -> ImplementationTemplateSchemaRevision {
        self.template_schema_revision
    }

    /// Returns exact dependency interface identities in canonical order.
    pub fn dependencies(&self) -> &[InterfaceDependency] {
        &self.dependencies
    }

    /// Returns the exact target and runtime configuration.
    pub const fn configuration(&self) -> &PackageImplementationConfiguration {
        &self.configuration
    }

    /// Returns every required runtime, runtime ABI, frame ABI, target, and panic ABI identity.
    pub fn runtime_requirements(&self) -> &[RuntimeRequirements] {
        &self.runtime_requirements
    }

    /// Returns the canonical integrity commitment for all runtime requirements.
    pub const fn runtime_requirements_identity(&self) -> &[u8; 32] {
        &self.runtime_requirements_identity
    }
}
