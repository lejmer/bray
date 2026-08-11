use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};
use bray_runtime_interface::{
    PanicAbiIdentity, RuntimeAbiVersion, RuntimeIdentity, RuntimeRequirements,
};
use bray_target::{TargetFactKind, TargetFactValue, TargetIdentity, TargetMachineProperties};

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
    target: PackageImplementationTargetFacts,
    runtime: Option<RuntimeIdentity>,
    runtime_abi: RuntimeAbiVersion,
    panic_abi: PanicAbiIdentity,
}

impl PackageImplementationConfiguration {
    /// Creates one complete implementation configuration.
    pub const fn new(
        target: PackageImplementationTargetFacts,
        runtime: Option<RuntimeIdentity>,
        runtime_abi: RuntimeAbiVersion,
        panic_abi: PanicAbiIdentity,
    ) -> Self {
        Self {
            target,
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
            PackageImplementationTargetFacts::new(target.profile()),
            runtime,
            target.runtime_abi(),
            panic_abi,
        )
    }

    /// Returns the exact target identity.
    pub const fn target(&self) -> &TargetIdentity {
        self.target.identity()
    }

    /// Returns every exact target property used by MIR and semantic selection.
    pub const fn target_facts(&self) -> &PackageImplementationTargetFacts {
        &self.target
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

/// One exact language-defined target fact value in a package implementation identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PackageImplementationTargetFactValue {
    /// A compiler-known string fact.
    String(NonEmptySharedStr),
    /// A target-sized unsigned integer represented independently of the compiler host.
    Usize(u64),
    /// A Boolean target predicate.
    Boolean(bool),
}

/// One language-defined target fact and its exact selected value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageImplementationTargetFact {
    kind: TargetFactKind,
    value: PackageImplementationTargetFactValue,
}

impl PackageImplementationTargetFact {
    pub(crate) const fn new(
        kind: TargetFactKind,
        value: PackageImplementationTargetFactValue,
    ) -> Self {
        Self { kind, value }
    }

    /// Returns the language-defined target fact.
    pub const fn kind(&self) -> TargetFactKind {
        self.kind
    }

    /// Returns the exact selected fact value.
    pub const fn value(&self) -> &PackageImplementationTargetFactValue {
        &self.value
    }
}

/// Complete inspectable target identity, machine properties, and language-defined fact values.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageImplementationTargetFacts {
    identity: TargetIdentity,
    machine: TargetMachineProperties,
    facts: Arc<[PackageImplementationTargetFact]>,
}

impl PackageImplementationTargetFacts {
    /// Captures every target fact from one validated target profile.
    pub fn new(profile: &bray_target::TargetProfile) -> Self {
        let facts = TargetFactKind::ALL.iter().copied().map(|kind| {
            let value = match profile.fact(kind) {
                TargetFactValue::String(value) => PackageImplementationTargetFactValue::String(
                    NonEmptySharedStr::try_new(value)
                        .unwrap_or_else(|| unreachable!("validated target facts are nonempty")),
                ),
                TargetFactValue::Usize(value) => PackageImplementationTargetFactValue::Usize(value),
                TargetFactValue::Boolean(value) => {
                    PackageImplementationTargetFactValue::Boolean(value)
                }
            };

            PackageImplementationTargetFact::new(kind, value)
        });

        // Target identities and machine properties are immutable values retained by the bundle.
        Self {
            identity: profile.identity().clone(),
            machine: profile.machine().clone(),
            facts: shared_slice(facts),
        }
    }

    pub(crate) fn try_from_parts(
        identity: TargetIdentity,
        machine: TargetMachineProperties,
        facts: impl IntoIterator<Item = PackageImplementationTargetFact>,
    ) -> Option<Self> {
        let facts = shared_slice(facts);

        if facts.len() != TargetFactKind::ALL.len()
            || facts.iter().zip(TargetFactKind::ALL).any(|(fact, kind)| {
                fact.kind() != *kind || !target_fact_value_matches_kind(*kind, fact.value())
            })
            || !machine_facts_match(&identity, &machine, &facts)
        {
            return None;
        }

        Some(Self {
            identity,
            machine,
            facts,
        })
    }

    /// Returns the exact compilation target identity.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    /// Returns the exact target machine representation properties.
    pub const fn machine(&self) -> &TargetMachineProperties {
        &self.machine
    }

    /// Returns every language-defined target fact in stable path order.
    pub fn facts(&self) -> &[PackageImplementationTargetFact] {
        &self.facts
    }
}

