use std::collections::BTreeMap;
use std::num::NonZeroU16;

use bray_codegen::{
    CodegenAbiPiece, CodegenAbiScalar, CodegenAggregateCoercion, CodegenIndirectParameterKind,
    CodegenTarget, CodegenTypeKind, CodegenTypeMapping,
};
use bray_symbols::{CallableAbi, TypeId};
use bray_target::TargetValueLayout;

use super::layout::pointer_layout;

pub(in crate::compilation::product::realization) fn is_aggregate(kind: &CodegenTypeKind) -> bool {
    matches!(
        kind,
        CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. }
    )
}

pub(in crate::compilation::product::realization) fn windows_x64_abi(
    abi: CallableAbi,
    target: &CodegenTarget,
) -> bool {
    abi != CallableAbi::Bray
        && matches!(
            bray_target::NativeTarget::for_profile(target.profile()),
            Some(bray_target::NativeTarget::X86_64WindowsMsvc)
        )
}

// Called after direct classification. Win64 packs the entire aggregate into one
// integer register, even when its fields are floats or narrower integers.
pub(in crate::compilation::product::realization) fn direct_abi_coercion(
    abi: CallableAbi,
    kind: &CodegenTypeKind,
    layout: TargetValueLayout,
    target: &CodegenTarget,
) -> Option<CodegenAggregateCoercion> {
    if !windows_x64_abi(abi, target) || !is_aggregate(kind) {
        return None;
    }

    assert!(
        matches!(layout.size(), 1 | 2 | 4 | 8),
        "direct Win64 aggregate fits one register"
    );

    let bits = u16::try_from(layout.size() * 8).expect("Win64 aggregate width fits u16");

    Some(CodegenAggregateCoercion::new([CodegenAbiPiece {
        offset_bytes: 0,
        scalar: CodegenAbiScalar::Integer(NonZeroU16::new(bits).expect("nonempty aggregate")),
    }]))
}

pub(in crate::compilation::product::realization) fn indirect_abi_value(
    abi: CallableAbi,
    kind: &CodegenTypeKind,
    layout: TargetValueLayout,
    target: &CodegenTarget,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    if !is_aggregate(kind) {
        return false;
    }

    if abi == CallableAbi::Bray {
        let register_pair_bytes = pointer_layout(target).size().saturating_mul(2);

        return layout.size() > register_pair_bytes;
    }

    if target.profile().machine().architecture() == bray_target::TargetArchitecture::Aarch64
        && is_homogeneous_float_aggregate(kind, mappings)
    {
        return false;
    }

    if windows_x64_abi(abi, target) {
        !matches!(layout.size(), 1 | 2 | 4 | 8)
    } else {
        layout.size() > pointer_layout(target).size().saturating_mul(2)
    }
}

pub(in crate::compilation::product::realization) fn indirect_parameter_kind(
    abi: CallableAbi,
    target: &CodegenTarget,
) -> CodegenIndirectParameterKind {
    if abi != CallableAbi::Bray
        && (target.profile().machine().architecture() == bray_target::TargetArchitecture::Aarch64
            || windows_x64_abi(abi, target))
    {
        return CodegenIndirectParameterKind::Reference;
    }

    CodegenIndirectParameterKind::ByValue
}

pub(in crate::compilation::product::realization) fn is_homogeneous_float_aggregate(
    root: &CodegenTypeKind,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    let mut pending = vec![(root, 1_u64)];
    let mut element_width = None;
    let mut element_count = 0_u64;

    while let Some((kind, multiplicity)) = pending.pop() {
        match kind {
            CodegenTypeKind::Float(width) => {
                if element_width.is_some_and(|element_width| element_width != *width) {
                    return false;
                }

                element_width = Some(*width);
                element_count = element_count.saturating_add(multiplicity);

                if element_count > 4 {
                    return false;
                }
            }
            CodegenTypeKind::Aggregate(fields) => {
                for field in fields.iter().rev() {
                    let Some(mapping) = mappings.get(&field.ty()) else {
                        return false;
                    };

                    pending.push((mapping.kind(), multiplicity));
                }
            }
            CodegenTypeKind::Array { element, length } => {
                let Some(mapping) = mappings.get(element) else {
                    return false;
                };

                let Some(multiplicity) = multiplicity.checked_mul(*length) else {
                    return false;
                };

                if multiplicity == 0 || multiplicity > 4 {
                    return false;
                }

                pending.push((mapping.kind(), multiplicity));
            }
            _ => return false,
        }
    }

    element_width.is_some() && element_count > 0
}
