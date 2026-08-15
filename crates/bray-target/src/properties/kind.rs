/// One language-defined property exposed under the compiler-known `target` path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetPropertyKind {
    /// `target.identity.NAME`.
    IdentityName,
    /// `target.identity.ARCH`.
    IdentityArchitecture,
    /// `target.identity.VENDOR`.
    IdentityVendor,
    /// `target.identity.SYSTEM`.
    IdentitySystem,
    /// `target.identity.ENVIRONMENT`.
    IdentityEnvironment,
    /// `target.identity.ABI`.
    IdentityAbi,
    /// `target.pointer.BITS`.
    PointerBits,
    /// `target.pointer.BYTES`.
    PointerBytes,
    /// `target.endian.LITTLE`.
    EndianLittle,
    /// `target.endian.BIG`.
    EndianBig,
    /// `target.scalar.BOOL`.
    ScalarBool,
    /// `target.scalar.CHAR`.
    ScalarChar,
    /// `target.scalar.I8`.
    ScalarI8,
    /// `target.scalar.I16`.
    ScalarI16,
    /// `target.scalar.I32`.
    ScalarI32,
    /// `target.scalar.I64`.
    ScalarI64,
    /// `target.scalar.I128`.
    ScalarI128,
    /// `target.scalar.U8`.
    ScalarU8,
    /// `target.scalar.U16`.
    ScalarU16,
    /// `target.scalar.U32`.
    ScalarU32,
    /// `target.scalar.U64`.
    ScalarU64,
    /// `target.scalar.U128`.
    ScalarU128,
    /// `target.scalar.USIZE`.
    ScalarUsize,
    /// `target.scalar.ISIZE`.
    ScalarIsize,
    /// `target.scalar.R16`.
    ScalarR16,
    /// `target.scalar.R32`.
    ScalarR32,
    /// `target.scalar.R64`.
    ScalarR64,
    /// `target.scalar.R128`.
    ScalarR128,
    /// `target.scalar.C32`.
    ScalarC32,
    /// `target.scalar.C64`.
    ScalarC64,
    /// `target.scalar.C128`.
    ScalarC128,
    /// `target.scalar.C256`.
    ScalarC256,
    /// `target.atomic.U8`.
    AtomicU8,
    /// `target.atomic.U16`.
    AtomicU16,
    /// `target.atomic.U32`.
    AtomicU32,
    /// `target.atomic.U64`.
    AtomicU64,
    /// `target.atomic.U128`.
    AtomicU128,
    /// `target.atomic.POINTER`.
    AtomicPointer,
    /// `target.atomic.U8_ALIGNMENT`.
    AtomicU8Alignment,
    /// `target.atomic.U8_ALWAYS_LOCK_FREE`.
    AtomicU8AlwaysLockFree,
    /// `target.atomic.U8_WAIT_NOTIFY`.
    AtomicU8WaitNotify,
    /// `target.atomic.U8_CROSS_PROCESS`.
    AtomicU8CrossProcess,
    /// `target.atomic.U16_ALIGNMENT`.
    AtomicU16Alignment,
    /// `target.atomic.U16_ALWAYS_LOCK_FREE`.
    AtomicU16AlwaysLockFree,
    /// `target.atomic.U16_WAIT_NOTIFY`.
    AtomicU16WaitNotify,
    /// `target.atomic.U16_CROSS_PROCESS`.
    AtomicU16CrossProcess,
    /// `target.atomic.U32_ALIGNMENT`.
    AtomicU32Alignment,
    /// `target.atomic.U32_ALWAYS_LOCK_FREE`.
    AtomicU32AlwaysLockFree,
    /// `target.atomic.U32_WAIT_NOTIFY`.
    AtomicU32WaitNotify,
    /// `target.atomic.U32_CROSS_PROCESS`.
    AtomicU32CrossProcess,
    /// `target.atomic.U64_ALIGNMENT`.
    AtomicU64Alignment,
    /// `target.atomic.U64_ALWAYS_LOCK_FREE`.
    AtomicU64AlwaysLockFree,
    /// `target.atomic.U64_WAIT_NOTIFY`.
    AtomicU64WaitNotify,
    /// `target.atomic.U64_CROSS_PROCESS`.
    AtomicU64CrossProcess,
    /// `target.atomic.U128_ALIGNMENT`.
    AtomicU128Alignment,
    /// `target.atomic.U128_ALWAYS_LOCK_FREE`.
    AtomicU128AlwaysLockFree,
    /// `target.atomic.U128_WAIT_NOTIFY`.
    AtomicU128WaitNotify,
    /// `target.atomic.U128_CROSS_PROCESS`.
    AtomicU128CrossProcess,
    /// `target.atomic.POINTER_ALIGNMENT`.
    AtomicPointerAlignment,
    /// `target.atomic.POINTER_ALWAYS_LOCK_FREE`.
    AtomicPointerAlwaysLockFree,
    /// `target.atomic.POINTER_WAIT_NOTIFY`.
    AtomicPointerWaitNotify,
    /// `target.atomic.POINTER_CROSS_PROCESS`.
    AtomicPointerCrossProcess,
    /// `target.abi.C`.
    AbiC,
    /// `target.abi.SYSTEM`.
    AbiSystem,
    /// `target.c.CHAR`.
    CChar,
    /// `target.c.SIGNED_CHAR`.
    CSignedChar,
    /// `target.c.UNSIGNED_CHAR`.
    CUnsignedChar,
    /// `target.c.SHORT`.
    CShort,
    /// `target.c.UNSIGNED_SHORT`.
    CUnsignedShort,
    /// `target.c.INT`.
    CInt,
    /// `target.c.UNSIGNED_INT`.
    CUnsignedInt,
    /// `target.c.LONG`.
    CLong,
    /// `target.c.UNSIGNED_LONG`.
    CUnsignedLong,
    /// `target.c.LONG_LONG`.
    CLongLong,
    /// `target.c.UNSIGNED_LONG_LONG`.
    CUnsignedLongLong,
    /// `target.c.SIZE`.
    CSize,
    /// `target.c.PTRDIFF`.
    CPointerDifference,
    /// `target.c.WCHAR`.
    CWideChar,
    /// `target.c.BOOL`.
    CBool,
    /// `target.c.FLOAT`.
    CFloat,
    /// `target.c.DOUBLE`.
    CDouble,
    /// `target.c.LONG_DOUBLE`.
    CLongDouble,
    /// `target.address_space.HOST`.
    AddressSpaceHost,
    /// `target.address_space.DEVICE`.
    AddressSpaceDevice,
    /// `target.alignment.MAX_STORAGE`.
    AlignmentMaxStorage,
    /// `target.alignment.MAX_ALLOCATION`.
    AlignmentMaxAllocation,
    /// `target.platform.dynamic_loading`.
    PlatformDynamicLoading,
}

