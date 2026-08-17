use std::collections::BTreeMap;

use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_target::TargetPropertyKind;

use super::super::CompilerKnownSymbolBuildError;
use crate::{AnySymbolId, ConstantSymbolId, ExactSymbolId};

pub(super) struct TargetPropertySymbols {
    pub(super) properties: BTreeMap<TargetPropertyKind, ConstantSymbolId>,
    pub(super) symbols: BTreeMap<ConstantSymbolId, TargetPropertyKind>,
}

pub(super) fn build_target_properties(
    declarations: &BTreeMap<CompilerKnownDeclarationKey, AnySymbolId>,
) -> Result<TargetPropertySymbols, CompilerKnownSymbolBuildError> {
    let mut properties = BTreeMap::new();
    let mut symbols = BTreeMap::new();

    for &property in TargetPropertyKind::ALL {
        let key = CompilerKnownDeclarationKey::try_new(target_property_catalog_key(property))
            .ok_or(CompilerKnownSymbolBuildError::MissingTargetProperty { property })?;

        let symbol = declarations
            .get(&key)
            .copied()
            .and_then(ConstantSymbolId::try_from_any)
            .ok_or(CompilerKnownSymbolBuildError::MissingTargetProperty { property })?;

        properties.insert(property, symbol);
        symbols.insert(symbol, property);
    }

    Ok(TargetPropertySymbols {
        properties,
        symbols,
    })
}