fn target_fact_value_matches_kind(
    kind: TargetFactKind,
    value: &PackageImplementationTargetFactValue,
) -> bool {
    match kind {
        TargetFactKind::IdentityName
        | TargetFactKind::IdentityArchitecture
        | TargetFactKind::IdentityVendor
        | TargetFactKind::IdentitySystem
        | TargetFactKind::IdentityEnvironment
        | TargetFactKind::IdentityAbi
        | TargetFactKind::CChar
        | TargetFactKind::CSignedChar
        | TargetFactKind::CUnsignedChar
        | TargetFactKind::CShort
        | TargetFactKind::CUnsignedShort
        | TargetFactKind::CInt
        | TargetFactKind::CUnsignedInt
        | TargetFactKind::CLong
        | TargetFactKind::CUnsignedLong
        | TargetFactKind::CLongLong
        | TargetFactKind::CUnsignedLongLong
        | TargetFactKind::CSize
        | TargetFactKind::CPointerDifference
        | TargetFactKind::CWideChar
        | TargetFactKind::CBool
        | TargetFactKind::CFloat
        | TargetFactKind::CDouble
        | TargetFactKind::CLongDouble => {
            matches!(value, PackageImplementationTargetFactValue::String(_))
        }
        TargetFactKind::PointerBits
        | TargetFactKind::PointerBytes
        | TargetFactKind::AlignmentMaxStorage
        | TargetFactKind::AlignmentMaxAllocation => {
            matches!(value, PackageImplementationTargetFactValue::Usize(_))
        }
        TargetFactKind::EndianLittle
        | TargetFactKind::EndianBig
        | TargetFactKind::ScalarBool
        | TargetFactKind::ScalarChar
        | TargetFactKind::ScalarI8
        | TargetFactKind::ScalarI16
        | TargetFactKind::ScalarI32
        | TargetFactKind::ScalarI64
        | TargetFactKind::ScalarI128
        | TargetFactKind::ScalarU8
        | TargetFactKind::ScalarU16
        | TargetFactKind::ScalarU32
        | TargetFactKind::ScalarU64
        | TargetFactKind::ScalarU128
        | TargetFactKind::ScalarUsize
        | TargetFactKind::ScalarIsize
        | TargetFactKind::ScalarR16
        | TargetFactKind::ScalarR32
        | TargetFactKind::ScalarR64
        | TargetFactKind::ScalarR128
        | TargetFactKind::ScalarC32
        | TargetFactKind::ScalarC64
        | TargetFactKind::ScalarC128
        | TargetFactKind::ScalarC256
        | TargetFactKind::AtomicU8
        | TargetFactKind::AtomicU16
        | TargetFactKind::AtomicU32
        | TargetFactKind::AtomicU64
        | TargetFactKind::AtomicU128
        | TargetFactKind::AtomicPointer
        | TargetFactKind::AbiC
        | TargetFactKind::AbiSystem
        | TargetFactKind::AddressSpaceHost
        | TargetFactKind::AddressSpaceDevice
        | TargetFactKind::PlatformDynamicLoading => {
            matches!(value, PackageImplementationTargetFactValue::Boolean(_))
        }
    }
}

fn machine_facts_match(
    identity: &TargetIdentity,
    machine: &TargetMachineProperties,
    facts: &[PackageImplementationTargetFact],
) -> bool {
    let value = |kind| {
        TargetFactKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .and_then(|index| facts.get(index))
            .map(PackageImplementationTargetFact::value)
    };

    matches!(
        value(TargetFactKind::IdentityName),
        Some(PackageImplementationTargetFactValue::String(actual))
            if actual.as_str() == identity.as_str()
    ) && matches!(
        value(TargetFactKind::IdentityArchitecture),
        Some(PackageImplementationTargetFactValue::String(actual))
            if actual.as_str() == machine.architecture().as_str()
    ) && matches!(
        value(TargetFactKind::PointerBits),
        Some(PackageImplementationTargetFactValue::Usize(actual))
            if *actual == u64::from(machine.pointer_width_bits().get())
    ) && matches!(
        value(TargetFactKind::PointerBytes),
        Some(PackageImplementationTargetFactValue::Usize(actual))
            if *actual == u64::from(machine.pointer_width_bits().get() / 8)
    ) && matches!(
        value(TargetFactKind::EndianLittle),
        Some(PackageImplementationTargetFactValue::Boolean(actual))
            if *actual == (machine.endianness() == bray_target::Endianness::Little)
    ) && matches!(
        value(TargetFactKind::EndianBig),
        Some(PackageImplementationTargetFactValue::Boolean(actual))
            if *actual == (machine.endianness() == bray_target::Endianness::Big)
    )
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
