use std::num::{NonZeroU16, NonZeroU32};

use bray_codegen::{
    CallableAbiMapping, CodegenLinkage, CodegenTarget, CodegenTargetBuildError,
    TargetAbi, TargetAddressSpace, TargetAddressSpaceKind, TargetCallingConvention,
    TargetCompatibility, TargetContract, TargetDataLayout, TargetMachineSelection,
    TargetScalarKind, TargetScalarLayout, TargetSymbolConvention,
};
use bray_compiler_known::AvailabilityRule;
use bray_runtime_interface::{PanicAbiIdentity, RuntimeAbiVersion};
use bray_symbols::AvailableCompilerKnownSymbols;
use bray_target::{
    CodeModel, Endianness, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    TargetMachineProperties, TargetProfile,
};
use bray_symbols::CallableAbi;

/// The target profile and product ABI selected for one compilation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedTarget {
    profile: TargetProfile,
    runtime_abi: RuntimeAbiVersion,
}

impl SelectedTarget {
    /// Creates a selected target from validated language and runtime facts.
    pub const fn new(profile: TargetProfile, runtime_abi: RuntimeAbiVersion) -> Self {
        Self {
            profile,
            runtime_abi,
        }
    }

    /// Creates the compiler's deterministic baseline target.
    pub fn baseline() -> Self {
        let Some(identity) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
            panic!("the compiler baseline target identity must be valid");
        };

        let pointer_width = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);
        let pointer_alignment = NonZeroU32::new(8).unwrap_or(NonZeroU32::MIN);
        let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        let Some(machine) = TargetMachineProperties::try_new(
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
            Endianness::Little,
            pointer_width,
            pointer_alignment,
            stack_alignment,
        ) else {
            panic!("the compiler baseline target machine must be valid");
        };

        let facts = match bray_target::TargetFacts::try_portable("unknown", "linux", "gnu", "gnu") {
            Some(facts) => facts,
            None => panic!("the compiler baseline target facts must be valid"),
        };

        let profile = match TargetProfile::try_new(identity, machine, facts) {
            Ok(profile) => profile,
            Err(error) => panic!("the compiler baseline target profile must be valid: {error:?}"),
        };

        Self::new(profile, RuntimeAbiVersion::new(1, 0))
    }

    /// Returns the selected language-level target profile.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
    }

    /// Returns the selected private runtime ABI version.
    pub const fn runtime_abi(&self) -> RuntimeAbiVersion {
        self.runtime_abi
    }

    /// Returns whether the target satisfies one compiler-known availability rule.
    pub const fn supports(&self, rule: AvailabilityRule) -> bool {
        let facts = self.profile.facts();

        match rule {
            AvailabilityRule::Always => true,
            AvailabilityRule::Real16 => facts.scalars().real16(),
            AvailabilityRule::Real128 => facts.scalars().real128(),
            AvailabilityRule::Complex32 => facts.scalars().complex32(),
            AvailabilityRule::Complex256 => facts.scalars().complex256(),
            AvailabilityRule::RawMemory => facts.operations().raw_memory(),
            AvailabilityRule::Atomics => facts.atomics().any(),
            AvailabilityRule::ForeignAbi => facts.abis().c() || facts.abis().system(),
            AvailabilityRule::AddressSpaces => facts.address_spaces().device(),
            AvailabilityRule::Allocation => facts.operations().allocation(),
        }
    }

    /// Returns the width used by target-sized integer literals and constants.
    pub const fn integer_width_bits(&self) -> NonZeroU16 {
        self.profile.machine().pointer_width_bits()
    }

    /// Returns the backend-neutral code generation target.
    pub fn codegen_target(&self) -> Result<CodegenTarget, CodegenTargetBuildError> {
        let contract = TargetContract::new(
            self.codegen_data_layout(),
            target_abi(),
            panic_abi(),
            target_symbols(),
            target_compatibility(),
        );

        let selection = TargetMachineSelection::try_new(
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            "x86-64",
            std::iter::empty::<&str>(),
        )?;

        CodegenTarget::try_new(
            self.profile.clone(),
            self.profile.identity().as_str(),
            contract,
            selection,
        )
    }

    fn codegen_data_layout(&self) -> TargetDataLayout {
        let machine = self.profile.machine();

        let integer_widths = [8_u16, 16, 32, 64, 128];
        let float_widths = [16_u16, 32, 64, 128];

        let scalars = std::iter::once(scalar_layout(
            TargetScalarKind::Boolean,
            1,
            1,
        ))
        .chain(integer_widths.into_iter().map(|width| {
            let bytes = width.div_ceil(8);

            let alignment = if width == 128 {
                16
            } else {
                scalar_alignment(bytes)
            };

            scalar_layout(
                TargetScalarKind::Integer(nonzero_u16(width)),
                bytes,
                alignment,
            )
        }))
        .chain(float_widths.into_iter().map(|width| {
            let bytes = width.div_ceil(8);

            let alignment = if width == 128 {
                16
            } else {
                scalar_alignment(bytes)
            };

            scalar_layout(
                TargetScalarKind::Float(nonzero_u16(width)),
                bytes,
                alignment,
            )
        }));

        let address_spaces = [
            TargetAddressSpaceKind::Default,
            TargetAddressSpaceKind::Function,
            TargetAddressSpaceKind::Global,
            TargetAddressSpaceKind::Constant,
            TargetAddressSpaceKind::Stack,
            TargetAddressSpaceKind::Heap,
        ]
        .into_iter()
        .map(|kind| TargetAddressSpace::new(kind, 0));

        TargetDataLayout::try_new(
            scalars,
            machine.pointer_alignment_bytes(),
            address_spaces,
        )
        .unwrap_or_else(|error| panic!("selected target data layout must be valid: {error:?}"))
    }
}

