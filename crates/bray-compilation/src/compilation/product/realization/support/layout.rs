use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU16, NonZeroU64};

use bray_codegen::{CodegenTarget, CodegenTypeKind, CodegenTypeMapping, TargetAddressSpaceKind};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{DeclaredLayoutMode, TypeData, TypeId};
use bray_target::{
    TargetAtomicRepresentation, TargetLayoutContract, TargetScalarKind, TargetValueLayout,
};

use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

pub(in crate::compilation::product::realization) fn atomic_storage_is_padding_free(
    ty: TypeId,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    let Some(mapping) = mappings.get(&ty) else {
        return false;
    };

    let Some(layout) = mapping.layout() else {
        return false;
    };

    match mapping.kind() {
        CodegenTypeKind::Aggregate(fields) => {
            let mut end = 0_u64;

            for field in fields.iter() {
                let Some(field_layout) = mappings
                    .get(&field.ty())
                    .and_then(CodegenTypeMapping::layout)
                else {
                    return false;
                };

                if field.offset_bytes() != end
                    || !atomic_storage_is_padding_free(field.ty(), mappings)
                {
                    return false;
                }

                let Some(field_end) = end.checked_add(field_layout.size()) else {
                    return false;
                };

                end = field_end;
            }

            end == layout.size()
        }
        CodegenTypeKind::Array { element, length } => {
            let Some(element_layout) = mappings.get(element).and_then(CodegenTypeMapping::layout)
            else {
                return false;
            };

            atomic_storage_is_padding_free(*element, mappings)
                && element_layout
                    .size()
                    .checked_mul(*length)
                    .is_some_and(|size| size == layout.size())
        }
        CodegenTypeKind::Opaque
        | CodegenTypeKind::Union { .. }
        | CodegenTypeKind::UnsizedSlice { .. }
        | CodegenTypeKind::UnsizedTraitView => false,
        CodegenTypeKind::Unit
        | CodegenTypeKind::Boolean
        | CodegenTypeKind::SignedInteger(_)
        | CodegenTypeKind::UnsignedInteger(_)
        | CodegenTypeKind::Float(_)
        | CodegenTypeKind::Pointer { .. }
        | CodegenTypeKind::Callable(_) => true,
    }
}

pub(in crate::compilation::product::realization) fn atomic_representation_for_type(
    compilation: &Compilation,
    ty: TypeId,
    target: &CodegenTarget,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAtomicRepresentation>, CodegenPreparationError> {
    let values = compilation.semantic_value_store()?;

    let data = values.type_data(ty);

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(None);
    };

    let role = crate::compilation::foreign::compiler_known_representation(compilation, *definition);

    if let Some(representation) = role.and_then(|role| {
        bray_checker::atomic_target_representation(
            role,
            target.profile().machine().pointer_width_bits().get(),
        )
    }) {
        return Ok(Some(representation));
    }

    let representation =
        compilation.declared_type_representation_with_cancellation(*definition, cancellation)?;

    let representation = representation.value();

    if representation.is_recovered()
        || !representation.has_finite_size()
        || (!representation.is_plain_storage()
            && representation.layout() != DeclaredLayoutMode::Transparent)
    {
        return Ok(None);
    }

    compilation
        .plain_storage_atomic_representation(ty, cancellation)
        .map_err(CodegenPreparationError::from)
}

pub(in crate::compilation::product::realization) const fn atomic_storage_role(
    representation: TargetAtomicRepresentation,
) -> Option<RepresentationRole> {
    match representation {
        TargetAtomicRepresentation::U8 => Some(RepresentationRole::ScalarU8),
        TargetAtomicRepresentation::U16 => Some(RepresentationRole::ScalarU16),
        TargetAtomicRepresentation::U32 => Some(RepresentationRole::ScalarU32),
        TargetAtomicRepresentation::U64 => Some(RepresentationRole::ScalarU64),
        TargetAtomicRepresentation::U128 => Some(RepresentationRole::ScalarU128),
        TargetAtomicRepresentation::Pointer => None,
    }
}

pub(in crate::compilation::product::realization) const fn target_layout_contract(
    layout: DeclaredLayoutMode,
) -> TargetLayoutContract {
    match layout {
        DeclaredLayoutMode::Default => TargetLayoutContract::Default,
        DeclaredLayoutMode::Stable => TargetLayoutContract::Stable,
        DeclaredLayoutMode::C => TargetLayoutContract::C,
        DeclaredLayoutMode::Transparent => TargetLayoutContract::Transparent,
    }
}

