use crate::{Endianness, TargetProfile};

use super::{TargetCScalarKind, TargetFactKind, TargetScalarKind};

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
        let scalar = |kind| Self::Boolean(facts.scalars().supports(kind));

        let c_scalar = |kind| {
            Self::String(
                facts
                    .c_abi()
                    .mapping(kind)
                    .map_or("unavailable", TargetScalarKind::as_str),
            )
        };

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
            TargetFactKind::ScalarBool => scalar(TargetScalarKind::Bool),
            TargetFactKind::ScalarChar => scalar(TargetScalarKind::Char),
            TargetFactKind::ScalarI8 => scalar(TargetScalarKind::I8),
            TargetFactKind::ScalarI16 => scalar(TargetScalarKind::I16),
            TargetFactKind::ScalarI32 => scalar(TargetScalarKind::I32),
            TargetFactKind::ScalarI64 => scalar(TargetScalarKind::I64),
            TargetFactKind::ScalarI128 => scalar(TargetScalarKind::I128),
            TargetFactKind::ScalarU8 => scalar(TargetScalarKind::U8),
            TargetFactKind::ScalarU16 => scalar(TargetScalarKind::U16),
            TargetFactKind::ScalarU32 => scalar(TargetScalarKind::U32),
            TargetFactKind::ScalarU64 => scalar(TargetScalarKind::U64),
            TargetFactKind::ScalarU128 => scalar(TargetScalarKind::U128),
            TargetFactKind::ScalarUsize => scalar(TargetScalarKind::Usize),
            TargetFactKind::ScalarIsize => scalar(TargetScalarKind::Isize),
            TargetFactKind::ScalarR16 => scalar(TargetScalarKind::R16),
            TargetFactKind::ScalarR32 => scalar(TargetScalarKind::R32),
            TargetFactKind::ScalarR64 => scalar(TargetScalarKind::R64),
            TargetFactKind::ScalarR128 => scalar(TargetScalarKind::R128),
            TargetFactKind::ScalarC32 => scalar(TargetScalarKind::C32),
            TargetFactKind::ScalarC64 => scalar(TargetScalarKind::C64),
            TargetFactKind::ScalarC128 => scalar(TargetScalarKind::C128),
            TargetFactKind::ScalarC256 => scalar(TargetScalarKind::C256),
            TargetFactKind::AtomicU8 => Self::Boolean(facts.atomics().u8()),
            TargetFactKind::AtomicU16 => Self::Boolean(facts.atomics().u16()),
            TargetFactKind::AtomicU32 => Self::Boolean(facts.atomics().u32()),
            TargetFactKind::AtomicU64 => Self::Boolean(facts.atomics().u64()),
            TargetFactKind::AtomicU128 => Self::Boolean(facts.atomics().u128()),
            TargetFactKind::AtomicPointer => Self::Boolean(facts.atomics().pointer()),
            TargetFactKind::AbiC => Self::Boolean(facts.abis().c()),
            TargetFactKind::AbiSystem => Self::Boolean(facts.abis().system()),
            TargetFactKind::CChar => c_scalar(TargetCScalarKind::Char),
            TargetFactKind::CSignedChar => c_scalar(TargetCScalarKind::SignedChar),
            TargetFactKind::CUnsignedChar => c_scalar(TargetCScalarKind::UnsignedChar),
            TargetFactKind::CShort => c_scalar(TargetCScalarKind::Short),
            TargetFactKind::CUnsignedShort => c_scalar(TargetCScalarKind::UnsignedShort),
            TargetFactKind::CInt => c_scalar(TargetCScalarKind::Int),
            TargetFactKind::CUnsignedInt => c_scalar(TargetCScalarKind::UnsignedInt),
            TargetFactKind::CLong => c_scalar(TargetCScalarKind::Long),
            TargetFactKind::CUnsignedLong => c_scalar(TargetCScalarKind::UnsignedLong),
            TargetFactKind::CLongLong => c_scalar(TargetCScalarKind::LongLong),
            TargetFactKind::CUnsignedLongLong => {
                c_scalar(TargetCScalarKind::UnsignedLongLong)
            }
            TargetFactKind::CSize => c_scalar(TargetCScalarKind::Size),
            TargetFactKind::CPointerDifference => {
                c_scalar(TargetCScalarKind::PointerDifference)
            }
            TargetFactKind::CWideChar => c_scalar(TargetCScalarKind::WideChar),
            TargetFactKind::CBool => c_scalar(TargetCScalarKind::Bool),
            TargetFactKind::CFloat => c_scalar(TargetCScalarKind::Float),
            TargetFactKind::CDouble => c_scalar(TargetCScalarKind::Double),
            TargetFactKind::CLongDouble => c_scalar(TargetCScalarKind::LongDouble),
            TargetFactKind::AddressSpaceHost => Self::Boolean(facts.address_spaces().host()),
            TargetFactKind::AddressSpaceDevice => Self::Boolean(facts.address_spaces().device()),
            TargetFactKind::AlignmentMaxStorage => {
                Self::Usize(facts.alignments().max_storage().get())
            }
            TargetFactKind::AlignmentMaxAllocation => {
                Self::Usize(facts.alignments().max_allocation().get())
            }
            TargetFactKind::PlatformDynamicLoading => Self::Boolean(facts.dynamic_loading()),
        }
    }
}
