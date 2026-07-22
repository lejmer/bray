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
