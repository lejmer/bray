use std::collections::BTreeMap;

use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_target::TargetFactKind;

use super::super::CompilerKnownSymbolBuildError;
use crate::{AnySymbolId, ConstantSymbolId, ExactSymbolId};

pub(super) struct TargetFactSymbols {
    pub(super) facts: BTreeMap<TargetFactKind, ConstantSymbolId>,
    pub(super) symbols: BTreeMap<ConstantSymbolId, TargetFactKind>,
}

pub(super) fn build_target_facts(
    declarations: &BTreeMap<CompilerKnownDeclarationKey, AnySymbolId>,
) -> Result<TargetFactSymbols, CompilerKnownSymbolBuildError> {
    let mut facts = BTreeMap::new();
    let mut symbols = BTreeMap::new();

    for &fact in TargetFactKind::ALL {
        let key = CompilerKnownDeclarationKey::try_new(target_fact_catalog_key(fact))
            .ok_or(CompilerKnownSymbolBuildError::MissingTargetFact { fact })?;

        let symbol = declarations
            .get(&key)
            .copied()
            .and_then(ConstantSymbolId::try_from_any)
            .ok_or(CompilerKnownSymbolBuildError::MissingTargetFact { fact })?;

        facts.insert(fact, symbol);
        symbols.insert(symbol, fact);
    }

    Ok(TargetFactSymbols { facts, symbols })
}