fn scalar_layout(
    kind: TargetScalarKind,
    size_bytes: u16,
    alignment_bytes: u16,
) -> TargetScalarLayout {
    TargetScalarLayout::try_new(
        kind,
        nonzero_u16(size_bytes),
        nonzero_u16(alignment_bytes),
    )
    .unwrap_or_else(|error| panic!("selected scalar layout must be valid: {error:?}"))
}

const fn scalar_alignment(size_bytes: u16) -> u16 {
    if size_bytes > 8 {
        8
    } else {
        size_bytes
    }
}

fn nonzero_u16(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value)
        .unwrap_or_else(|| panic!("selected target scalar width must be nonzero"))
}

fn target_abi() -> TargetAbi {
    let mappings = [
        (CallableAbi::Bray, "bray-x86_64"),
        (CallableAbi::C, "sysv64"),
        (CallableAbi::System, "sysv64"),
    ]
    .into_iter()
    .map(|(abi, name)| {
        let convention = TargetCallingConvention::try_new(name)
            .unwrap_or_else(|| panic!("selected calling convention must be valid"));

        CallableAbiMapping::new(abi, convention)
    });

    TargetAbi::try_new(mappings)
        .unwrap_or_else(|error| panic!("selected target ABI must be valid: {error:?}"))
}

fn panic_abi() -> PanicAbiIdentity {
    PanicAbiIdentity::try_new("bray.panic.unwind")
        .unwrap_or_else(|| panic!("selected panic ABI identity must be valid"))
}

fn target_symbols() -> TargetSymbolConvention {
    TargetSymbolConvention::try_new(
        "",
        ".Lbray.",
        [
            CodegenLinkage::Private,
            CodegenLinkage::Internal,
            CodegenLinkage::External,
            CodegenLinkage::Weak,
            CodegenLinkage::LinkOnce,
            CodegenLinkage::Common,
            CodegenLinkage::Import,
            CodegenLinkage::Export,
        ],
    )
    .unwrap_or_else(|_| panic!("selected target symbol convention must be valid"))
}

fn target_compatibility() -> TargetCompatibility {
    TargetCompatibility::try_new("bray-x86_64-elf", 1, ["elf"])
        .unwrap_or_else(|| panic!("selected target compatibility must be valid"))
}

impl Default for SelectedTarget {
    fn default() -> Self {
        Self::baseline()
    }
}

/// Target facts and compiler-known declarations available to one compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedTargetContext {
    target: SelectedTarget,
    available_compiler_known_symbols: AvailableCompilerKnownSymbols,
}

impl SelectedTargetContext {
    pub(crate) const fn new(
        target: SelectedTarget,
        available_compiler_known_symbols: AvailableCompilerKnownSymbols,
    ) -> Self {
        Self {
            target,
            available_compiler_known_symbols,
        }
    }

    /// Returns the selected target and product ABI facts.
    pub const fn target(&self) -> &SelectedTarget {
        &self.target
    }

    /// Returns the compiler-known declarations available for this target.
    pub const fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        &self.available_compiler_known_symbols
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::AvailabilityRule;
    use bray_runtime_interface::RuntimeAbiVersion;

    use super::SelectedTarget;

    #[test]
    fn baseline_target_is_explicit_and_host_independent() {
        let target = SelectedTarget::baseline();

        assert_eq!(
            target.profile().identity().as_str(),
            "x86_64-unknown-linux-gnu"
        );

        assert_eq!(target.integer_width_bits().get(), 64);
        assert_eq!(target.runtime_abi(), RuntimeAbiVersion::new(1, 0));
    }

    #[test]
    fn declaration_availability_is_derived_from_the_selected_profile() {
        let target = SelectedTarget::baseline();

        assert!(target.supports(AvailabilityRule::Always));
        assert!(!target.supports(AvailabilityRule::Real16));
        assert!(!target.supports(AvailabilityRule::RawMemory));
        assert!(target.supports(AvailabilityRule::ForeignAbi));
    }
}
