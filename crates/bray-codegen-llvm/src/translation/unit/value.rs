use super::core::UnitTranslator;
use super::support::{
    aggregate_value_element, insert_value, integer_constant, llvm, pointer_field_index, real_width,
    real_words,
};
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{MirImmediateValue, MirOperand};
use bray_symbols::{ConstantValueId, ConstantValueKind, RealConstantBits};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn operand(
        &mut self,
        operand: &MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match operand {
            MirOperand::Value(value) => self
                .values
                .get(value)
                .copied()
                .ok_or(CodegenFailure::GeneratedModuleInvariant),
            MirOperand::Immediate { value, ty } => {
                let ty = self.types.map(*ty)?;

                match value {
                    MirImmediateValue::Boolean(value) => match ty {
                        BasicTypeEnum::IntType(ty) => {
                            Ok(ty.const_int(u64::from(*value), false).into())
                        }
                        _ => Err(CodegenFailure::GeneratedModuleInvariant),
                    },
                    MirImmediateValue::Unit | MirImmediateValue::NullableAbsent => {
                        Ok(ty.const_zero())
                    }
                }
            }
            MirOperand::Copy(place) | MirOperand::Move(place) => {
                let pointer = self.place(place)?;

                llvm(
                    self.builder
                        .build_load(self.types.map(place.ty())?, pointer, "load"),
                )
            }
            MirOperand::Constant { value, .. } => self.constant(*value),
        }
    }

    pub(super) fn constant(
        &mut self,
        value: ConstantValueId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .constant(value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = mapping.data().ty();

        // Child constants are translated recursively after releasing the parent mapping borrow.
        let kind = mapping.data().kind().clone();

        match kind {
            ConstantValueKind::Error => Err(CodegenFailure::GeneratedModuleInvariant),
            ConstantValueKind::Boolean(value) => {
                let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                Ok(ty.const_int(u64::from(value), false).into())
            }
            ConstantValueKind::Character(value) => {
                let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                Ok(ty.const_int(u64::from(u32::from(value)), false).into())
            }
            ConstantValueKind::Integer(value) => self.integer_constant(ty, &value),
            ConstantValueKind::Real(bits) => self.real_constant(ty, bits),
            ConstantValueKind::Complex { real, imaginary } => {
                let mut value = self.types.map(ty)?.const_zero();
                let real = self.real_constant_component(real)?;
                let imaginary = self.real_constant_component(imaginary)?;

                value = insert_value(&self.builder, value, real, 0)?;

                insert_value(&self.builder, value, imaginary, 1)
            }
            ConstantValueKind::String(text) => self.string_constant(value, ty, &text),
            ConstantValueKind::Unit | ConstantValueKind::NullableAbsent => {
                Ok(self.types.map(ty)?.const_zero())
            }
            ConstantValueKind::NullablePresent(child) => {
                let child = self.constant(child)?;
                let mapped = self.types.map(ty)?;

                if child.get_type() == mapped {
                    return Ok(child);
                }

                let mut value = mapped.const_zero();

                let mapping = self
                    .request
                    .mappings()
                    .ty(ty)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let payload = fields
                    .len()
                    .checked_sub(1)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if fields.len() > 1 {
                    let tag_type = self.types.map(fields[0].ty())?;

                    let BasicTypeEnum::IntType(tag_type) = tag_type else {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    };

                    value = insert_value(
                        &self.builder,
                        value,
                        tag_type.const_int(1, false).into(),
                        aggregate_value_element(self.request.mappings(), fields, 0)?,
                    )?;
                }

                insert_value(
                    &self.builder,
                    value,
                    child,
                    aggregate_value_element(self.request.mappings(), fields, payload)?,
                )
            }
            ConstantValueKind::Tuple(children) | ConstantValueKind::Array(children) => {
                self.aggregate_constant(ty, children.iter().copied())
            }
            ConstantValueKind::Product(fields) => {
                let mapping = self
                    .request
                    .mappings()
                    .ty(ty)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let CodegenTypeKind::Aggregate(layout) = mapping.kind() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let mut value = self.types.map(ty)?.const_zero();

                for field in fields.iter() {
                    let field_value = self.constant(*field.value())?;

                    let index = layout
                        .iter()
                        .position(|layout| {
                            layout.reference()
                                == Some(bray_ir::MirFieldReference::Struct(*field.field()))
                        })
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    value = insert_value(
                        &self.builder,
                        value,
                        field_value,
                        aggregate_value_element(self.request.mappings(), layout, index)?,
                    )?;
                }

                Ok(value)
            }
            ConstantValueKind::Union { variant, fields } => {
                self.union_constant(ty, variant, &fields)
            }
        }
    }

    pub(super) fn integer_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        value: &bray_symbols::IntegerConstant,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(integer_constant(ty, value).into())
    }

    pub(super) fn real_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        bits: RealConstantBits,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let target = self.types.map(ty)?;

        self.real_bits(target, bits)
    }

    pub(super) fn real_constant_component(
        &mut self,
        bits: RealConstantBits,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let width = real_width(bits);

        let target = match width {
            16 => self.types.context().f16_type().into(),
            32 => self.types.context().f32_type().into(),
            64 => self.types.context().f64_type().into(),
            128 => self.types.context().f128_type().into(),
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        self.real_bits(target, bits)
    }

    pub(super) fn real_bits(
        &self,
        target: BasicTypeEnum<'context>,
        bits: RealConstantBits,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let BasicTypeEnum::FloatType(float) = target else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let words = real_words(bits);
        let width = u32::from(float.get_bit_width());

        let integer_type = self
            .types
            .context()
            .custom_width_int_type(
                std::num::NonZeroU32::new(width).unwrap_or(std::num::NonZeroU32::MIN),
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let integer = integer_type.const_int_arbitrary_precision(&words);

        llvm(self.builder.build_bit_cast(integer, float, "constant.real"))
    }

    pub(super) fn aggregate_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        children: impl IntoIterator<Item = ConstantValueId>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self
            .request
            .mappings()
            .ty(ty)
            .and_then(|mapping| match mapping.kind() {
                // Constant materialization mutates the value cache after this lookup.
                CodegenTypeKind::Aggregate(fields) => Some(fields.clone()),
                CodegenTypeKind::Array { .. } => None,
                _ => None,
            });

        let mut value = self.types.map(ty)?.const_zero();

        for (index, child) in children.into_iter().enumerate() {
            let child = self.constant(child)?;

            let element = match &fields {
                Some(fields) => aggregate_value_element(self.request.mappings(), fields, index)?,
                None => index,
            };

            value = insert_value(&self.builder, value, child, element)?;
        }

        Ok(value)
    }

    pub(super) fn string_constant(
        &mut self,
        value: ConstantValueId,
        ty: bray_symbols::TypeId,
        text: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let bytes = self.types.context().const_string(text.as_bytes(), false);
        let name = format!("bray.constant.string.{}", value.slot());

        let global = self
            .module
            .get_global(&name)
            .unwrap_or_else(|| self.module.add_global(bytes.get_type(), None, &name));

        global.set_constant(true);
        global.set_initializer(&bytes);

        let pointer: BasicValueEnum<'context> = global.as_pointer_value().into();

        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match mapping.kind() {
            CodegenTypeKind::Pointer { .. } => Ok(pointer),
            CodegenTypeKind::Aggregate(fields) if fields.len() == 2 => {
                let mut value = self.types.map(ty)?.const_zero();

                let pointer_index = pointer_field_index(self.request.mappings(), fields)?;

                let length_index = 1_usize
                    .checked_sub(pointer_index)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let BasicTypeEnum::IntType(length_type) =
                    self.types.map(fields[length_index].ty())?
                else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                value = insert_value(
                    &self.builder,
                    value,
                    pointer,
                    aggregate_value_element(self.request.mappings(), fields, pointer_index)?,
                )?;

                insert_value(
                    &self.builder,
                    value,
                    length_type
                        .const_int(
                            u64::try_from(text.len())
                                .map_err(|_| CodegenFailure::ResourceExhausted)?,
                            false,
                        )
                        .into(),
                    aggregate_value_element(self.request.mappings(), fields, length_index)?,
                )
            }
            CodegenTypeKind::Unit
            | CodegenTypeKind::Boolean
            | CodegenTypeKind::SignedInteger(_)
            | CodegenTypeKind::UnsignedInteger(_)
            | CodegenTypeKind::Float(_)
            | CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::UnsizedSlice { .. }
            | CodegenTypeKind::UnsizedTraitView
            | CodegenTypeKind::Union { .. }
            | CodegenTypeKind::Callable(_) => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn union_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        fields: &[bray_symbols::ConstantField<
            bray_symbols::UnionPayloadFieldSymbolId,
            ConstantValueId,
        >],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let variant = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let llvm_type = self.types.map(ty)?;
        let storage = llvm(self.builder.build_alloca(llvm_type, "constant.union"))?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(*tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag = integer_constant(tag_type, variant.tag());

        llvm(self.builder.build_store(storage, tag))?;

        for field in fields {
            let layout = variant
                .fields()
                .iter()
                .find(|layout| {
                    layout.reference()
                        == Some(bray_ir::MirFieldReference::UnionPayload(*field.field()))
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let destination = self.constant_offset_pointer(storage, layout.offset_bytes())?;
            let value = self.constant(*field.value())?;

            llvm(self.builder.build_store(destination, value))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "constant.union.value"),
        )
    }

    pub(super) fn constant_offset_pointer(
        &self,
        pointer: PointerValue<'context>,
        offset: u64,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let integer_type = self
            .types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None);

        let address = llvm(
            self.builder
                .build_ptr_to_int(pointer, integer_type, "address"),
        )?;

        let address = llvm(self.builder.build_int_add(
            address,
            integer_type.const_int(offset, false),
            "address.offset",
        ))?;

        llvm(
            self.builder
                .build_int_to_ptr(address, pointer.get_type(), "address.pointer"),
        )
    }

    pub(super) fn operand_type(
        &self,
        operand: &MirOperand,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        match operand {
            MirOperand::Value(value) => self
                .unit
                .value(*value)
                .map(bray_ir::MirValue::ty)
                .ok_or(CodegenFailure::GeneratedModuleInvariant),
            MirOperand::Constant { ty, .. } | MirOperand::Immediate { ty, .. } => Ok(*ty),
            MirOperand::Copy(place) | MirOperand::Move(place) => Ok(place.ty()),
        }
    }
}
