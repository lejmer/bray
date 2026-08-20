use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};
use bray_runtime_interface::{
    PanicAbiIdentity, RuntimeAbiVersion, RuntimeIdentity, RuntimeRequirements,
};
use bray_target::{
    TargetIdentity, TargetMachineProperties, TargetPropertyKind, TargetPropertyValue,
};

use crate::{
    InterfaceContentHash, InterfaceDependency, InterfaceLanguageRevision, PackageInterfaceIdentity,
};

use super::codec::runtime_requirements_identity;

/// Exact compiler template-schema revision implemented by this crate.
pub const CURRENT_TEMPLATE_SCHEMA_REVISION: ImplementationTemplateSchemaRevision =
    ImplementationTemplateSchemaRevision::new(3);

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
    target: PackageImplementationTargetProperties,
    runtime: Option<RuntimeIdentity>,
    runtime_abi: RuntimeAbiVersion,
    panic_abi: PanicAbiIdentity,
}

impl PackageImplementationConfiguration {
    /// Creates one complete implementation configuration.
    pub const fn new(
        target: PackageImplementationTargetProperties,
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

    /// Creates configuration identity from the target properties embedded in executable MIR.
    pub fn for_mir_target(
        target: &bray_ir::MirTargetContract,
        runtime: Option<RuntimeIdentity>,
        panic_abi: PanicAbiIdentity,
    ) -> Self {
        Self::new(
            PackageImplementationTargetProperties::new(target.profile()),
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
    pub const fn target_properties(&self) -> &PackageImplementationTargetProperties {
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

/// One exact language-defined target property value in a package implementation identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PackageImplementationTargetPropertyValue {
    /// A compiler-known string property.
    String(NonEmptySharedStr),
    /// A target-sized unsigned integer represented independently of the compiler host.
    Usize(u64),
    /// A Boolean target predicate.
    Boolean(bool),
}

/// One language-defined target property and its exact selected value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageImplementationTargetProperty {
    kind: TargetPropertyKind,
    value: PackageImplementationTargetPropertyValue,
}

impl PackageImplementationTargetProperty {
    pub(crate) const fn new(
        kind: TargetPropertyKind,
        value: PackageImplementationTargetPropertyValue,
    ) -> Self {
        Self { kind, value }
    }

    /// Returns the language-defined target property.
    pub const fn kind(&self) -> TargetPropertyKind {
        self.kind
    }

    /// Returns the exact selected property value.
    pub const fn value(&self) -> &PackageImplementationTargetPropertyValue {
        &self.value
    }
}

/// Complete inspectable target identity, machine properties, and language-defined properties.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageImplementationTargetProperties {
    identity: TargetIdentity,
    machine: TargetMachineProperties,
    properties: Arc<[PackageImplementationTargetProperty]>,
}

impl PackageImplementationTargetProperties {
    /// Captures every target property from one validated target profile.
    pub fn new(profile: &bray_target::TargetProfile) -> Self {
        let properties = TargetPropertyKind::ALL.iter().copied().map(|kind| {
            let value = match profile.property(kind) {
                TargetPropertyValue::String(value) => {
                    PackageImplementationTargetPropertyValue::String(
                        NonEmptySharedStr::try_new(value).unwrap_or_else(|| {
                            unreachable!("validated target properties are nonempty")
                        }),
                    )
                }
                TargetPropertyValue::Usize(value) => {
                    PackageImplementationTargetPropertyValue::Usize(value)
                }
                TargetPropertyValue::Boolean(value) => {
                    PackageImplementationTargetPropertyValue::Boolean(value)
                }
            };

            PackageImplementationTargetProperty::new(kind, value)
        });

        // Target identities and machine properties are immutable values retained by the bundle.
        Self {
            identity: profile.identity().clone(),
            machine: profile.machine().clone(),
            properties: shared_slice(properties),
        }
    }

    pub(crate) fn try_from_parts(
        identity: TargetIdentity,
        machine: TargetMachineProperties,
        properties: impl IntoIterator<Item = PackageImplementationTargetProperty>,
    ) -> Option<Self> {
        let properties = shared_slice(properties);

        if properties.len() != TargetPropertyKind::ALL.len()
            || properties
                .iter()
                .zip(TargetPropertyKind::ALL)
                .any(|(property, kind)| {
                    property.kind() != *kind
                        || !target_property_value_matches_kind(*kind, property.value())
                })
            || !machine_properties_match(&identity, &machine, &properties)
        {
            return None;
        }

        Some(Self {
            identity,
            machine,
            properties,
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

    /// Returns every language-defined target property in stable path order.
    pub fn properties(&self) -> &[PackageImplementationTargetProperty] {
        &self.properties
    }
}

fn target_property_value_matches_kind(
    kind: TargetPropertyKind,
    value: &PackageImplementationTargetPropertyValue,
) -> bool {
    match kind {
        TargetPropertyKind::IdentityName
        | TargetPropertyKind::IdentityArchitecture
        | TargetPropertyKind::IdentityVendor
        | TargetPropertyKind::IdentitySystem
        | TargetPropertyKind::IdentityEnvironment
        | TargetPropertyKind::IdentityAbi
        | TargetPropertyKind::CChar
        | TargetPropertyKind::CSignedChar
        | TargetPropertyKind::CUnsignedChar
        | TargetPropertyKind::CShort
        | TargetPropertyKind::CUnsignedShort
        | TargetPropertyKind::CInt
        | TargetPropertyKind::CUnsignedInt
        | TargetPropertyKind::CLong
        | TargetPropertyKind::CUnsignedLong
        | TargetPropertyKind::CLongLong
        | TargetPropertyKind::CUnsignedLongLong
        | TargetPropertyKind::CSize
        | TargetPropertyKind::CPointerDifference
        | TargetPropertyKind::CWideChar
        | TargetPropertyKind::CBool
        | TargetPropertyKind::CFloat
        | TargetPropertyKind::CDouble
        | TargetPropertyKind::CLongDouble => {
            matches!(value, PackageImplementationTargetPropertyValue::String(_))
        }
        TargetPropertyKind::PointerBits
        | TargetPropertyKind::PointerBytes
        | TargetPropertyKind::AtomicU8Alignment
        | TargetPropertyKind::AtomicU16Alignment
        | TargetPropertyKind::AtomicU32Alignment
        | TargetPropertyKind::AtomicU64Alignment
        | TargetPropertyKind::AtomicU128Alignment
        | TargetPropertyKind::AtomicPointerAlignment
        | TargetPropertyKind::AlignmentMaxStorage
        | TargetPropertyKind::AlignmentMaxAllocation => {
            matches!(value, PackageImplementationTargetPropertyValue::Usize(_))
        }
        TargetPropertyKind::EndianLittle
        | TargetPropertyKind::EndianBig
        | TargetPropertyKind::ScalarBool
        | TargetPropertyKind::ScalarChar
        | TargetPropertyKind::ScalarI8
        | TargetPropertyKind::ScalarI16
        | TargetPropertyKind::ScalarI32
        | TargetPropertyKind::ScalarI64
        | TargetPropertyKind::ScalarI128
        | TargetPropertyKind::ScalarU8
        | TargetPropertyKind::ScalarU16
        | TargetPropertyKind::ScalarU32
        | TargetPropertyKind::ScalarU64
        | TargetPropertyKind::ScalarU128
        | TargetPropertyKind::ScalarUsize
        | TargetPropertyKind::ScalarIsize
        | TargetPropertyKind::ScalarR16
        | TargetPropertyKind::ScalarR32
        | TargetPropertyKind::ScalarR64
        | TargetPropertyKind::ScalarR128
        | TargetPropertyKind::ScalarC32
        | TargetPropertyKind::ScalarC64
        | TargetPropertyKind::ScalarC128
        | TargetPropertyKind::ScalarC256
        | TargetPropertyKind::AtomicU8
        | TargetPropertyKind::AtomicU16
        | TargetPropertyKind::AtomicU32
        | TargetPropertyKind::AtomicU64
        | TargetPropertyKind::AtomicU128
        | TargetPropertyKind::AtomicPointer
        | TargetPropertyKind::AtomicU8AlwaysLockFree
        | TargetPropertyKind::AtomicU8WaitNotify
        | TargetPropertyKind::AtomicU8CrossProcess
        | TargetPropertyKind::AtomicU16AlwaysLockFree
        | TargetPropertyKind::AtomicU16WaitNotify
        | TargetPropertyKind::AtomicU16CrossProcess
        | TargetPropertyKind::AtomicU32AlwaysLockFree
        | TargetPropertyKind::AtomicU32WaitNotify
        | TargetPropertyKind::AtomicU32CrossProcess
        | TargetPropertyKind::AtomicU64AlwaysLockFree
        | TargetPropertyKind::AtomicU64WaitNotify
        | TargetPropertyKind::AtomicU64CrossProcess
        | TargetPropertyKind::AtomicU128AlwaysLockFree
        | TargetPropertyKind::AtomicU128WaitNotify
        | TargetPropertyKind::AtomicU128CrossProcess
        | TargetPropertyKind::AtomicPointerAlwaysLockFree
        | TargetPropertyKind::AtomicPointerWaitNotify
        | TargetPropertyKind::AtomicPointerCrossProcess
        | TargetPropertyKind::AbiC
        | TargetPropertyKind::AbiSystem
        | TargetPropertyKind::AddressSpaceHost
        | TargetPropertyKind::AddressSpaceDevice
        | TargetPropertyKind::PlatformDynamicLoading
        | TargetPropertyKind::PlatformNativeThreads => {
            matches!(value, PackageImplementationTargetPropertyValue::Boolean(_))
        }
    }
}

fn machine_properties_match(
    identity: &TargetIdentity,
    machine: &TargetMachineProperties,
    properties: &[PackageImplementationTargetProperty],
) -> bool {
    let value = |kind| {
        TargetPropertyKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .and_then(|index| properties.get(index))
            .map(PackageImplementationTargetProperty::value)
    };

    matches!(
        value(TargetPropertyKind::IdentityName),
        Some(PackageImplementationTargetPropertyValue::String(actual))
            if actual.as_str() == identity.as_str()
    ) && matches!(
        value(TargetPropertyKind::IdentityArchitecture),
        Some(PackageImplementationTargetPropertyValue::String(actual))
            if actual.as_str() == machine.architecture().as_str()
    ) && matches!(
        value(TargetPropertyKind::PointerBits),
        Some(PackageImplementationTargetPropertyValue::Usize(actual))
            if *actual == u64::from(machine.pointer_width_bits().get())
    ) && matches!(
        value(TargetPropertyKind::PointerBytes),
        Some(PackageImplementationTargetPropertyValue::Usize(actual))
            if *actual == u64::from(machine.pointer_width_bits().get() / 8)
    ) && matches!(
        value(TargetPropertyKind::EndianLittle),
        Some(PackageImplementationTargetPropertyValue::Boolean(actual))
            if *actual == (machine.endianness() == bray_target::Endianness::Little)
    ) && matches!(
        value(TargetPropertyKind::EndianBig),
        Some(PackageImplementationTargetPropertyValue::Boolean(actual))
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
