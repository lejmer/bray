use std::num::NonZeroU16;

use bray_runtime_interface::PanicAbiIdentity;
use bray_symbols::CallableAbi;
use bray_target::{CodeModel, NativeTarget, RelocationModel, TargetScalarKind as FactScalarKind};

use super::{
    CallableAbiMapping, CodegenLinkage, CodegenTarget, TargetAbi, TargetAddressSpace,
    TargetAddressSpaceKind, TargetCallingConvention, TargetCompatibility, TargetContract,
    TargetDataLayout, TargetMachineSelection, TargetScalarKind, TargetScalarLayout,
    TargetSymbolConvention,
};

impl CodegenTarget {
    /// Returns the complete backend-neutral contract for one native toolchain target.
    pub fn for_native(target: NativeTarget) -> Self {
        let profile = target.profile();

        let contract = TargetContract::new(
            data_layout(&profile),
            target_abi(),
            panic_abi(),
            symbol_convention(),
            compatibility(target),
        );

        let selection = TargetMachineSelection::try_new(
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            target.cpu(),
            std::iter::empty::<&str>(),
        )
        .unwrap_or_else(|error| panic!("native target machine selection must be valid: {error:?}"));

        Self::try_new(profile, target.as_str(), contract, selection)
            .unwrap_or_else(|error| panic!("native codegen target must be valid: {error:?}"))
    }
}

fn data_layout(profile: &bray_target::TargetProfile) -> TargetDataLayout {
    let facts = profile.facts().scalars();
    let integer_widths = [8_u16, 16, 32, 64, 128];
    let float_widths = [16_u16, 32, 64, 128];

    let scalars = std::iter::once(scalar_layout(
        TargetScalarKind::Boolean,
        1,
        facts.alignment(FactScalarKind::Bool).get(),
    ))
    .chain(integer_widths.into_iter().map(|width| {
        scalar_layout(
            TargetScalarKind::Integer(nonzero(width)),
            width.div_ceil(8),
            facts.alignment(integer_fact_kind(width)).get(),
        )
    }))
    .chain(float_widths.into_iter().map(|width| {
        scalar_layout(
            TargetScalarKind::Float(nonzero(width)),
            width.div_ceil(8),
            facts.alignment(float_fact_kind(width)).get(),
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
        profile.machine().pointer_alignment_bytes(),
        address_spaces,
    )
    .unwrap_or_else(|error| panic!("native target data layout must be valid: {error:?}"))
}

fn target_abi() -> TargetAbi {
    let mappings = [
        (CallableAbi::Bray, "c"),
        (CallableAbi::C, "c"),
        (CallableAbi::System, "c"),
    ]
    .into_iter()
    .map(|(abi, name)| {
        let convention = TargetCallingConvention::try_new(name)
            .unwrap_or_else(|| panic!("native calling convention must be valid"));

        CallableAbiMapping::new(abi, convention)
    });

    TargetAbi::try_new(mappings)
        .unwrap_or_else(|error| panic!("native target ABI must be valid: {error:?}"))
}

fn panic_abi() -> PanicAbiIdentity {
    PanicAbiIdentity::try_new("bray.panic.unwind")
        .unwrap_or_else(|| panic!("native panic ABI identity must be valid"))
}

fn symbol_convention() -> TargetSymbolConvention {
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
    .unwrap_or_else(|error| panic!("native target symbol convention must be valid: {error:?}"))
}

fn compatibility(target: NativeTarget) -> TargetCompatibility {
    let revision = format!(
        "bray-{}-{}",
        target.architecture().as_str(),
        target.object_format().as_str()
    );

    TargetCompatibility::try_new(revision, 1, [target.object_format().as_str()])
        .unwrap_or_else(|| panic!("native target compatibility must be valid"))
}

fn scalar_layout(
    kind: TargetScalarKind,
    size_bytes: u16,
    alignment_bytes: u64,
) -> TargetScalarLayout {
    let alignment_bytes = u16::try_from(alignment_bytes)
        .ok()
        .and_then(NonZeroU16::new)
        .unwrap_or_else(|| panic!("native scalar alignment must fit in a nonzero u16"));

    TargetScalarLayout::try_new(kind, nonzero(size_bytes), alignment_bytes)
        .unwrap_or_else(|error| panic!("native scalar layout must be valid: {error:?}"))
}

const fn integer_fact_kind(width: u16) -> FactScalarKind {
    match width {
        8 => FactScalarKind::I8,
        16 => FactScalarKind::I16,
        32 => FactScalarKind::I32,
        64 => FactScalarKind::I64,
        128 => FactScalarKind::I128,
        _ => panic!("native integer width must be supported"),
    }
}

const fn float_fact_kind(width: u16) -> FactScalarKind {
    match width {
        16 => FactScalarKind::R16,
        32 => FactScalarKind::R32,
        64 => FactScalarKind::R64,
        128 => FactScalarKind::R128,
        _ => panic!("native float width must be supported"),
    }
}

fn nonzero(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap_or_else(|| panic!("native scalar width must be nonzero"))
}

#[cfg(test)]
mod tests {
    use bray_symbols::CallableAbi;
    use bray_target::NativeTarget;

    use super::CodegenTarget;

    #[test]
    fn native_codegen_contracts_cover_every_native_target() {
        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);

            assert_eq!(target.identity().as_str(), native.as_str());
            assert_eq!(target.triple(), native.as_str());
            assert_eq!(target.machine().architecture(), native.architecture());
            assert_eq!(target.machine().object_format(), native.object_format());
            assert_eq!(target.cpu(), native.cpu());

            assert_eq!(
                target
                    .abi()
                    .convention(CallableAbi::Bray)
                    .map(|convention| convention.as_str()),
                Some("c")
            );

            assert_eq!(
                target
                    .abi()
                    .convention(CallableAbi::C)
                    .map(|convention| convention.as_str()),
                Some("c")
            );

            assert_eq!(
                target
                    .abi()
                    .convention(CallableAbi::System)
                    .map(|convention| convention.as_str()),
                Some("c")
            );
        }
    }

    #[test]
    fn native_codegen_contracts_are_reproducible() {
        for native in NativeTarget::ALL {
            assert_eq!(
                CodegenTarget::for_native(native),
                CodegenTarget::for_native(native)
            );
        }
    }
}
