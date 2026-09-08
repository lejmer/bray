use bray_codegen::{CodegenFailure, CodegenMappings, CodegenTypeBehavior, CodegenTypeKind};
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

use super::super::LlvmTypeMappings;

pub(super) fn static_initializer<'context>(
    module: &Module<'context>,
    mapping: &bray_codegen::CodegenStaticStorageMapping,
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    static_constant(
        module,
        mapping,
        mapping.initial_value(),
        mapping.ty(),
        mapping.owner(),
        mappings,
        types,
    )
}

fn static_constant<'context>(
    module: &Module<'context>,
    storage: &bray_codegen::CodegenStaticStorageMapping,
    value: bray_symbols::ConstantValueId,
    representation: bray_symbols::TypeId,
    owner: &bray_codegen::CodegenInstanceKey,
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let constant = mappings
        .constant(value)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let ty = types.map(representation)?;

    match constant.data().kind() {
        bray_symbols::ConstantValueKind::Boolean(value) => {
            let BasicTypeEnum::IntType(ty) = ty else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            Ok(ty.const_int(u64::from(*value), false).into())
        }
        bray_symbols::ConstantValueKind::Character(value) => {
            let BasicTypeEnum::IntType(ty) = ty else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            Ok(ty.const_int(u64::from(u32::from(*value)), false).into())
        }
        bray_symbols::ConstantValueKind::Integer(value) => {
            let BasicTypeEnum::IntType(ty) = ty else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            Ok(crate::translation::integer_constant(ty, value).into())
        }
        bray_symbols::ConstantValueKind::Real(bits) => static_real_constant(types, *bits),
        bray_symbols::ConstantValueKind::Complex { real, imaginary } => {
            let width = u64::from(crate::translation::real_width(*real)) / 8;

            let values = [
                (0, width, static_real_constant(types, *real)?),
                (width, width, static_real_constant(types, *imaginary)?),
            ];

            physical_aggregate(types, width.saturating_mul(2), values)
        }
        bray_symbols::ConstantValueKind::String(text) => static_string_constant(
            module,
            constant.data().ty(),
            representation,
            text,
            owner,
            mappings,
            types,
        ),
        bray_symbols::ConstantValueKind::StaticAddress(_) => {
            let relocation = storage
                .relocation(value)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let target = if let Some(target) = module.get_global(relocation.symbol().as_str()) {
                target
            } else {
                let target = module.add_global(
                    types.map(relocation.ty())?,
                    None,
                    relocation.symbol().as_str(),
                );

                target.set_linkage(inkwell::module::Linkage::External);
                target.set_visibility(inkwell::GlobalVisibility::Hidden);

                if relocation.instance().duration()
                    == bray_symbols::StaticStorageDuration::ExactThread
                {
                    target.set_thread_local(true);
                }

                target
            };

            Ok(target.as_pointer_value().into())
        }
        bray_symbols::ConstantValueKind::Unit | bray_symbols::ConstantValueKind::NullableAbsent => {
            Ok(ty.const_zero())
        }
        bray_symbols::ConstantValueKind::NullablePresent(child) => {
            let child = mappings
                .constant(*child)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let child_value = static_constant(
                module,
                storage,
                child.value(),
                child.representation(),
                owner,
                mappings,
                types,
            )?;

            if child_value.get_type() == ty {
                return Ok(child_value);
            }

            let fields = aggregate_fields(owner, representation, mappings)?;

            let payload = fields
                .last()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let total = layout_size(owner, representation, mappings)?;

            let mut values = vec![(
                payload.offset_bytes(),
                layout_size(owner, payload.ty(), mappings)?,
                child_value,
            )];

            if fields.len() > 1 {
                let tag = fields
                    .first()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let BasicTypeEnum::IntType(tag_type) = types.map(tag.ty())? else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                values.push((
                    tag.offset_bytes(),
                    layout_size(owner, tag.ty(), mappings)?,
                    tag_type.const_int(1, false).into(),
                ));
            }

            physical_aggregate(types, total, values)
        }
        bray_symbols::ConstantValueKind::Tuple(children) => static_positional_aggregate(
            module,
            storage,
            representation,
            owner,
            children,
            mappings,
            types,
        ),
        bray_symbols::ConstantValueKind::Array(children) => static_array(
            module,
            storage,
            representation,
            owner,
            children,
            mappings,
            types,
        ),
        bray_symbols::ConstantValueKind::Product(fields) => {
            if fields.is_empty() {
                return Ok(ty.const_zero());
            }

            let layout = aggregate_fields(owner, representation, mappings)?;
            let total = layout_size(owner, representation, mappings)?;
            let mut values = Vec::with_capacity(fields.len());

            for field in fields.iter() {
                let field_layout = layout
                    .iter()
                    .find(|layout| {
                        layout.reference()
                            == Some(bray_ir::MirFieldReference::Struct(*field.field()))
                    })
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let child = mappings
                    .constant(*field.value())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let value = static_constant(
                    module,
                    storage,
                    child.value(),
                    child.representation(),
                    owner,
                    mappings,
                    types,
                )?;

                values.push((
                    field_layout.offset_bytes(),
                    layout_size(owner, field_layout.ty(), mappings)?,
                    value,
                ));
            }

            physical_aggregate(types, total, values)
        }
        bray_symbols::ConstantValueKind::Union { variant, fields } => static_union(
            module,
            storage,
            representation,
            owner,
            *variant,
            fields,
            mappings,
            types,
        ),
        bray_symbols::ConstantValueKind::Error => Err(CodegenFailure::GeneratedModuleInvariant),
    }
}