impl TargetPropertyKind {
    /// Every language-defined target property in stable path order.
    pub const ALL: &'static [Self] = &[
        Self::IdentityName,
        Self::IdentityArchitecture,
        Self::IdentityVendor,
        Self::IdentitySystem,
        Self::IdentityEnvironment,
        Self::IdentityAbi,
        Self::PointerBits,
        Self::PointerBytes,
        Self::EndianLittle,
        Self::EndianBig,
        Self::ScalarBool,
        Self::ScalarChar,
        Self::ScalarI8,
        Self::ScalarI16,
        Self::ScalarI32,
        Self::ScalarI64,
        Self::ScalarI128,
        Self::ScalarU8,
        Self::ScalarU16,
        Self::ScalarU32,
        Self::ScalarU64,
        Self::ScalarU128,
        Self::ScalarUsize,
        Self::ScalarIsize,
        Self::ScalarR16,
        Self::ScalarR32,
        Self::ScalarR64,
        Self::ScalarR128,
        Self::ScalarC32,
        Self::ScalarC64,
        Self::ScalarC128,
        Self::ScalarC256,
        Self::AtomicU8,
        Self::AtomicU16,
        Self::AtomicU32,
        Self::AtomicU64,
        Self::AtomicU128,
        Self::AtomicPointer,
        Self::AtomicU8Alignment,
        Self::AtomicU8AlwaysLockFree,
        Self::AtomicU8WaitNotify,
        Self::AtomicU8CrossProcess,
        Self::AtomicU16Alignment,
        Self::AtomicU16AlwaysLockFree,
        Self::AtomicU16WaitNotify,
        Self::AtomicU16CrossProcess,
        Self::AtomicU32Alignment,
        Self::AtomicU32AlwaysLockFree,
        Self::AtomicU32WaitNotify,
        Self::AtomicU32CrossProcess,
        Self::AtomicU64Alignment,
        Self::AtomicU64AlwaysLockFree,
        Self::AtomicU64WaitNotify,
        Self::AtomicU64CrossProcess,
        Self::AtomicU128Alignment,
        Self::AtomicU128AlwaysLockFree,
        Self::AtomicU128WaitNotify,
        Self::AtomicU128CrossProcess,
        Self::AtomicPointerAlignment,
        Self::AtomicPointerAlwaysLockFree,
        Self::AtomicPointerWaitNotify,
        Self::AtomicPointerCrossProcess,
        Self::AbiC,
        Self::AbiSystem,
        Self::CChar,
        Self::CSignedChar,
        Self::CUnsignedChar,
        Self::CShort,
        Self::CUnsignedShort,
        Self::CInt,
        Self::CUnsignedInt,
        Self::CLong,
        Self::CUnsignedLong,
        Self::CLongLong,
        Self::CUnsignedLongLong,
        Self::CSize,
        Self::CPointerDifference,
        Self::CWideChar,
        Self::CBool,
        Self::CFloat,
        Self::CDouble,
        Self::CLongDouble,
        Self::AddressSpaceHost,
        Self::AddressSpaceDevice,
        Self::AlignmentMaxStorage,
        Self::AlignmentMaxAllocation,
        Self::PlatformDynamicLoading,
    ];

    /// Returns the language-defined target-property path.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IdentityName => "target.identity.NAME",
            Self::IdentityArchitecture => "target.identity.ARCH",
            Self::IdentityVendor => "target.identity.VENDOR",
            Self::IdentitySystem => "target.identity.SYSTEM",
            Self::IdentityEnvironment => "target.identity.ENVIRONMENT",
            Self::IdentityAbi => "target.identity.ABI",
            Self::PointerBits => "target.pointer.BITS",
            Self::PointerBytes => "target.pointer.BYTES",
            Self::EndianLittle => "target.endian.LITTLE",
            Self::EndianBig => "target.endian.BIG",
            Self::ScalarBool => "target.scalar.BOOL",
            Self::ScalarChar => "target.scalar.CHAR",
            Self::ScalarI8 => "target.scalar.I8",
            Self::ScalarI16 => "target.scalar.I16",
            Self::ScalarI32 => "target.scalar.I32",
            Self::ScalarI64 => "target.scalar.I64",
            Self::ScalarI128 => "target.scalar.I128",
            Self::ScalarU8 => "target.scalar.U8",
            Self::ScalarU16 => "target.scalar.U16",
            Self::ScalarU32 => "target.scalar.U32",
            Self::ScalarU64 => "target.scalar.U64",
            Self::ScalarU128 => "target.scalar.U128",
            Self::ScalarUsize => "target.scalar.USIZE",
            Self::ScalarIsize => "target.scalar.ISIZE",
            Self::ScalarR16 => "target.scalar.R16",
            Self::ScalarR32 => "target.scalar.R32",
            Self::ScalarR64 => "target.scalar.R64",
            Self::ScalarR128 => "target.scalar.R128",
            Self::ScalarC32 => "target.scalar.C32",
            Self::ScalarC64 => "target.scalar.C64",
            Self::ScalarC128 => "target.scalar.C128",
            Self::ScalarC256 => "target.scalar.C256",
            Self::AtomicU8 => "target.atomic.U8",
            Self::AtomicU16 => "target.atomic.U16",
            Self::AtomicU32 => "target.atomic.U32",
            Self::AtomicU64 => "target.atomic.U64",
            Self::AtomicU128 => "target.atomic.U128",
            Self::AtomicPointer => "target.atomic.POINTER",
            Self::AtomicU8Alignment => "target.atomic.U8_ALIGNMENT",
            Self::AtomicU8AlwaysLockFree => "target.atomic.U8_ALWAYS_LOCK_FREE",
            Self::AtomicU8WaitNotify => "target.atomic.U8_WAIT_NOTIFY",
            Self::AtomicU8CrossProcess => "target.atomic.U8_CROSS_PROCESS",
            Self::AtomicU16Alignment => "target.atomic.U16_ALIGNMENT",
            Self::AtomicU16AlwaysLockFree => "target.atomic.U16_ALWAYS_LOCK_FREE",
            Self::AtomicU16WaitNotify => "target.atomic.U16_WAIT_NOTIFY",
            Self::AtomicU16CrossProcess => "target.atomic.U16_CROSS_PROCESS",
            Self::AtomicU32Alignment => "target.atomic.U32_ALIGNMENT",
            Self::AtomicU32AlwaysLockFree => "target.atomic.U32_ALWAYS_LOCK_FREE",
            Self::AtomicU32WaitNotify => "target.atomic.U32_WAIT_NOTIFY",
            Self::AtomicU32CrossProcess => "target.atomic.U32_CROSS_PROCESS",
            Self::AtomicU64Alignment => "target.atomic.U64_ALIGNMENT",
            Self::AtomicU64AlwaysLockFree => "target.atomic.U64_ALWAYS_LOCK_FREE",
            Self::AtomicU64WaitNotify => "target.atomic.U64_WAIT_NOTIFY",
            Self::AtomicU64CrossProcess => "target.atomic.U64_CROSS_PROCESS",
            Self::AtomicU128Alignment => "target.atomic.U128_ALIGNMENT",
            Self::AtomicU128AlwaysLockFree => "target.atomic.U128_ALWAYS_LOCK_FREE",
            Self::AtomicU128WaitNotify => "target.atomic.U128_WAIT_NOTIFY",
            Self::AtomicU128CrossProcess => "target.atomic.U128_CROSS_PROCESS",
            Self::AtomicPointerAlignment => "target.atomic.POINTER_ALIGNMENT",
            Self::AtomicPointerAlwaysLockFree => "target.atomic.POINTER_ALWAYS_LOCK_FREE",
            Self::AtomicPointerWaitNotify => "target.atomic.POINTER_WAIT_NOTIFY",
            Self::AtomicPointerCrossProcess => "target.atomic.POINTER_CROSS_PROCESS",
            Self::AbiC => "target.abi.C",
            Self::AbiSystem => "target.abi.SYSTEM",
            Self::CChar => "target.c.CHAR",
            Self::CSignedChar => "target.c.SIGNED_CHAR",
            Self::CUnsignedChar => "target.c.UNSIGNED_CHAR",
            Self::CShort => "target.c.SHORT",
            Self::CUnsignedShort => "target.c.UNSIGNED_SHORT",
            Self::CInt => "target.c.INT",
            Self::CUnsignedInt => "target.c.UNSIGNED_INT",
            Self::CLong => "target.c.LONG",
            Self::CUnsignedLong => "target.c.UNSIGNED_LONG",
            Self::CLongLong => "target.c.LONG_LONG",
            Self::CUnsignedLongLong => "target.c.UNSIGNED_LONG_LONG",
            Self::CSize => "target.c.SIZE",
            Self::CPointerDifference => "target.c.PTRDIFF",
            Self::CWideChar => "target.c.WCHAR",
            Self::CBool => "target.c.BOOL",
            Self::CFloat => "target.c.FLOAT",
            Self::CDouble => "target.c.DOUBLE",
            Self::CLongDouble => "target.c.LONG_DOUBLE",
            Self::AddressSpaceHost => "target.address_space.HOST",
            Self::AddressSpaceDevice => "target.address_space.DEVICE",
            Self::AlignmentMaxStorage => "target.alignment.MAX_STORAGE",
            Self::AlignmentMaxAllocation => "target.alignment.MAX_ALLOCATION",
            Self::PlatformDynamicLoading => "target.platform.dynamic_loading",
        }
    }

    /// Finds the language-defined target property with the supplied exact path.
    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.as_str() == path)
    }
}