pub(in crate::compilation::product::realization) fn scalar_mapping(
    compilation: &Compilation,
    ty: TypeId,
    role: RepresentationRole,
    scalar: TargetScalarKind,
    target: &CodegenTarget,
    cancellation: &CancellationToken,
    mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
    pending: &mut BTreeSet<TypeId>,
) -> Result<CodegenTypeMapping, CodegenPreparationError> {
    if !target.profile().properties().scalars().supports(scalar) {
        return Err(CodegenPreparationError::UnsupportedType(ty));
    }

    if let Some(component) = role.complex_component() {
        let component = compilation.compiler_known_type(component)?;

        return compilation.codegen_aggregate_type(
            ty,
            [(None, component), (None, component)],
            TargetLayoutContract::Default,
            Some(
                target
                    .profile()
                    .properties()
                    .scalars()
                    .alignment(scalar)
                    .get(),
            ),
            None,
            target,
            cancellation,
            mappings,
            pending,
        );
    }

    let pointer_width = target.machine().pointer_width_bits().get();

    let (size, kind) = match scalar {
        TargetScalarKind::Bool => (1, CodegenTypeKind::Boolean),
        TargetScalarKind::Char => (4, CodegenTypeKind::UnsignedInteger(nonzero_width(32))),
        TargetScalarKind::I8 => (1, CodegenTypeKind::SignedInteger(nonzero_width(8))),
        TargetScalarKind::I16 => (2, CodegenTypeKind::SignedInteger(nonzero_width(16))),
        TargetScalarKind::I32 => (4, CodegenTypeKind::SignedInteger(nonzero_width(32))),
        TargetScalarKind::I64 => (8, CodegenTypeKind::SignedInteger(nonzero_width(64))),
        TargetScalarKind::I128 => (16, CodegenTypeKind::SignedInteger(nonzero_width(128))),
        TargetScalarKind::U8 => (1, CodegenTypeKind::UnsignedInteger(nonzero_width(8))),
        TargetScalarKind::U16 => (2, CodegenTypeKind::UnsignedInteger(nonzero_width(16))),
        TargetScalarKind::U32 => (4, CodegenTypeKind::UnsignedInteger(nonzero_width(32))),
        TargetScalarKind::U64 => (8, CodegenTypeKind::UnsignedInteger(nonzero_width(64))),
        TargetScalarKind::U128 => (16, CodegenTypeKind::UnsignedInteger(nonzero_width(128))),
        TargetScalarKind::Isize => (
            u64::from(pointer_width.div_ceil(8)),
            CodegenTypeKind::SignedInteger(nonzero_width(pointer_width)),
        ),
        TargetScalarKind::Usize => (
            u64::from(pointer_width.div_ceil(8)),
            CodegenTypeKind::UnsignedInteger(nonzero_width(pointer_width)),
        ),
        TargetScalarKind::R16 => (2, CodegenTypeKind::Float(nonzero_width(16))),
        TargetScalarKind::R32 => (4, CodegenTypeKind::Float(nonzero_width(32))),
        TargetScalarKind::R64 => (8, CodegenTypeKind::Float(nonzero_width(64))),
        TargetScalarKind::R128 => (16, CodegenTypeKind::Float(nonzero_width(128))),
        TargetScalarKind::C32
        | TargetScalarKind::C64
        | TargetScalarKind::C128
        | TargetScalarKind::C256 => return Err(CodegenPreparationError::UnresolvedType(ty)),
    };

    Ok(CodegenTypeMapping::new(
        ty,
        TargetValueLayout::new(
            size,
            target.profile().properties().scalars().alignment(scalar),
            TargetLayoutContract::Default,
        ),
        kind,
    ))
}

pub(in crate::compilation::product::realization) fn pointer_mapping(
    ty: TypeId,
    pointee: TypeId,
    target: &CodegenTarget,
    address_space: TargetAddressSpaceKind,
) -> CodegenTypeMapping {
    CodegenTypeMapping::new(
        ty,
        pointer_layout(target),
        CodegenTypeKind::Pointer {
            target: pointee,
            address_space,
        },
    )
}

pub(in crate::compilation::product::realization) fn pointer_layout(
    target: &CodegenTarget,
) -> TargetValueLayout {
    TargetValueLayout::new(
        u64::from(target.machine().pointer_width_bits().get().div_ceil(8)),
        NonZeroU64::from(target.machine().pointer_alignment_bytes()),
        TargetLayoutContract::Default,
    )
}

pub(in crate::compilation::product::realization) fn sized_layout(
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
    ty: TypeId,
) -> Result<TargetValueLayout, CodegenPreparationError> {
    mappings
        .get(&ty)
        .and_then(CodegenTypeMapping::layout)
        .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))
}

pub(in crate::compilation::product::realization) fn ensure_target_alignment(
    ty: TypeId,
    alignment: NonZeroU64,
    target: &CodegenTarget,
) -> Result<(), CodegenPreparationError> {
    if alignment > target.profile().properties().alignments().max_storage() {
        return Err(CodegenPreparationError::UnsupportedType(ty));
    }

    Ok(())
}

pub(in crate::compilation::product::realization) fn align_to(
    value: u64,
    alignment: NonZeroU64,
) -> Option<u64> {
    let mask = alignment.get().checked_sub(1)?;

    value.checked_add(mask).map(|value| value & !mask)
}

pub(in crate::compilation::product::realization) fn packed_alignment(
    alignment: NonZeroU64,
    packing: Option<NonZeroU64>,
) -> NonZeroU64 {
    packing.map_or(alignment, |packing| alignment.min(packing))
}

pub(in crate::compilation::product::realization) fn nonzero_width(width: u16) -> NonZeroU16 {
    NonZeroU16::new(width).unwrap_or(NonZeroU16::MIN)
}
