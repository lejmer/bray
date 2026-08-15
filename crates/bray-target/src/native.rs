use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use crate::{
    Endianness, ObjectFormat, TargetArchitecture, TargetAtomicFacts, TargetAtomicOperationFacts,
    TargetAtomicRepresentationFacts, TargetCAbiFacts, TargetFacts, TargetIdentity,
    TargetMachineProperties, TargetOperationFacts, TargetProfile, TargetScalarKind,
};

/// Native target profiles provided by the Bray toolchain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeTarget {
    /// 64-bit x86 Linux using the GNU environment.
    X86_64LinuxGnu,
    /// 64-bit Arm Linux using the GNU environment.
    Aarch64LinuxGnu,
    /// 64-bit x86 Windows using the Microsoft environment.
    X86_64WindowsMsvc,
    /// 64-bit Arm Windows using the Microsoft environment.
    Aarch64WindowsMsvc,
    /// 64-bit x86 macOS.
    X86_64MacOs,
    /// 64-bit Arm macOS.
    Aarch64MacOs,
}

impl NativeTarget {
    /// Every native target in stable toolchain order.
    pub const ALL: [Self; 6] = [
        Self::X86_64LinuxGnu,
        Self::Aarch64LinuxGnu,
        Self::X86_64WindowsMsvc,
        Self::Aarch64WindowsMsvc,
        Self::X86_64MacOs,
        Self::Aarch64MacOs,
    ];