const fn target_fact_catalog_key(fact: TargetFactKind) -> &'static str {
    match fact {
        TargetFactKind::IdentityName => "TargetIdentityName",
        TargetFactKind::IdentityArchitecture => "TargetIdentityArchitecture",
        TargetFactKind::IdentityVendor => "TargetIdentityVendor",
        TargetFactKind::IdentitySystem => "TargetIdentitySystem",
        TargetFactKind::IdentityEnvironment => "TargetIdentityEnvironment",
        TargetFactKind::IdentityAbi => "TargetIdentityAbi",
        TargetFactKind::PointerBits => "TargetPointerBits",
        TargetFactKind::PointerBytes => "TargetPointerBytes",
        TargetFactKind::EndianLittle => "TargetEndianLittle",
        TargetFactKind::EndianBig => "TargetEndianBig",
        TargetFactKind::ScalarBool => "TargetScalarBool",
        TargetFactKind::ScalarChar => "TargetScalarChar",
        TargetFactKind::ScalarI8 => "TargetScalarI8",
        TargetFactKind::ScalarI16 => "TargetScalarI16",
        TargetFactKind::ScalarI32 => "TargetScalarI32",
        TargetFactKind::ScalarI64 => "TargetScalarI64",
        TargetFactKind::ScalarI128 => "TargetScalarI128",
        TargetFactKind::ScalarU8 => "TargetScalarU8",
        TargetFactKind::ScalarU16 => "TargetScalarU16",
        TargetFactKind::ScalarU32 => "TargetScalarU32",
        TargetFactKind::ScalarU64 => "TargetScalarU64",
        TargetFactKind::ScalarU128 => "TargetScalarU128",
        TargetFactKind::ScalarUsize => "TargetScalarUsize",
        TargetFactKind::ScalarIsize => "TargetScalarIsize",
        TargetFactKind::ScalarR16 => "TargetScalarR16",
        TargetFactKind::ScalarR32 => "TargetScalarR32",
        TargetFactKind::ScalarR64 => "TargetScalarR64",
        TargetFactKind::ScalarR128 => "TargetScalarR128",
        TargetFactKind::ScalarC32 => "TargetScalarC32",
        TargetFactKind::ScalarC64 => "TargetScalarC64",
        TargetFactKind::ScalarC128 => "TargetScalarC128",
        TargetFactKind::ScalarC256 => "TargetScalarC256",
        TargetFactKind::AtomicU8 => "TargetAtomicU8",
        TargetFactKind::AtomicU16 => "TargetAtomicU16",
        TargetFactKind::AtomicU32 => "TargetAtomicU32",
        TargetFactKind::AtomicU64 => "TargetAtomicU64",
        TargetFactKind::AtomicU128 => "TargetAtomicU128",
        TargetFactKind::AtomicPointer => "TargetAtomicPointer",
        TargetFactKind::AtomicU8Alignment => "TargetAtomicU8Alignment",
        TargetFactKind::AtomicU8AlwaysLockFree => "TargetAtomicU8AlwaysLockFree",
        TargetFactKind::AtomicU8WaitNotify => "TargetAtomicU8WaitNotify",
        TargetFactKind::AtomicU8CrossProcess => "TargetAtomicU8CrossProcess",
        TargetFactKind::AtomicU16Alignment => "TargetAtomicU16Alignment",
        TargetFactKind::AtomicU16AlwaysLockFree => "TargetAtomicU16AlwaysLockFree",
        TargetFactKind::AtomicU16WaitNotify => "TargetAtomicU16WaitNotify",
        TargetFactKind::AtomicU16CrossProcess => "TargetAtomicU16CrossProcess",
        TargetFactKind::AtomicU32Alignment => "TargetAtomicU32Alignment",
        TargetFactKind::AtomicU32AlwaysLockFree => "TargetAtomicU32AlwaysLockFree",
        TargetFactKind::AtomicU32WaitNotify => "TargetAtomicU32WaitNotify",
        TargetFactKind::AtomicU32CrossProcess => "TargetAtomicU32CrossProcess",
        TargetFactKind::AtomicU64Alignment => "TargetAtomicU64Alignment",
        TargetFactKind::AtomicU64AlwaysLockFree => "TargetAtomicU64AlwaysLockFree",
        TargetFactKind::AtomicU64WaitNotify => "TargetAtomicU64WaitNotify",
        TargetFactKind::AtomicU64CrossProcess => "TargetAtomicU64CrossProcess",
        TargetFactKind::AtomicU128Alignment => "TargetAtomicU128Alignment",
        TargetFactKind::AtomicU128AlwaysLockFree => "TargetAtomicU128AlwaysLockFree",
        TargetFactKind::AtomicU128WaitNotify => "TargetAtomicU128WaitNotify",
        TargetFactKind::AtomicU128CrossProcess => "TargetAtomicU128CrossProcess",
        TargetFactKind::AtomicPointerAlignment => "TargetAtomicPointerAlignment",
        TargetFactKind::AtomicPointerAlwaysLockFree => "TargetAtomicPointerAlwaysLockFree",
        TargetFactKind::AtomicPointerWaitNotify => "TargetAtomicPointerWaitNotify",
        TargetFactKind::AtomicPointerCrossProcess => "TargetAtomicPointerCrossProcess",
        TargetFactKind::AbiC => "TargetAbiC",
        TargetFactKind::AbiSystem => "TargetAbiSystem",
        TargetFactKind::CChar => "TargetCChar",
        TargetFactKind::CSignedChar => "TargetCSignedChar",
        TargetFactKind::CUnsignedChar => "TargetCUnsignedChar",
        TargetFactKind::CShort => "TargetCShort",
        TargetFactKind::CUnsignedShort => "TargetCUnsignedShort",
        TargetFactKind::CInt => "TargetCInt",
        TargetFactKind::CUnsignedInt => "TargetCUnsignedInt",
        TargetFactKind::CLong => "TargetCLong",
        TargetFactKind::CUnsignedLong => "TargetCUnsignedLong",
        TargetFactKind::CLongLong => "TargetCLongLong",
        TargetFactKind::CUnsignedLongLong => "TargetCUnsignedLongLong",
        TargetFactKind::CSize => "TargetCSize",
        TargetFactKind::CPointerDifference => "TargetCPointerDifference",
        TargetFactKind::CWideChar => "TargetCWideChar",
        TargetFactKind::CBool => "TargetCBool",
        TargetFactKind::CFloat => "TargetCFloat",
        TargetFactKind::CDouble => "TargetCDouble",
        TargetFactKind::CLongDouble => "TargetCLongDouble",
        TargetFactKind::AddressSpaceHost => "TargetAddressSpaceHost",
        TargetFactKind::AddressSpaceDevice => "TargetAddressSpaceDevice",
        TargetFactKind::AlignmentMaxStorage => "TargetAlignmentMaxStorage",
        TargetFactKind::AlignmentMaxAllocation => "TargetAlignmentMaxAllocation",
        TargetFactKind::PlatformDynamicLoading => "TargetPlatformDynamicLoading",
    }
}