const fn target_property_catalog_key(property: TargetPropertyKind) -> &'static str {
    match property {
        TargetPropertyKind::IdentityName => "TargetIdentityName",
        TargetPropertyKind::IdentityArchitecture => "TargetIdentityArchitecture",
        TargetPropertyKind::IdentityVendor => "TargetIdentityVendor",
        TargetPropertyKind::IdentitySystem => "TargetIdentitySystem",
        TargetPropertyKind::IdentityEnvironment => "TargetIdentityEnvironment",
        TargetPropertyKind::IdentityAbi => "TargetIdentityAbi",
        TargetPropertyKind::PointerBits => "TargetPointerBits",
        TargetPropertyKind::PointerBytes => "TargetPointerBytes",
        TargetPropertyKind::EndianLittle => "TargetEndianLittle",
        TargetPropertyKind::EndianBig => "TargetEndianBig",
        TargetPropertyKind::ScalarBool => "TargetScalarBool",
        TargetPropertyKind::ScalarChar => "TargetScalarChar",
        TargetPropertyKind::ScalarI8 => "TargetScalarI8",
        TargetPropertyKind::ScalarI16 => "TargetScalarI16",
        TargetPropertyKind::ScalarI32 => "TargetScalarI32",
        TargetPropertyKind::ScalarI64 => "TargetScalarI64",
        TargetPropertyKind::ScalarI128 => "TargetScalarI128",
        TargetPropertyKind::ScalarU8 => "TargetScalarU8",
        TargetPropertyKind::ScalarU16 => "TargetScalarU16",
        TargetPropertyKind::ScalarU32 => "TargetScalarU32",
        TargetPropertyKind::ScalarU64 => "TargetScalarU64",
        TargetPropertyKind::ScalarU128 => "TargetScalarU128",
        TargetPropertyKind::ScalarUsize => "TargetScalarUsize",
        TargetPropertyKind::ScalarIsize => "TargetScalarIsize",
        TargetPropertyKind::ScalarR16 => "TargetScalarR16",
        TargetPropertyKind::ScalarR32 => "TargetScalarR32",
        TargetPropertyKind::ScalarR64 => "TargetScalarR64",
        TargetPropertyKind::ScalarR128 => "TargetScalarR128",
        TargetPropertyKind::ScalarC32 => "TargetScalarC32",
        TargetPropertyKind::ScalarC64 => "TargetScalarC64",
        TargetPropertyKind::ScalarC128 => "TargetScalarC128",
        TargetPropertyKind::ScalarC256 => "TargetScalarC256",
        TargetPropertyKind::AtomicU8 => "TargetAtomicU8",
        TargetPropertyKind::AtomicU16 => "TargetAtomicU16",
        TargetPropertyKind::AtomicU32 => "TargetAtomicU32",
        TargetPropertyKind::AtomicU64 => "TargetAtomicU64",
        TargetPropertyKind::AtomicU128 => "TargetAtomicU128",
        TargetPropertyKind::AtomicPointer => "TargetAtomicPointer",
        TargetPropertyKind::AtomicU8Alignment => "TargetAtomicU8Alignment",
        TargetPropertyKind::AtomicU8AlwaysLockFree => "TargetAtomicU8AlwaysLockFree",
        TargetPropertyKind::AtomicU8WaitNotify => "TargetAtomicU8WaitNotify",
        TargetPropertyKind::AtomicU8CrossProcess => "TargetAtomicU8CrossProcess",
        TargetPropertyKind::AtomicU16Alignment => "TargetAtomicU16Alignment",
        TargetPropertyKind::AtomicU16AlwaysLockFree => "TargetAtomicU16AlwaysLockFree",
        TargetPropertyKind::AtomicU16WaitNotify => "TargetAtomicU16WaitNotify",
        TargetPropertyKind::AtomicU16CrossProcess => "TargetAtomicU16CrossProcess",
        TargetPropertyKind::AtomicU32Alignment => "TargetAtomicU32Alignment",
        TargetPropertyKind::AtomicU32AlwaysLockFree => "TargetAtomicU32AlwaysLockFree",
        TargetPropertyKind::AtomicU32WaitNotify => "TargetAtomicU32WaitNotify",
        TargetPropertyKind::AtomicU32CrossProcess => "TargetAtomicU32CrossProcess",
        TargetPropertyKind::AtomicU64Alignment => "TargetAtomicU64Alignment",
        TargetPropertyKind::AtomicU64AlwaysLockFree => "TargetAtomicU64AlwaysLockFree",
        TargetPropertyKind::AtomicU64WaitNotify => "TargetAtomicU64WaitNotify",
        TargetPropertyKind::AtomicU64CrossProcess => "TargetAtomicU64CrossProcess",
        TargetPropertyKind::AtomicU128Alignment => "TargetAtomicU128Alignment",
        TargetPropertyKind::AtomicU128AlwaysLockFree => "TargetAtomicU128AlwaysLockFree",
        TargetPropertyKind::AtomicU128WaitNotify => "TargetAtomicU128WaitNotify",
        TargetPropertyKind::AtomicU128CrossProcess => "TargetAtomicU128CrossProcess",
        TargetPropertyKind::AtomicPointerAlignment => "TargetAtomicPointerAlignment",
        TargetPropertyKind::AtomicPointerAlwaysLockFree => "TargetAtomicPointerAlwaysLockFree",
        TargetPropertyKind::AtomicPointerWaitNotify => "TargetAtomicPointerWaitNotify",
        TargetPropertyKind::AtomicPointerCrossProcess => "TargetAtomicPointerCrossProcess",
        TargetPropertyKind::AbiC => "TargetAbiC",
        TargetPropertyKind::AbiSystem => "TargetAbiSystem",
        TargetPropertyKind::CChar => "TargetCChar",
        TargetPropertyKind::CSignedChar => "TargetCSignedChar",
        TargetPropertyKind::CUnsignedChar => "TargetCUnsignedChar",
        TargetPropertyKind::CShort => "TargetCShort",
        TargetPropertyKind::CUnsignedShort => "TargetCUnsignedShort",
        TargetPropertyKind::CInt => "TargetCInt",
        TargetPropertyKind::CUnsignedInt => "TargetCUnsignedInt",
        TargetPropertyKind::CLong => "TargetCLong",
        TargetPropertyKind::CUnsignedLong => "TargetCUnsignedLong",
        TargetPropertyKind::CLongLong => "TargetCLongLong",
        TargetPropertyKind::CUnsignedLongLong => "TargetCUnsignedLongLong",
        TargetPropertyKind::CSize => "TargetCSize",
        TargetPropertyKind::CPointerDifference => "TargetCPointerDifference",
        TargetPropertyKind::CWideChar => "TargetCWideChar",
        TargetPropertyKind::CBool => "TargetCBool",
        TargetPropertyKind::CFloat => "TargetCFloat",
        TargetPropertyKind::CDouble => "TargetCDouble",
        TargetPropertyKind::CLongDouble => "TargetCLongDouble",
        TargetPropertyKind::AddressSpaceHost => "TargetAddressSpaceHost",
        TargetPropertyKind::AddressSpaceDevice => "TargetAddressSpaceDevice",
        TargetPropertyKind::AlignmentMaxStorage => "TargetAlignmentMaxStorage",
        TargetPropertyKind::AlignmentMaxAllocation => "TargetAlignmentMaxAllocation",
        TargetPropertyKind::PlatformDynamicLoading => "TargetPlatformDynamicLoading",
        TargetPropertyKind::PlatformNativeThreads => "TargetPlatformNativeThreads",
    }
}