    /// Returns the canonical target identity and backend triple.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
            Self::Aarch64LinuxGnu => "aarch64-unknown-linux-gnu",
            Self::X86_64WindowsMsvc => "x86_64-pc-windows-msvc",
            Self::Aarch64WindowsMsvc => "aarch64-pc-windows-msvc",
            Self::X86_64MacOs => "x86_64-apple-darwin",
            Self::Aarch64MacOs => "aarch64-apple-darwin",
        }
    }

    /// Returns the processor architecture.
    pub const fn architecture(self) -> TargetArchitecture {
        match self {
            Self::X86_64LinuxGnu | Self::X86_64WindowsMsvc | Self::X86_64MacOs => {
                TargetArchitecture::X86_64
            }
            Self::Aarch64LinuxGnu | Self::Aarch64WindowsMsvc | Self::Aarch64MacOs => {
                TargetArchitecture::Aarch64
            }
        }
    }

    /// Returns the native object format.
    pub const fn object_format(self) -> ObjectFormat {
        match self {
            Self::X86_64LinuxGnu | Self::Aarch64LinuxGnu => ObjectFormat::Elf,
            Self::X86_64WindowsMsvc | Self::Aarch64WindowsMsvc => ObjectFormat::Coff,
            Self::X86_64MacOs | Self::Aarch64MacOs => ObjectFormat::MachO,
        }
    }

    /// Returns the canonical backend CPU selection.
    pub const fn cpu(self) -> &'static str {
        match self {
            Self::X86_64LinuxGnu | Self::X86_64WindowsMsvc | Self::X86_64MacOs => "x86-64",
            Self::Aarch64LinuxGnu | Self::Aarch64WindowsMsvc | Self::Aarch64MacOs => "generic",
        }
    }

    /// Returns the canonical target identity.
    pub fn identity(self) -> TargetIdentity {
        TargetIdentity::try_new(self.as_str())
            .unwrap_or_else(|| panic!("native target identity must be valid"))
    }

    /// Returns the complete language-level target profile.
    pub fn profile(self) -> TargetProfile {
        let pointer_width = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);
        let pointer_alignment = NonZeroU32::new(8).unwrap_or(NonZeroU32::MIN);
        let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        let machine = TargetMachineProperties::try_new(
            self.architecture(),
            self.object_format(),
            Endianness::Little,
            pointer_width,
            pointer_alignment,
            stack_alignment,
        )
        .unwrap_or_else(|| panic!("native target machine properties must be valid"));

        let (vendor, system, environment, abi) = self.identity_facts();

        let facts = TargetFacts::try_portable(vendor, system, environment, abi, self.c_abi_facts())
            .map(|facts| {
                facts
                    .with_atomics(native_atomic_facts())
                    .with_operations(TargetOperationFacts::new(true, true))
                    .with_dynamic_loading(true)
            })
            .unwrap_or_else(|| panic!("native target facts must be valid"));

        TargetProfile::try_new(self.identity(), machine, facts)
            .unwrap_or_else(|error| panic!("native target profile must be valid: {error:?}"))
    }

    /// Returns the native target with the supplied canonical identity.
    pub fn for_identity(identity: &TargetIdentity) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|target| target.as_str() == identity.as_str())
    }

    /// Returns the native target represented by an exact target profile.
    pub fn for_profile(profile: &TargetProfile) -> Option<Self> {
        Self::for_identity(profile.identity()).filter(|target| target.profile() == *profile)
    }

    /// Returns the native target matching the compiler host when one is provided.
    pub const fn current() -> Option<Self> {
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            Some(Self::X86_64LinuxGnu)
        } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
            Some(Self::Aarch64LinuxGnu)
        } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
            Some(Self::X86_64WindowsMsvc)
        } else if cfg!(all(target_arch = "aarch64", target_os = "windows")) {
            Some(Self::Aarch64WindowsMsvc)
        } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
            Some(Self::X86_64MacOs)
        } else if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            Some(Self::Aarch64MacOs)
        } else {
            None
        }
    }

    const fn identity_facts(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::X86_64LinuxGnu | Self::Aarch64LinuxGnu => ("unknown", "linux", "gnu", "gnu"),
            Self::X86_64WindowsMsvc | Self::Aarch64WindowsMsvc => ("pc", "windows", "msvc", "msvc"),
            Self::X86_64MacOs | Self::Aarch64MacOs => ("apple", "darwin", "none", "darwin"),
        }
    }

    fn c_abi_facts(self) -> TargetCAbiFacts {
        let char = if self == Self::Aarch64LinuxGnu {
            TargetScalarKind::U8
        } else {
            TargetScalarKind::I8
        };

        let (long, unsigned_long, wide_char) = match self {
            Self::X86_64WindowsMsvc | Self::Aarch64WindowsMsvc => (
                TargetScalarKind::I32,
                TargetScalarKind::U32,
                TargetScalarKind::U16,
            ),
            Self::X86_64LinuxGnu
            | Self::Aarch64LinuxGnu
            | Self::X86_64MacOs
            | Self::Aarch64MacOs => (
                TargetScalarKind::I64,
                TargetScalarKind::U64,
                TargetScalarKind::I32,
            ),
        };

        let long_double = match self {
            Self::X86_64WindowsMsvc | Self::Aarch64WindowsMsvc | Self::Aarch64MacOs => {
                Some(TargetScalarKind::R64)
            }
            Self::X86_64LinuxGnu | Self::Aarch64LinuxGnu | Self::X86_64MacOs => None,
        };

        TargetCAbiFacts::try_new(char, long, unsigned_long, wide_char, long_double)
            .unwrap_or_else(|| panic!("native target C ABI facts must be valid"))
    }
}

fn native_atomic_facts() -> TargetAtomicFacts {
    TargetAtomicFacts::new(
        lock_free_integer(1),
        lock_free_integer(2),
        lock_free_integer(4),
        lock_free_integer(8),
        unavailable_atomic(16),
        lock_free_pointer(8),
    )
}

fn lock_free_integer(alignment: u64) -> TargetAtomicRepresentationFacts {
    atomic_representation(TargetAtomicOperationFacts::integer(), alignment, true)
}

fn lock_free_pointer(alignment: u64) -> TargetAtomicRepresentationFacts {
    atomic_representation(TargetAtomicOperationFacts::pointer(), alignment, true)
}

