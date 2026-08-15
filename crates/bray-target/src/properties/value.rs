use crate::{Endianness, TargetProfile};

use super::{
    TargetAtomicRepresentation, TargetCScalarKind, TargetPropertyKind, TargetScalarKind,
};

/// The typed value of one language-defined target property.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetPropertyValue<'profile> {
    /// A compiler-known string property.
    String(&'profile str),
    /// A compiler-known `usize` property represented independently of the compiler host.
    Usize(u64),
    /// A compiler-known Boolean property.
    Boolean(bool),
}

impl<'profile> TargetPropertyValue<'profile> {
    pub(crate) fn for_profile(profile: &'profile TargetProfile, kind: TargetPropertyKind) -> Self {
        let properties = profile.properties();
        let scalar = |kind| Self::Boolean(properties.scalars().supports(kind));
        let atomic = |representation| properties.atomics().representation(representation);

        let c_scalar = |kind| {
            Self::String(
                properties
                    .c_abi()
                    .mapping(kind)
                    .map_or("unavailable", TargetScalarKind::as_str),
            )
        };

        match kind {
            TargetPropertyKind::IdentityName => Self::String(profile.identity().as_str()),
            TargetPropertyKind::IdentityArchitecture => {
                Self::String(profile.machine().architecture().as_str())
            }
            TargetPropertyKind::IdentityVendor => Self::String(properties.identity().vendor()),
            TargetPropertyKind::IdentitySystem => Self::String(properties.identity().system()),
            TargetPropertyKind::IdentityEnvironment => Self::String(properties.identity().environment()),
            TargetPropertyKind::IdentityAbi => Self::String(properties.identity().abi()),
            TargetPropertyKind::PointerBits => {
                Self::Usize(u64::from(profile.machine().pointer_width_bits().get()))
            }
            TargetPropertyKind::PointerBytes => {
                Self::Usize(u64::from(profile.machine().pointer_width_bits().get() / 8))
            }
            TargetPropertyKind::EndianLittle => {
                Self::Boolean(profile.machine().endianness() == Endianness::Little)
            }
            TargetPropertyKind::EndianBig => {
                Self::Boolean(profile.machine().endianness() == Endianness::Big)
            }
            TargetPropertyKind::ScalarBool => scalar(TargetScalarKind::Bool),
            TargetPropertyKind::ScalarChar => scalar(TargetScalarKind::Char),
            TargetPropertyKind::ScalarI8 => scalar(TargetScalarKind::I8),
            TargetPropertyKind::ScalarI16 => scalar(TargetScalarKind::I16),
            TargetPropertyKind::ScalarI32 => scalar(TargetScalarKind::I32),
            TargetPropertyKind::ScalarI64 => scalar(TargetScalarKind::I64),
            TargetPropertyKind::ScalarI128 => scalar(TargetScalarKind::I128),
            TargetPropertyKind::ScalarU8 => scalar(TargetScalarKind::U8),
            TargetPropertyKind::ScalarU16 => scalar(TargetScalarKind::U16),
            TargetPropertyKind::ScalarU32 => scalar(TargetScalarKind::U32),
            TargetPropertyKind::ScalarU64 => scalar(TargetScalarKind::U64),
            TargetPropertyKind::ScalarU128 => scalar(TargetScalarKind::U128),
            TargetPropertyKind::ScalarUsize => scalar(TargetScalarKind::Usize),
            TargetPropertyKind::ScalarIsize => scalar(TargetScalarKind::Isize),
            TargetPropertyKind::ScalarR16 => scalar(TargetScalarKind::R16),
            TargetPropertyKind::ScalarR32 => scalar(TargetScalarKind::R32),
            TargetPropertyKind::ScalarR64 => scalar(TargetScalarKind::R64),
            TargetPropertyKind::ScalarR128 => scalar(TargetScalarKind::R128),
            TargetPropertyKind::ScalarC32 => scalar(TargetScalarKind::C32),
            TargetPropertyKind::ScalarC64 => scalar(TargetScalarKind::C64),
            TargetPropertyKind::ScalarC128 => scalar(TargetScalarKind::C128),
            TargetPropertyKind::ScalarC256 => scalar(TargetScalarKind::C256),
            TargetPropertyKind::AtomicU8 => Self::Boolean(properties.atomics().u8()),
            TargetPropertyKind::AtomicU16 => Self::Boolean(properties.atomics().u16()),
            TargetPropertyKind::AtomicU32 => Self::Boolean(properties.atomics().u32()),
            TargetPropertyKind::AtomicU64 => Self::Boolean(properties.atomics().u64()),
            TargetPropertyKind::AtomicU128 => Self::Boolean(properties.atomics().u128()),
            TargetPropertyKind::AtomicPointer => Self::Boolean(properties.atomics().pointer()),
            TargetPropertyKind::AtomicU8Alignment => Self::Usize(
                atomic(TargetAtomicRepresentation::U8)
                    .required_alignment()
                    .get(),
            ),
            TargetPropertyKind::AtomicU8AlwaysLockFree => Self::Boolean(
                atomic(TargetAtomicRepresentation::U8).always_lock_free(),
            ),
            TargetPropertyKind::AtomicU8WaitNotify => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U8).wait_notify())
            }
            TargetPropertyKind::AtomicU8CrossProcess => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U8).cross_process())
            }
            TargetPropertyKind::AtomicU16Alignment => Self::Usize(
                atomic(TargetAtomicRepresentation::U16)
                    .required_alignment()
                    .get(),
            ),
            TargetPropertyKind::AtomicU16AlwaysLockFree => Self::Boolean(
                atomic(TargetAtomicRepresentation::U16).always_lock_free(),
            ),
            TargetPropertyKind::AtomicU16WaitNotify => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U16).wait_notify())
            }
            TargetPropertyKind::AtomicU16CrossProcess => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U16).cross_process())
            }
            TargetPropertyKind::AtomicU32Alignment => Self::Usize(
                atomic(TargetAtomicRepresentation::U32)
                    .required_alignment()
                    .get(),
            ),
            TargetPropertyKind::AtomicU32AlwaysLockFree => Self::Boolean(
                atomic(TargetAtomicRepresentation::U32).always_lock_free(),
            ),
            TargetPropertyKind::AtomicU32WaitNotify => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U32).wait_notify())
            }
            TargetPropertyKind::AtomicU32CrossProcess => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U32).cross_process())
            }
            TargetPropertyKind::AtomicU64Alignment => Self::Usize(
                atomic(TargetAtomicRepresentation::U64)
                    .required_alignment()
                    .get(),
            ),
            TargetPropertyKind::AtomicU64AlwaysLockFree => Self::Boolean(
                atomic(TargetAtomicRepresentation::U64).always_lock_free(),
            ),
            TargetPropertyKind::AtomicU64WaitNotify => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U64).wait_notify())
            }
            TargetPropertyKind::AtomicU64CrossProcess => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U64).cross_process())
            }
            TargetPropertyKind::AtomicU128Alignment => Self::Usize(
                atomic(TargetAtomicRepresentation::U128)
                    .required_alignment()
                    .get(),
            ),
            TargetPropertyKind::AtomicU128AlwaysLockFree => Self::Boolean(
                atomic(TargetAtomicRepresentation::U128).always_lock_free(),
            ),
            TargetPropertyKind::AtomicU128WaitNotify => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U128).wait_notify())
            }
            TargetPropertyKind::AtomicU128CrossProcess => {
                Self::Boolean(atomic(TargetAtomicRepresentation::U128).cross_process())
            }
            TargetPropertyKind::AtomicPointerAlignment => Self::Usize(
                atomic(TargetAtomicRepresentation::Pointer)
                    .required_alignment()
                    .get(),
            ),
            TargetPropertyKind::AtomicPointerAlwaysLockFree => Self::Boolean(
                atomic(TargetAtomicRepresentation::Pointer).always_lock_free(),
            ),
            TargetPropertyKind::AtomicPointerWaitNotify => {
                Self::Boolean(atomic(TargetAtomicRepresentation::Pointer).wait_notify())
            }
            TargetPropertyKind::AtomicPointerCrossProcess => {
                Self::Boolean(atomic(TargetAtomicRepresentation::Pointer).cross_process())
            }
            TargetPropertyKind::AbiC => Self::Boolean(properties.abis().c()),
            TargetPropertyKind::AbiSystem => Self::Boolean(properties.abis().system()),
            TargetPropertyKind::CChar => c_scalar(TargetCScalarKind::Char),
            TargetPropertyKind::CSignedChar => c_scalar(TargetCScalarKind::SignedChar),
            TargetPropertyKind::CUnsignedChar => c_scalar(TargetCScalarKind::UnsignedChar),
            TargetPropertyKind::CShort => c_scalar(TargetCScalarKind::Short),
            TargetPropertyKind::CUnsignedShort => c_scalar(TargetCScalarKind::UnsignedShort),
            TargetPropertyKind::CInt => c_scalar(TargetCScalarKind::Int),
            TargetPropertyKind::CUnsignedInt => c_scalar(TargetCScalarKind::UnsignedInt),
            TargetPropertyKind::CLong => c_scalar(TargetCScalarKind::Long),
            TargetPropertyKind::CUnsignedLong => c_scalar(TargetCScalarKind::UnsignedLong),
            TargetPropertyKind::CLongLong => c_scalar(TargetCScalarKind::LongLong),
            TargetPropertyKind::CUnsignedLongLong => c_scalar(TargetCScalarKind::UnsignedLongLong),
            TargetPropertyKind::CSize => c_scalar(TargetCScalarKind::Size),
            TargetPropertyKind::CPointerDifference => c_scalar(TargetCScalarKind::PointerDifference),
            TargetPropertyKind::CWideChar => c_scalar(TargetCScalarKind::WideChar),
            TargetPropertyKind::CBool => c_scalar(TargetCScalarKind::Bool),
            TargetPropertyKind::CFloat => c_scalar(TargetCScalarKind::Float),
            TargetPropertyKind::CDouble => c_scalar(TargetCScalarKind::Double),
            TargetPropertyKind::CLongDouble => c_scalar(TargetCScalarKind::LongDouble),
            TargetPropertyKind::AddressSpaceHost => Self::Boolean(properties.address_spaces().host()),
            TargetPropertyKind::AddressSpaceDevice => Self::Boolean(properties.address_spaces().device()),
            TargetPropertyKind::AlignmentMaxStorage => {
                Self::Usize(properties.alignments().max_storage().get())
            }
            TargetPropertyKind::AlignmentMaxAllocation => {
                Self::Usize(properties.alignments().max_allocation().get())
            }
            TargetPropertyKind::PlatformDynamicLoading => Self::Boolean(properties.dynamic_loading()),
        }
    }
}