fn static_string_constant<'context>(
    module: &Module<'context>,
    semantic_type: bray_symbols::TypeId,
    representation: bray_symbols::TypeId,
    text: &str,
    owner: &bray_codegen::CodegenInstanceKey,
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let bytes = types.context().const_string(text.as_bytes(), false).into();
    let identity = crate::translation::string_constant_name(text);

    let data = crate::mapping::publish_immutable_global(
        module,
        mappings.target(),
        &format!("{identity}.data"),
        bytes,
    );

    let mapping = type_mapping(owner, representation, mappings)?;

    match mapping.kind() {
        CodegenTypeKind::Pointer { .. } => {
            let value = string_value(
                semantic_type,
                data.as_pointer_value().into(),
                text.len(),
                owner,
                mappings,
                types,
            )?;

            let global = crate::mapping::publish_immutable_global(
                module,
                mappings.target(),
                &format!("{identity}.value"),
                value,
            );

            Ok(global.as_pointer_value().into())
        }
        CodegenTypeKind::Aggregate(fields)
            if fields.len() == 3 && mapping.behavior() == Some(CodegenTypeBehavior::String) =>
        {
            string_value(
                representation,
                data.as_pointer_value().into(),
                text.len(),
                owner,
                mappings,
                types,
            )
        }
        _ => Err(CodegenFailure::GeneratedModuleInvariant),
    }
}

fn string_value<'context>(
    representation: bray_symbols::TypeId,
    data: BasicValueEnum<'context>,
    length: usize,
    owner: &bray_codegen::CodegenInstanceKey,
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let fields = aggregate_fields(owner, representation, mappings)?;

    let pointer = fields
        .first()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let length_field = fields
        .get(2)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let BasicTypeEnum::IntType(length_type) = types.map(length_field.ty())? else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let length = crate::conversion::resource_limit(length, "static_array_length")?;

    physical_aggregate(
        types,
        layout_size(owner, representation, mappings)?,
        [
            (
                pointer.offset_bytes(),
                layout_size(owner, pointer.ty(), mappings)?,
                data,
            ),
            (
                length_field.offset_bytes(),
                layout_size(owner, length_field.ty(), mappings)?,
                length_type.const_int(length, false).into(),
            ),
        ],
    )
}

fn static_positional_aggregate<'context>(
    module: &Module<'context>,
    storage: &bray_codegen::CodegenStaticStorageMapping,
    representation: bray_symbols::TypeId,
    owner: &bray_codegen::CodegenInstanceKey,
    children: &[bray_symbols::ConstantValueId],
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let fields = aggregate_fields(owner, representation, mappings)?;
    let total = layout_size(owner, representation, mappings)?;
    let mut values = Vec::with_capacity(children.len());

    for (index, child) in children.iter().enumerate() {
        let field = fields
            .get(index)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let child = mappings
            .constant(*child)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let value = static_constant(
            module,
            storage,
            child.value(),
            child.representation(),
            owner,
            mappings,
            types,
        )?;

        values.push((
            field.offset_bytes(),
            layout_size(owner, field.ty(), mappings)?,
            value,
        ));
    }

    physical_aggregate(types, total, values)
}

fn static_array<'context>(
    module: &Module<'context>,
    storage: &bray_codegen::CodegenStaticStorageMapping,
    representation: bray_symbols::TypeId,
    owner: &bray_codegen::CodegenInstanceKey,
    children: &[bray_symbols::ConstantValueId],
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let mapping = type_mapping(owner, representation, mappings)?;

    let CodegenTypeKind::Array { length, .. } = mapping.kind() else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let total = layout_size(owner, representation, mappings)?;
    let stride = total.checked_div(*length).unwrap_or(0);
    let mut values = Vec::with_capacity(children.len());

    for (index, child) in children.iter().enumerate() {
        let child = mappings
            .constant(*child)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let value = static_constant(
            module,
            storage,
            child.value(),
            child.representation(),
            owner,
            mappings,
            types,
        )?;

        let index: u64 = crate::conversion::resource_limit(index, "static_array_index")?;

        values.push((index.saturating_mul(stride), stride, value));
    }

    physical_aggregate(types, total, values)
}