fn unavailable_atomic(alignment: u64) -> TargetAtomicRepresentationFacts {
    let alignment = NonZeroU64::new(alignment).unwrap_or(NonZeroU64::MIN);

    TargetAtomicRepresentationFacts::unavailable(alignment)
}

fn atomic_representation(
    operations: TargetAtomicOperationFacts,
    alignment: u64,
    wait_notify: bool,
) -> TargetAtomicRepresentationFacts {
    let alignment = NonZeroU64::new(alignment).unwrap_or(NonZeroU64::MIN);

    TargetAtomicRepresentationFacts::try_new(operations, alignment, true, wait_notify, true)
        .unwrap_or_else(|| panic!("native atomic representation facts must be valid"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::NativeTarget;
    use crate::{
        Endianness, ObjectFormat, TargetArchitecture, TargetAtomicRepresentation,
        TargetCScalarKind, TargetScalarKind,
    };

    #[test]
    fn native_profiles_cover_the_declared_platform_matrix() {
        let profiles = NativeTarget::ALL.map(NativeTarget::profile);

        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.identity().as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "aarch64-apple-darwin",
                "aarch64-pc-windows-msvc",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
            ])
        );

        for target in NativeTarget::ALL {
            let profile = target.profile();

            assert_eq!(profile.machine().endianness(), Endianness::Little);
            assert_eq!(profile.machine().pointer_width_bits().get(), 64);
            assert_eq!(profile.machine().pointer_alignment_bytes().get(), 8);
            assert_eq!(profile.machine().stack_alignment_bytes().get(), 16);
            assert!(profile.facts().operations().raw_memory());
            assert!(profile.facts().operations().allocation());
            assert!(profile.facts().dynamic_loading());

            let atomics = profile.facts().atomics();

            assert!(atomics.u8());
            assert!(atomics.u16());
            assert!(atomics.u32());
            assert!(atomics.u64());
            assert!(!atomics.u128());
            assert!(atomics.pointer());

            for (representation, alignment, available) in [
                (TargetAtomicRepresentation::U8, 1, true),
                (TargetAtomicRepresentation::U16, 2, true),
                (TargetAtomicRepresentation::U32, 4, true),
                (TargetAtomicRepresentation::U64, 8, true),
                (TargetAtomicRepresentation::U128, 16, false),
                (TargetAtomicRepresentation::Pointer, 8, true),
            ] {
                let representation = atomics.representation(representation);

                assert_eq!(representation.required_alignment().get(), alignment);
                assert_eq!(representation.always_lock_free(), available);
                assert_eq!(representation.wait_notify(), available);
                assert_eq!(representation.cross_process(), available);
            }

            assert_eq!(
                profile.fact(crate::TargetFactKind::AtomicU64Alignment),
                crate::TargetFactValue::Usize(8)
            );

            assert_eq!(
                profile.fact(crate::TargetFactKind::AtomicU64AlwaysLockFree),
                crate::TargetFactValue::Boolean(true)
            );

            assert_eq!(
                profile.fact(crate::TargetFactKind::AtomicU128WaitNotify),
                crate::TargetFactValue::Boolean(false)
            );

            assert!(matches!(
                profile.machine().architecture(),
                TargetArchitecture::X86_64 | TargetArchitecture::Aarch64
            ));

            assert!(matches!(
                profile.machine().object_format(),
                ObjectFormat::Elf | ObjectFormat::Coff | ObjectFormat::MachO
            ));

            let expected_long = if matches!(
                target,
                NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc
            ) {
                TargetScalarKind::I32
            } else {
                TargetScalarKind::I64
            };

            assert_eq!(
                profile.facts().c_abi().mapping(TargetCScalarKind::Long),
                Some(expected_long)
            );
        }
    }

    #[test]
    fn profile_and_identity_lookup_require_exact_native_contracts() {
        for target in NativeTarget::ALL {
            let profile = target.profile();

            assert_eq!(NativeTarget::for_identity(profile.identity()), Some(target));
            assert_eq!(NativeTarget::for_profile(&profile), Some(target));
        }
    }
}
