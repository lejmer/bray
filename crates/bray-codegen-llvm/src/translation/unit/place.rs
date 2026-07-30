use super::core::UnitTranslator;
use super::support::{
    aggregate_element, extract_value, insert_value, int_value, llvm, pointer_value,
};
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{
    MirBlockId, MirOperand, MirOperation, MirPlace, MirProjectionKind, MirStorageId, MirValueId,
};
use inkwell::basic_block::BasicBlock;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn place(
        &mut self,
        place: &MirPlace,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let mut pointer = self.storage(place.storage())?;

        let mut source_type = self
            .unit
            .storage(place.storage())
            .map(bray_ir::MirStorage::ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        for projection in place.projections() {
            if projection.source_type() != source_type {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            pointer = self.project_place(pointer, source_type, projection)?;

            source_type = projection.result_type();
        }

        if source_type != place.ty() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(pointer)
    }

    pub(super) fn project_place(
        &mut self,
        pointer: PointerValue<'context>,
        source_type: bray_symbols::TypeId,
        projection: &bray_ir::MirProjection,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        // Projection can translate index operands after releasing the immutable mapping borrow.
        let kind = self
            .request
            .mappings()
            .ty(source_type)
            .map(|mapping| mapping.kind().clone())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        // Keep this exhaustive so every place projection requires an explicit translation.
        match projection.kind() {
            MirProjectionKind::Dereference => pointer_value(llvm(self.builder.build_load(
                self.types.map(source_type)?,
                pointer,
                "dereference",
            ))?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant),
            MirProjectionKind::Field(_)
            | MirProjectionKind::TupleField(_)
            | MirProjectionKind::ElementFromStart(_)
            | MirProjectionKind::ElementFromEnd(_)
                if matches!(kind, CodegenTypeKind::Aggregate(_)) =>
            {
                let index = self.projection_element(source_type, projection.kind())?;

                llvm(self.builder.build_struct_gep(
                    self.types.map(source_type)?,
                    pointer,
                    index,
                    "projection",
                ))
            }
            MirProjectionKind::ElementFromStart(index) => {
                self.static_element_pointer(pointer, &kind, u64::from(*index))
            }
            MirProjectionKind::ElementFromEnd(index) => {
                let CodegenTypeKind::Array { length, .. } = kind else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let index = length
                    .checked_sub(u64::from(*index))
                    .and_then(|value| value.checked_sub(1))
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.static_element_pointer(
                    pointer,
                    self.request
                        .mappings()
                        .ty(source_type)
                        .map(bray_codegen::CodegenTypeMapping::kind)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
                    index,
                )
            }
            MirProjectionKind::Index(index) => {
                let index = self.operand(index)?;

                self.index_pointer(pointer, source_type, &kind, index)
            }
            MirProjectionKind::Slice { start, end } => self.slice_pointer(
                pointer,
                source_type,
                projection.result_type(),
                &kind,
                start.as_ref(),
                end.as_ref(),
            ),
            MirProjectionKind::Variant(variant) => {
                self.union_variant_pointer(pointer, &kind, *variant)
            }
            MirProjectionKind::ActiveUnionPayloadField { variant, field } => {
                self.union_field_pointer(pointer, &kind, *variant, *field)
            }
            MirProjectionKind::NullableValue => {
                self.nullable_value_pointer(pointer, source_type, &kind)
            }
            MirProjectionKind::OwnedStorage => Ok(pointer),
            MirProjectionKind::Field(_) | MirProjectionKind::TupleField(_) => {
                Err(CodegenFailure::GeneratedModuleInvariant)
            }
        }
    }

    pub(super) fn static_element_pointer(
        &self,
        pointer: PointerValue<'context>,
        kind: &CodegenTypeKind,
        index: u64,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let CodegenTypeKind::Array { element, length } = kind else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if index >= *length {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let stride = self
            .request
            .mappings()
            .ty(*element)
            .map(|mapping| mapping.layout().size())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let offset = stride
            .checked_mul(index)
            .ok_or(CodegenFailure::ResourceExhausted)?;

        self.constant_offset_pointer(pointer, offset)
    }

    pub(super) fn index_pointer(
        &mut self,
        pointer: PointerValue<'context>,
        _source_type: bray_symbols::TypeId,
        kind: &CodegenTypeKind,
        index: BasicValueEnum<'context>,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let index = int_value(index).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match kind {
            CodegenTypeKind::Array { .. } => index
                .get_zero_extended_constant()
                .ok_or(CodegenFailure::UnsupportedTarget)
                .and_then(|index| self.static_element_pointer(pointer, kind, index)),
            CodegenTypeKind::Aggregate(_) => Err(CodegenFailure::UnsupportedTarget),
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn slice_pointer(
        &mut self,
        pointer: PointerValue<'context>,
        source_type: bray_symbols::TypeId,
        result_type: bray_symbols::TypeId,
        source_kind: &CodegenTypeKind,
        start: Option<&MirOperand>,
        end: Option<&MirOperand>,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let integer_type = self
            .types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None);

        let zero = integer_type.const_zero();

        let start = match start {
            Some(start) => {
                let start = self.operand(start)?;

                let start = int_value(start)
                    .and_then(|value| value.get_zero_extended_constant())
                    .ok_or(CodegenFailure::UnsupportedTarget)?;

                integer_type.const_int(start, false)
            }
            None => zero,
        };

        let (data, source_length, element) = match source_kind {
            CodegenTypeKind::Array { element, length } => {
                (pointer, integer_type.const_int(*length, false), *element)
            }
            CodegenTypeKind::Aggregate(fields) => {
                let (data, length) = self.slice_parts(pointer, source_type, fields)?;

                let element = self
                    .request
                    .mappings()
                    .ty(fields[0].ty())
                    .and_then(|mapping| match mapping.kind() {
                        CodegenTypeKind::Pointer { target, .. } => Some(*target),
                        _ => None,
                    })
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                (data, self.pointer_sized_integer(length.into())?, element)
            }
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let end = match end {
            Some(end) => {
                let end = self.operand(end)?;

                let end = int_value(end)
                    .and_then(|value| value.get_zero_extended_constant())
                    .ok_or(CodegenFailure::UnsupportedTarget)?;

                integer_type.const_int(end, false)
            }
            None => source_length,
        };

        let Some(start_constant) = start.get_zero_extended_constant() else {
            return Err(CodegenFailure::UnsupportedTarget);
        };

        let Some(end_constant) = end.get_zero_extended_constant() else {
            return Err(CodegenFailure::UnsupportedTarget);
        };

        let Some(source_length_constant) = source_length.get_zero_extended_constant() else {
            return Err(CodegenFailure::UnsupportedTarget);
        };

        if start_constant > end_constant || end_constant > source_length_constant {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let length = llvm(self.builder.build_int_sub(end, start, "slice.length"))?;

        let stride = self
            .request
            .mappings()
            .ty(element)
            .map(|mapping| mapping.layout().size())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let data = self.dynamic_offset_pointer(data, start, stride)?;

        let result_mapping = self
            .request
            .mappings()
            .ty(result_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = result_mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let mut result = self.types.map(result_type)?.const_zero();

        let pointer_index = fields
            .iter()
            .position(|field| {
                self.request
                    .mappings()
                    .ty(field.ty())
                    .is_some_and(|mapping| {
                        matches!(mapping.kind(), CodegenTypeKind::Pointer { .. })
                    })
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let length_index = 1_usize
            .checked_sub(pointer_index)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        result = insert_value(
            &self.builder,
            result,
            data.into(),
            usize::try_from(aggregate_element(
                self.request.mappings(),
                fields,
                pointer_index,
            )?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?;

        result = insert_value(
            &self.builder,
            result,
            length.into(),
            usize::try_from(aggregate_element(
                self.request.mappings(),
                fields,
                length_index,
            )?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?;

        let storage = llvm(self.builder.build_alloca(result.get_type(), "slice.value"))?;

        llvm(self.builder.build_store(storage, result))?;

        Ok(storage)
    }

    pub(super) fn slice_parts(
        &mut self,
        pointer: PointerValue<'context>,
        source_type: bray_symbols::TypeId,
        fields: &[bray_codegen::CodegenFieldLayout],
    ) -> Result<(PointerValue<'context>, inkwell::values::IntValue<'context>), CodegenFailure> {
        let pointer_index = fields
            .iter()
            .position(|field| {
                self.request
                    .mappings()
                    .ty(field.ty())
                    .is_some_and(|mapping| {
                        matches!(mapping.kind(), CodegenTypeKind::Pointer { .. })
                    })
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let length_index = 1_usize
            .checked_sub(pointer_index)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pointer_field = llvm(self.builder.build_struct_gep(
            self.types.map(source_type)?,
            pointer,
            aggregate_element(self.request.mappings(), fields, pointer_index)?,
            "slice.data.address",
        ))?;

        let length_field = llvm(self.builder.build_struct_gep(
            self.types.map(source_type)?,
            pointer,
            aggregate_element(self.request.mappings(), fields, length_index)?,
            "slice.length.address",
        ))?;

        let data = llvm(self.builder.build_load(
            self.types.map(fields[pointer_index].ty())?,
            pointer_field,
            "slice.data",
        ))
        .and_then(|value| pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let length = llvm(self.builder.build_load(
            self.types.map(fields[length_index].ty())?,
            length_field,
            "slice.length",
        ))
        .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        Ok((data, length))
    }

    pub(super) fn union_variant_pointer(
        &self,
        pointer: PointerValue<'context>,
        kind: &CodegenTypeKind,
        variant: bray_symbols::UnionVariantSymbolId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let CodegenTypeKind::Union { variants, .. } = kind else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let offset = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .and_then(|layout| {
                layout
                    .fields()
                    .iter()
                    .map(bray_codegen::CodegenFieldLayout::offset_bytes)
                    .min()
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.constant_offset_pointer(pointer, offset)
    }

    pub(super) fn union_field_pointer(
        &self,
        pointer: PointerValue<'context>,
        kind: &CodegenTypeKind,
        variant: bray_symbols::UnionVariantSymbolId,
        field: bray_symbols::UnionPayloadFieldSymbolId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let CodegenTypeKind::Union { variants, .. } = kind else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let offset = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .and_then(|layout| {
                layout.fields().iter().find(|layout| {
                    layout.reference() == Some(bray_ir::MirFieldReference::UnionPayload(field))
                })
            })
            .map(bray_codegen::CodegenFieldLayout::offset_bytes)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.constant_offset_pointer(pointer, offset)
    }

    pub(super) fn nullable_value_pointer(
        &mut self,
        pointer: PointerValue<'context>,
        source_type: bray_symbols::TypeId,
        kind: &CodegenTypeKind,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        match kind {
            CodegenTypeKind::Pointer { .. } => pointer_value(llvm(self.builder.build_load(
                self.types.map(source_type)?,
                pointer,
                "nullable.value",
            ))?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant),
            CodegenTypeKind::Aggregate(fields) => {
                let payload = fields
                    .len()
                    .checked_sub(1)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_struct_gep(
                    self.types.map(source_type)?,
                    pointer,
                    aggregate_element(self.request.mappings(), fields, payload)?,
                    "nullable.value",
                ))
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn dynamic_offset_pointer(
        &self,
        pointer: PointerValue<'context>,
        index: inkwell::values::IntValue<'context>,
        stride: u64,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let integer_type = self
            .types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None);

        let index = llvm(
            self.builder
                .build_int_cast(index, integer_type, "index.pointer_size"),
        )?;

        let offset = llvm(self.builder.build_int_mul(
            index,
            integer_type.const_int(stride, false),
            "index.byte_offset",
        ))?;

        let address = llvm(
            self.builder
                .build_ptr_to_int(pointer, integer_type, "index.base"),
        )?;

        let address = llvm(self.builder.build_int_add(address, offset, "index.address"))?;

        llvm(
            self.builder
                .build_int_to_ptr(address, pointer.get_type(), "index.pointer"),
        )
    }

    pub(super) fn pointer_sized_integer(
        &self,
        value: BasicValueEnum<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let value = int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let integer_type = self
            .types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None);

        llvm(
            self.builder
                .build_int_cast(value, integer_type, "integer.pointer_size"),
        )
    }

    pub(super) fn projection_element(
        &self,
        source: bray_symbols::TypeId,
        projection: &MirProjectionKind,
    ) -> Result<u32, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(source)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match (mapping.kind(), projection) {
            (CodegenTypeKind::Aggregate(fields), MirProjectionKind::Field(reference)) => {
                let index = fields
                    .iter()
                    .position(|field| field.reference() == Some(*reference))
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                aggregate_element(self.request.mappings(), fields, index)
            }
            (CodegenTypeKind::Aggregate(fields), MirProjectionKind::TupleField(index))
            | (CodegenTypeKind::Aggregate(fields), MirProjectionKind::ElementFromStart(index)) => {
                aggregate_element(
                    self.request.mappings(),
                    fields,
                    usize::try_from(*index).map_err(|_| CodegenFailure::ResourceExhausted)?,
                )
            }
            (CodegenTypeKind::Aggregate(fields), MirProjectionKind::ElementFromEnd(index)) => {
                let index =
                    usize::try_from(*index).map_err(|_| CodegenFailure::ResourceExhausted)?;

                let index = fields
                    .len()
                    .checked_sub(index + 1)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                aggregate_element(self.request.mappings(), fields, index)
            }
            (
                _,
                MirProjectionKind::NullableValue
                | MirProjectionKind::Variant(_)
                | MirProjectionKind::OwnedStorage,
            ) => Ok(0),
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn convert(
        &mut self,
        value: BasicValueEnum<'context>,
        source: bray_symbols::TypeId,
        target: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if source == target {
            return Ok(value);
        }

        let source_signed = self.signed_integer(source).unwrap_or(false);
        let target_signed = self.signed_integer(target).unwrap_or(false);
        let target_type = self.types.map(target)?;

        match (value, target_type) {
            (BasicValueEnum::IntValue(value), BasicTypeEnum::IntType(target)) => {
                llvm(self.builder.build_int_cast_sign_flag(
                    value,
                    target,
                    source_signed,
                    "convert.integer",
                ))
                .map(Into::into)
            }
            (BasicValueEnum::FloatValue(value), BasicTypeEnum::FloatType(target)) => llvm(
                self.builder
                    .build_float_cast(value, target, "convert.float"),
            )
            .map(Into::into),
            (BasicValueEnum::IntValue(value), BasicTypeEnum::FloatType(target)) => {
                llvm(if source_signed {
                    self.builder
                        .build_signed_int_to_float(value, target, "convert.integer.float")
                } else {
                    self.builder
                        .build_unsigned_int_to_float(value, target, "convert.integer.float")
                })
                .map(Into::into)
            }
            (BasicValueEnum::FloatValue(value), BasicTypeEnum::IntType(target)) => {
                llvm(if target_signed {
                    self.builder
                        .build_float_to_signed_int(value, target, "convert.float.integer")
                } else {
                    self.builder
                        .build_float_to_unsigned_int(value, target, "convert.float.integer")
                })
                .map(Into::into)
            }
            (value, target) if value.get_type() == target => Ok(value),
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn project_value(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        result_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if subject.get_type() == self.types.map(result_type)? {
            return Ok(subject);
        }

        let mapping = self
            .request
            .mappings()
            .ty(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let payload = fields
            .len()
            .checked_sub(1)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        extract_value(
            &self.builder,
            subject,
            aggregate_element(self.request.mappings(), fields, payload)?,
        )
    }

    pub(super) fn operation_result_type(
        &self,
        operation: &MirOperation,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        operation
            .result()
            .and_then(|result| self.unit.value(result))
            .map(bray_ir::MirValue::ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn value_type(
        &mut self,
        value: MirValueId,
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let ty = self
            .unit
            .value(value)
            .map(bray_ir::MirValue::ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.types.map(ty)
    }

    pub(super) fn signed_integer(&self, ty: bray_symbols::TypeId) -> Result<bool, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match mapping.kind() {
            CodegenTypeKind::SignedInteger(_) => Ok(true),
            CodegenTypeKind::Boolean | CodegenTypeKind::UnsignedInteger(_) => Ok(false),
            CodegenTypeKind::Unit
            | CodegenTypeKind::Float(_)
            | CodegenTypeKind::Pointer { .. }
            | CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. }
            | CodegenTypeKind::Callable(_) => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn block(&self, block: MirBlockId) -> Result<BasicBlock<'context>, CodegenFailure> {
        self.blocks
            .get(&block)
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn storage(
        &self,
        storage: MirStorageId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        self.storages
            .get(&storage)
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}
