use std::num::NonZeroU64;

use bray_base::NonEmptySharedStr;

use crate::{Endianness, TargetProfile};

/// One language-defined fact exposed under the compiler-known `target` path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetFactKind {
    /// `target.identity.name`.
    IdentityName,
    /// `target.identity.arch`.
    IdentityArchitecture,
    /// `target.identity.vendor`.
    IdentityVendor,
    /// `target.identity.system`.
    IdentitySystem,
    /// `target.identity.environment`.
    IdentityEnvironment,
    /// `target.identity.abi`.
    IdentityAbi,
    /// `target.pointer.bits`.
    PointerBits,
    /// `target.pointer.bytes`.
    PointerBytes,
    /// `target.endian.little`.
    EndianLittle,
    /// `target.endian.big`.
    EndianBig,
    /// `target.scalar.bool`.
    ScalarBool,
    /// `target.scalar.char`.
    ScalarChar,
    /// `target.scalar.i8`.
    ScalarI8,
    /// `target.scalar.i16`.
    ScalarI16,
    /// `target.scalar.i32`.
    ScalarI32,
    /// `target.scalar.i64`.
    ScalarI64,
    /// `target.scalar.i128`.
    ScalarI128,
    /// `target.scalar.u8`.
    ScalarU8,
    /// `target.scalar.u16`.
    ScalarU16,
    /// `target.scalar.u32`.
    ScalarU32,
    /// `target.scalar.u64`.
    ScalarU64,
    /// `target.scalar.u128`.
    ScalarU128,
    /// `target.scalar.usize`.
    ScalarUsize,
    /// `target.scalar.isize`.
    ScalarIsize,
    /// `target.scalar.r16`.
    ScalarR16,
    /// `target.scalar.r32`.
    ScalarR32,
    /// `target.scalar.r64`.
    ScalarR64,
    /// `target.scalar.r128`.
    ScalarR128,
    /// `target.scalar.c32`.
    ScalarC32,
    /// `target.scalar.c64`.
    ScalarC64,
    /// `target.scalar.c128`.
    ScalarC128,
    /// `target.scalar.c256`.
    ScalarC256,
    /// `target.atomic.u8`.
    AtomicU8,
    /// `target.atomic.u16`.
    AtomicU16,
    /// `target.atomic.u32`.
    AtomicU32,
    /// `target.atomic.u64`.
    AtomicU64,
    /// `target.atomic.u128`.
    AtomicU128,
    /// `target.atomic.pointer`.
    AtomicPointer,
    /// `target.abi.c`.
    AbiC,
    /// `target.abi.system`.
    AbiSystem,
    /// `target.address_space.host`.
    AddressSpaceHost,
    /// `target.address_space.device`.
    AddressSpaceDevice,
    /// `target.alignment.max_storage`.
    AlignmentMaxStorage,
    /// `target.alignment.max_allocation`.
    AlignmentMaxAllocation,
}

impl TargetFactKind {
    /// Returns the language-defined target-fact path.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IdentityName => "target.identity.name",
            Self::IdentityArchitecture => "target.identity.arch",
            Self::IdentityVendor => "target.identity.vendor",
            Self::IdentitySystem => "target.identity.system",
            Self::IdentityEnvironment => "target.identity.environment",
            Self::IdentityAbi => "target.identity.abi",
            Self::PointerBits => "target.pointer.bits",
            Self::PointerBytes => "target.pointer.bytes",
            Self::EndianLittle => "target.endian.little",
            Self::EndianBig => "target.endian.big",
            Self::ScalarBool => "target.scalar.bool",
            Self::ScalarChar => "target.scalar.char",
            Self::ScalarI8 => "target.scalar.i8",
            Self::ScalarI16 => "target.scalar.i16",
            Self::ScalarI32 => "target.scalar.i32",
            Self::ScalarI64 => "target.scalar.i64",
            Self::ScalarI128 => "target.scalar.i128",
            Self::ScalarU8 => "target.scalar.u8",
            Self::ScalarU16 => "target.scalar.u16",
            Self::ScalarU32 => "target.scalar.u32",
            Self::ScalarU64 => "target.scalar.u64",
            Self::ScalarU128 => "target.scalar.u128",
            Self::ScalarUsize => "target.scalar.usize",
            Self::ScalarIsize => "target.scalar.isize",
            Self::ScalarR16 => "target.scalar.r16",
            Self::ScalarR32 => "target.scalar.r32",
            Self::ScalarR64 => "target.scalar.r64",
            Self::ScalarR128 => "target.scalar.r128",
            Self::ScalarC32 => "target.scalar.c32",
            Self::ScalarC64 => "target.scalar.c64",
            Self::ScalarC128 => "target.scalar.c128",
            Self::ScalarC256 => "target.scalar.c256",
            Self::AtomicU8 => "target.atomic.u8",
            Self::AtomicU16 => "target.atomic.u16",
            Self::AtomicU32 => "target.atomic.u32",
            Self::AtomicU64 => "target.atomic.u64",
            Self::AtomicU128 => "target.atomic.u128",
            Self::AtomicPointer => "target.atomic.pointer",
            Self::AbiC => "target.abi.c",
            Self::AbiSystem => "target.abi.system",
            Self::AddressSpaceHost => "target.address_space.host",
            Self::AddressSpaceDevice => "target.address_space.device",
            Self::AlignmentMaxStorage => "target.alignment.max_storage",
            Self::AlignmentMaxAllocation => "target.alignment.max_allocation",
        }
    }
}