fn static_union<'context>(
    module: &Module<'context>,
    storage: &bray_codegen::CodegenStaticStorageMapping,
    representation: bray_symbols::TypeId,
    owner: &bray_codegen::CodegenInstanceKey,
    selected: bray_symbols::UnionVariantSymbolId,
    fields: &[bray_symbols::ConstantField<
        bray_symbols::UnionPayloadFieldSymbolId,
        bray_symbols::ConstantValueId,
    >],
    mappings: &CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let mapping = type_mapping(owner, representation, mappings)?;

    let CodegenTypeKind::Union { tag, .. } = mapping.kind() else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let variant = mapping
        .kind()
        .union_variant(selected)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let mut values = Vec::new();

    if let (Some(tag), Some(value)) = (*tag, variant.tag()) {
        let BasicTypeEnum::IntType(tag_type) = types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        values.push((
            0,
            layout_size(owner, tag, mappings)?,
            crate::translation::integer_constant(tag_type, value).into(),
        ));
    }

    for field in fields {
        let layout = variant
            .payload_field(*field.field())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let child = mappings
            .constant(*field.value())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let value = static_constant(
            module,
            storage,
            child.value(),
            child.representation(),
            owner,
            mappings,
            types,
        )?;

        values.push((
            layout.offset_bytes(),
            layout_size(owner, layout.ty(), mappings)?,
            value,
        ));
    }

    physical_aggregate(types, layout_size(owner, representation, mappings)?, values)
}

fn static_real_constant<'context>(
    types: &LlvmTypeMappings<'context, '_>,
    bits: bray_symbols::RealConstantBits,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let width = crate::translation::real_width(bits);

    let integer = types
        .context()
        .custom_width_int_type(
            std::num::NonZeroU32::new(width).unwrap_or(std::num::NonZeroU32::MIN),
        )
        .map_err(CodegenFailure::backend_library)?;

    Ok(integer
        .const_int_arbitrary_precision(&crate::translation::real_words(bits))
        .into())
}

fn physical_aggregate<'context>(
    types: &LlvmTypeMappings<'context, '_>,
    total: u64,
    values: impl IntoIterator<Item = (u64, u64, BasicValueEnum<'context>)>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let mut values = values.into_iter().collect::<Vec<_>>();

    values.sort_unstable_by_key(|(offset, _, _)| *offset);

    let mut element_types = Vec::new();
    let mut element_values = Vec::new();
    let mut current = 0_u64;

    for (offset, size, value) in values {
        if offset < current || offset.saturating_add(size) > total {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        push_padding(
            types,
            offset - current,
            &mut element_types,
            &mut element_values,
        )?;

        element_types.push(value.get_type());
        element_values.push(value);
        current = offset.saturating_add(size);
    }

    push_padding(
        types,
        total - current,
        &mut element_types,
        &mut element_values,
    )?;

    let aggregate = types.context().struct_type(&element_types, true);

    Ok(aggregate.const_named_struct(&element_values).into())
}

fn push_padding<'context>(
    types: &LlvmTypeMappings<'context, '_>,
    size: u64,
    element_types: &mut Vec<BasicTypeEnum<'context>>,
    element_values: &mut Vec<BasicValueEnum<'context>>,
) -> Result<(), CodegenFailure> {
    if size == 0 {
        return Ok(());
    }

    let size = crate::conversion::resource_limit(size, "static_padding_size")?;
    let padding = types.context().i8_type().array_type(size);

    element_types.push(padding.into());
    element_values.push(padding.const_zero().into());

    Ok(())
}

fn aggregate_fields<'mappings>(
    owner: &bray_codegen::CodegenInstanceKey,
    ty: bray_symbols::TypeId,
    mappings: &'mappings CodegenMappings,
) -> Result<&'mappings [bray_codegen::CodegenFieldLayout], CodegenFailure> {
    let mapping = type_mapping(owner, ty, mappings)?;

    let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    Ok(fields)
}

fn layout_size(
    owner: &bray_codegen::CodegenInstanceKey,
    ty: bray_symbols::TypeId,
    mappings: &CodegenMappings,
) -> Result<u64, CodegenFailure> {
    type_mapping(owner, ty, mappings)?
        .layout()
        .map(|layout| layout.size())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
}

fn type_mapping<'mappings>(
    owner: &bray_codegen::CodegenInstanceKey,
    ty: bray_symbols::TypeId,
    mappings: &'mappings CodegenMappings,
) -> Result<&'mappings bray_codegen::CodegenTypeMapping, CodegenFailure> {
    mappings
        .instance_ty(owner, ty)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
}
