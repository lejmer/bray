use std::collections::BTreeMap;

use bray_codegen::{
    CodegenIndirectParameterKind, CodegenTarget, CodegenTypeKind, CodegenTypeMapping,
};
use bray_symbols::{CallableAbi, TypeId};
use bray_target::TargetValueLayout;

use super::layout::pointer_layout;

pub(in crate::compilation::product::realization) fn indirect_abi_value(
    abi: CallableAbi,
    kind: &CodegenTypeKind,
    layout: TargetValueLayout,
    target: &CodegenTarget,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    let is_composite = matches!(
        kind,
        CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. }
    );

    if !is_composite {
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

    match bray_target::NativeTarget::for_profile(target.profile()) {
        Some(bray_target::NativeTarget::X86_64WindowsMsvc) => {
            !matches!(layout.size(), 1 | 2 | 4 | 8)
        }
        _ => layout.size() > pointer_layout(target).size().saturating_mul(2),
    }
}

pub(in crate::compilation::product::realization) fn indirect_parameter_kind(
    abi: CallableAbi,
    target: &CodegenTarget,
) -> CodegenIndirectParameterKind {
    if abi != CallableAbi::Bray
        && (target.profile().machine().architecture() == bray_target::TargetArchitecture::Aarch64
            || matches!(
                bray_target::NativeTarget::for_profile(target.profile()),
                Some(bray_target::NativeTarget::X86_64WindowsMsvc)
            ))
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