/// The typed value of one language-defined target fact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetFactValue<'profile> {
    /// A compiler-known string fact.
    String(&'profile str),
    /// A compiler-known `usize` fact represented independently of the compiler host.
    Usize(u64),
    /// A compiler-known Boolean fact.
    Boolean(bool),
}

impl<'profile> TargetFactValue<'profile> {
    pub(crate) fn for_profile(profile: &'profile TargetProfile, kind: TargetFactKind) -> Self {
        let facts = profile.facts();

        match kind {
            TargetFactKind::IdentityName => Self::String(profile.identity().as_str()),
            TargetFactKind::IdentityArchitecture => {
                Self::String(profile.machine().architecture().as_str())
            }
            TargetFactKind::IdentityVendor => Self::String(facts.identity().vendor()),
            TargetFactKind::IdentitySystem => Self::String(facts.identity().system()),
            TargetFactKind::IdentityEnvironment => Self::String(facts.identity().environment()),
            TargetFactKind::IdentityAbi => Self::String(facts.identity().abi()),
            TargetFactKind::PointerBits => {
                Self::Usize(u64::from(profile.machine().pointer_width_bits().get()))
            }
            TargetFactKind::PointerBytes => {
                Self::Usize(u64::from(profile.machine().pointer_width_bits().get() / 8))
            }
            TargetFactKind::EndianLittle => {
                Self::Boolean(profile.machine().endianness() == Endianness::Little)
            }
            TargetFactKind::EndianBig => {
                Self::Boolean(profile.machine().endianness() == Endianness::Big)
            }
            TargetFactKind::ScalarBool
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
            | TargetFactKind::ScalarR32
            | TargetFactKind::ScalarR64
            | TargetFactKind::ScalarC64
            | TargetFactKind::ScalarC128 => Self::Boolean(true),
            TargetFactKind::ScalarR16 => Self::Boolean(facts.scalars().real16()),
            TargetFactKind::ScalarR128 => Self::Boolean(facts.scalars().real128()),
            TargetFactKind::ScalarC32 => Self::Boolean(facts.scalars().complex32()),
            TargetFactKind::ScalarC256 => Self::Boolean(facts.scalars().complex256()),
            TargetFactKind::AtomicU8 => Self::Boolean(facts.atomics().u8()),
            TargetFactKind::AtomicU16 => Self::Boolean(facts.atomics().u16()),
            TargetFactKind::AtomicU32 => Self::Boolean(facts.atomics().u32()),
            TargetFactKind::AtomicU64 => Self::Boolean(facts.atomics().u64()),
            TargetFactKind::AtomicU128 => Self::Boolean(facts.atomics().u128()),
            TargetFactKind::AtomicPointer => Self::Boolean(facts.atomics().pointer()),
            TargetFactKind::AbiC => Self::Boolean(facts.abis().c()),
            TargetFactKind::AbiSystem => Self::Boolean(facts.abis().system()),
            TargetFactKind::AddressSpaceHost => Self::Boolean(facts.address_spaces().host()),
            TargetFactKind::AddressSpaceDevice => Self::Boolean(facts.address_spaces().device()),
            TargetFactKind::AlignmentMaxStorage => {
                Self::Usize(facts.alignments().max_storage().get())
            }
            TargetFactKind::AlignmentMaxAllocation => {
                Self::Usize(facts.alignments().max_allocation().get())
            }
        }
    }
}

/// Stable identity details of one target profile.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetIdentityFacts {
    vendor: NonEmptySharedStr,
    system: NonEmptySharedStr,
    environment: NonEmptySharedStr,
    abi: NonEmptySharedStr,
}

impl TargetIdentityFacts {
    /// Creates complete identity facts when every spelling is nonempty.
    pub fn try_new(
        vendor: impl Into<std::sync::Arc<str>>,
        system: impl Into<std::sync::Arc<str>>,
        environment: impl Into<std::sync::Arc<str>>,
        abi: impl Into<std::sync::Arc<str>>,
    ) -> Option<Self> {
        Some(Self {
            vendor: NonEmptySharedStr::try_new(vendor)?,
            system: NonEmptySharedStr::try_new(system)?,
            environment: NonEmptySharedStr::try_new(environment)?,
            abi: NonEmptySharedStr::try_new(abi)?,
        })
    }

    /// Returns the target vendor spelling.
    pub fn vendor(&self) -> &str {
        self.vendor.as_str()
    }

    /// Returns the target operating-system spelling.
    pub fn system(&self) -> &str {
        self.system.as_str()
    }

    /// Returns the target environment spelling.
    pub fn environment(&self) -> &str {
        self.environment.as_str()
    }

    /// Returns the target ABI-family spelling.
    pub fn abi(&self) -> &str {
        self.abi.as_str()
    }
}

/// Availability of target-conditional scalar representations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetScalarFacts {
    real16: bool,
    real128: bool,
    complex32: bool,
    complex256: bool,
}

impl TargetScalarFacts {
    /// Creates the target-conditional scalar availability facts.
    pub const fn new(real16: bool, real128: bool, complex32: bool, complex256: bool) -> Self {
        Self {
            real16,
            real128,
            complex32,
            complex256,
        }
    }

    /// Returns whether `r16` is available.
    pub const fn real16(self) -> bool {
        self.real16
    }

    /// Returns whether `r128` is available.
    pub const fn real128(self) -> bool {
        self.real128
    }

    /// Returns whether `c32` is available.
    pub const fn complex32(self) -> bool {
        self.complex32
    }

    /// Returns whether `c256` is available.
    pub const fn complex256(self) -> bool {
        self.complex256
    }
}

/// Atomic representations available on one target.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAtomicFacts {
    u8: bool,
    u16: bool,
    u32: bool,
    u64: bool,
    u128: bool,
    pointer: bool,
}

impl TargetAtomicFacts {
    /// Creates atomic representation availability facts.
    pub const fn new(u8: bool, u16: bool, u32: bool, u64: bool, u128: bool, pointer: bool) -> Self {
        Self {
            u8,
            u16,
            u32,
            u64,
            u128,
            pointer,
        }
    }

    /// Returns whether atomic `u8` operations are available.
    pub const fn u8(self) -> bool {
        self.u8
    }

    /// Returns whether atomic `u16` operations are available.
    pub const fn u16(self) -> bool {
        self.u16
    }

    /// Returns whether atomic `u32` operations are available.
    pub const fn u32(self) -> bool {
        self.u32
    }

    /// Returns whether atomic `u64` operations are available.
    pub const fn u64(self) -> bool {
        self.u64
    }

    /// Returns whether atomic `u128` operations are available.
    pub const fn u128(self) -> bool {
        self.u128
    }

    /// Returns whether atomic pointer operations are available.
    pub const fn pointer(self) -> bool {
        self.pointer
    }
}

/// Callable ABI families available on one target.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAbiFacts {
    c: bool,
    system: bool,
}

impl TargetAbiFacts {
    /// Creates callable ABI availability facts.
    pub const fn new(c: bool, system: bool) -> Self {
        Self { c, system }
    }

    /// Returns whether the target C ABI is available.
    pub const fn c(self) -> bool {
        self.c
    }

    /// Returns whether the target system ABI is available.
    pub const fn system(self) -> bool {
        self.system
    }
}

/// Address spaces available on one target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAddressSpaceFacts {
    host: bool,
    device: bool,
}

impl TargetAddressSpaceFacts {
    /// Creates address-space facts when at least one address space is available.
    pub const fn try_new(host: bool, device: bool) -> Option<Self> {
        if !host && !device {
            return None;
        }

        Some(Self { host, device })
    }

    /// Returns whether the host address space is available.
    pub const fn host(self) -> bool {
        self.host
    }

    /// Returns whether the device address space is available.
    pub const fn device(self) -> bool {
        self.device
    }
}

/// Maximum supported storage and allocation alignments.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAlignmentFacts {
    max_storage: NonZeroU64,
    max_allocation: NonZeroU64,
}

impl TargetAlignmentFacts {
    /// Creates alignment facts when both maxima are powers of two and allocation does not exceed storage.
    pub const fn try_new(max_storage: NonZeroU64, max_allocation: NonZeroU64) -> Option<Self> {
        if !max_storage.get().is_power_of_two()
            || !max_allocation.get().is_power_of_two()
            || max_allocation.get() > max_storage.get()
        {
            return None;
        }

        Some(Self {
            max_storage,
            max_allocation,
        })
    }

    /// Returns the maximum supported storage alignment in bytes.
    pub const fn max_storage(self) -> NonZeroU64 {
        self.max_storage
    }

    /// Returns the maximum supported allocation alignment in bytes.
    pub const fn max_allocation(self) -> NonZeroU64 {
        self.max_allocation
    }
}

/// Complete language-defined facts not derived from target identity or machine properties.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetFacts {
    identity: TargetIdentityFacts,
    scalars: TargetScalarFacts,
    atomics: TargetAtomicFacts,
    abis: TargetAbiFacts,
    address_spaces: TargetAddressSpaceFacts,
    alignments: TargetAlignmentFacts,
}

impl TargetFacts {
    /// Creates a complete set of independently supplied language target facts.
    pub const fn new(
        identity: TargetIdentityFacts,
        scalars: TargetScalarFacts,
        atomics: TargetAtomicFacts,
        abis: TargetAbiFacts,
        address_spaces: TargetAddressSpaceFacts,
        alignments: TargetAlignmentFacts,
    ) -> Self {
        Self {
            identity,
            scalars,
            atomics,
            abis,
            address_spaces,
            alignments,
        }
    }

    /// Creates the complete portable baseline fact set when identity spellings are nonempty.
    pub fn try_portable(vendor: &str, system: &str, environment: &str, abi: &str) -> Option<Self> {
        let identity = TargetIdentityFacts::try_new(vendor, system, environment, abi)?;

        let address_spaces = TargetAddressSpaceFacts::try_new(true, false)?;

        let maximum_alignment = NonZeroU64::new(1 << 29).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentFacts::try_new(maximum_alignment, maximum_alignment)?;

        Some(Self::new(
            identity,
            TargetScalarFacts::default(),
            TargetAtomicFacts::default(),
            TargetAbiFacts::new(true, true),
            address_spaces,
            alignments,
        ))
    }

    /// Returns stable target identity facts.
    pub const fn identity(&self) -> &TargetIdentityFacts {
        &self.identity
    }

    /// Returns target-conditional scalar availability facts.
    pub const fn scalars(&self) -> TargetScalarFacts {
        self.scalars
    }

    /// Returns atomic representation availability facts.
    pub const fn atomics(&self) -> TargetAtomicFacts {
        self.atomics
    }

    /// Returns callable ABI availability facts.
    pub const fn abis(&self) -> TargetAbiFacts {
        self.abis
    }

    /// Returns address-space availability facts.
    pub const fn address_spaces(&self) -> TargetAddressSpaceFacts {
        self.address_spaces
    }

    /// Returns maximum supported alignment facts.
    pub const fn alignments(&self) -> TargetAlignmentFacts {
        self.alignments
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use crate::test_support::test_target_profile;
    use crate::{TargetAlignmentFacts, TargetFactKind, TargetFactValue, TargetIdentityFacts};

    #[test]
    fn derived_facts_cannot_contradict_machine_properties() {
        let profile = test_target_profile();

        assert_eq!(
            profile.fact(TargetFactKind::IdentityArchitecture),
            TargetFactValue::String("x86_64")
        );

        assert_eq!(
            profile.fact(TargetFactKind::PointerBits),
            TargetFactValue::Usize(64)
        );

        assert_eq!(
            profile.fact(TargetFactKind::EndianLittle),
            TargetFactValue::Boolean(true)
        );

        assert_eq!(
            profile.fact(TargetFactKind::EndianBig),
            TargetFactValue::Boolean(false)
        );
    }

    #[test]
    fn required_scalar_facts_are_always_available() {
        let profile = test_target_profile();

        assert_eq!(
            profile.fact(TargetFactKind::ScalarI32),
            TargetFactValue::Boolean(true)
        );

        assert_eq!(
            profile.fact(TargetFactKind::ScalarR16),
            TargetFactValue::Boolean(false)
        );
    }

    #[test]
    fn incomplete_or_contradictory_fact_groups_are_rejected() {
        assert!(TargetIdentityFacts::try_new("", "linux", "gnu", "gnu").is_none());

        let storage = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let allocation = NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN);

        assert_eq!(TargetAlignmentFacts::try_new(storage, allocation), None);
    }
}
