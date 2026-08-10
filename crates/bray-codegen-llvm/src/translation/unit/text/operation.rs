use bray_codegen::{CodegenFailure, CodegenTypeBehavior, CodegenTypeKind};
use bray_ir::{MirTextOperation, MirTextOperationKind};
use inkwell::AtomicOrdering;
use inkwell::IntPredicate;
use inkwell::builder::Builder;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, IntType, PointerType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, insert_value, integer_constant, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in super::super) fn translate_text(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let result = match operation.kind() {
            MirTextOperationKind::ScalarCount => self.text_scalar_count(operation)?,
            MirTextOperationKind::IsEmpty => self.text_is_empty(operation)?,
            MirTextOperationKind::Equals => self.text_equals(operation)?,
            MirTextOperationKind::ScalarAt => self.text_scalar_at(operation)?,
            MirTextOperationKind::ScalarSlice => self.text_scalar_slice(operation)?,
            MirTextOperationKind::Utf8 => self.text_utf8(operation)?,
            MirTextOperationKind::FromUtf8 => self.text_from_utf8(operation)?,
            MirTextOperationKind::CharacterScalarValue => self.character_scalar_value(operation)?,
            MirTextOperationKind::CharacterFromScalarValue => {
                self.character_from_scalar_value(operation)?
            }
            MirTextOperationKind::CharacterUtf8Length => self.character_utf8_length(operation)?,
            MirTextOperationKind::CharacterUtf8Byte => self.character_utf8_byte(operation)?,
            MirTextOperationKind::CharacterIsAlphabetic => self.character_predicate(
                operation,
                bray_runtime_abi::CHARACTER_IS_ALPHABETIC_SYMBOL,
                "character.alphabetic",
            )?,
            MirTextOperationKind::CharacterIsNumeric => self.character_predicate(
                operation,
                bray_runtime_abi::CHARACTER_IS_NUMERIC_SYMBOL,
                "character.numeric",
            )?,
            MirTextOperationKind::CharacterIsWhitespace => self.character_predicate(
                operation,
                bray_runtime_abi::CHARACTER_IS_WHITESPACE_SYMBOL,
                "character.whitespace",
            )?,
            MirTextOperationKind::Release => {
                self.release_string(operation)?;

                return Ok(None);
            }
        };

        Ok(Some(result))
    }

    pub(in super::super) fn retain_string(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        if !self.string_type(ty) {
            return Ok(());
        }

        let (_, _, owner) = self.string_parts(value, ty)?;

        let retained = self
            .types
            .context()
            .append_basic_block(self.function, "string.retain");

        let done = self
            .types
            .context()
            .append_basic_block(self.function, "string.retained");

        let owned = llvm(self.builder.build_is_not_null(owner, "string.owned"))?;

        llvm(self.builder.build_conditional_branch(owned, retained, done))?;

        self.builder.position_at_end(retained);

        let one = self.pointer_integer_type().const_int(1, false);

        llvm(self.builder.build_atomicrmw(
            inkwell::AtomicRMWBinOp::Add,
            owner,
            one,
            AtomicOrdering::Monotonic,
        ))?;

        llvm(self.builder.build_unconditional_branch(done))?;

        self.builder.position_at_end(done);

        Ok(())
    }

    fn text_utf8(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (data, length) = self.only_string_view(operation)?;

        let result = operation
            .result_type()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let fields = self.text_aggregate_fields(result, 2)?;
        let mut value = self.types.map(result)?.const_zero();

        value = insert_value(
            &self.builder,
            value,
            data.into(),
            self.aggregate_value_element(&fields, 0)?,
        )?;

        insert_value(
            &self.builder,
            value,
            length.into(),
            self.aggregate_value_element(&fields, 1)?,
        )
    }

    fn text_from_utf8(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(bytes, bytes_type)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (data, length) = self.slice_parts_value(*bytes, *bytes_type)?;

        let (result_data, result_length, result_owner) = self.string_outputs(data.get_type())?;

        let byte = self.types.context().i8_type();

        let function = self.text_function(
            bray_runtime_abi::STRING_FROM_UTF8_SYMBOL,
            Some(byte.into()),
            &[
                data.get_type().into(),
                length.get_type().into(),
                result_data.get_type().into(),
                result_length.get_type().into(),
                result_owner.get_type().into(),
            ],
        );

        let valid = self
            .call_value(
                function,
                &[
                    data.into(),
                    length.into(),
                    result_data.into(),
                    result_length.into(),
                    result_owner.into(),
                ],
                "string.from_utf8",
            )?
            .into_int_value();

        let valid = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            valid,
            valid.get_type().const_zero(),
            "string.utf8.valid",
        ))?;

        let string = self.load_owned_string(operation, result_data, result_length, result_owner)?;

        self.utf8_result(operation.result_type(), valid, string)
    }

    fn release_string(&mut self, operation: &MirTextOperation) -> Result<(), CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(value, ty)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, length, owner) = self.string_parts(*value, *ty)?;

        let release = self
            .types
            .context()
            .append_basic_block(self.function, "string.release");

        let deallocate = self
            .types
            .context()
            .append_basic_block(self.function, "string.deallocate");

        let done = self
            .types
            .context()
            .append_basic_block(self.function, "string.released");

        let owned = llvm(self.builder.build_is_not_null(owner, "string.owned"))?;

        llvm(self.builder.build_conditional_branch(owned, release, done))?;

        self.builder.position_at_end(release);

        let integer = self.pointer_integer_type();
        let one = integer.const_int(1, false);

        let previous = llvm(self.builder.build_atomicrmw(
            inkwell::AtomicRMWBinOp::Sub,
            owner,
            one,
            AtomicOrdering::AcquireRelease,
        ))?;

        let last = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            previous,
            one,
            "string.last_owner",
        ))?;

        llvm(
            self.builder
                .build_conditional_branch(last, deallocate, done),
        )?;

        self.builder.position_at_end(deallocate);

        let bytes = llvm(self.builder.build_int_add(
            length,
            integer.const_int(u64::from(integer.get_bit_width() / 8), false),
            "string.allocation_size",
        ))?;

        let alignment = integer.const_int(u64::from(integer.get_bit_width() / 8), false);
        let function = self.memory_deallocation_function(owner.get_type());

        llvm(self.builder.build_call(
            function,
            &[owner.into(), bytes.into(), alignment.into()],
            "string.deallocate",
        ))?;

        llvm(self.builder.build_unconditional_branch(done))?;

        self.builder.position_at_end(done);

        Ok(())
    }

    pub(super) fn only_string_view(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<(PointerValue<'context>, IntValue<'context>), CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(value, ty)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (data, length, _) = self.borrowed_string_parts(*value, *ty)?;

        Ok((data, length))
    }

    pub(super) fn only_scalar_operand(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(value, _)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(value.into_int_value())
    }

    pub(super) fn text_operands(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<Vec<(BasicValueEnum<'context>, bray_symbols::TypeId)>, CodegenFailure> {
        if operation.operands().len() != operation.operand_types().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        operation
            .operands()
            .iter()
            .zip(operation.operand_types())
            .map(|(operand, ty)| self.operand(operand).map(|value| (value, *ty)))
            .collect()
    }

    pub(super) fn borrowed_string_parts(
        &mut self,
        value: BasicValueEnum<'context>,
        borrow: bray_symbols::TypeId,
    ) -> Result<
        (
            PointerValue<'context>,
            IntValue<'context>,
            PointerValue<'context>,
        ),
        CodegenFailure,
    > {
        let target = self
            .type_mapping(borrow)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Pointer { target, .. } => Some(*target),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pointer = value.into_pointer_value();

        let value = llvm(self.builder.build_load(
            self.types.map(target)?,
            pointer,
            "string.borrowed",
        ))?;

        self.string_parts(value, target)
    }

    fn string_parts(
        &self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<
        (
            PointerValue<'context>,
            IntValue<'context>,
            PointerValue<'context>,
        ),
        CodegenFailure,
    > {
        let fields = self.text_aggregate_fields(ty, 3)?;

        let data = extract_value(
            &self.builder,
            value,
            u32::try_from(self.aggregate_value_element(&fields, 0)?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?
        .into_pointer_value();

        let owner = extract_value(
            &self.builder,
            value,
            u32::try_from(self.aggregate_value_element(&fields, 1)?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?
        .into_pointer_value();

        let length = extract_value(
            &self.builder,
            value,
            u32::try_from(self.aggregate_value_element(&fields, 2)?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?
        .into_int_value();

        Ok((data, length, owner))
    }

    fn slice_parts_value(
        &self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<(PointerValue<'context>, IntValue<'context>), CodegenFailure> {
        let fields = self.text_aggregate_fields(ty, 2)?;

        let data = extract_value(
            &self.builder,
            value,
            u32::try_from(self.aggregate_value_element(&fields, 0)?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?
        .into_pointer_value();

        let length = extract_value(
            &self.builder,
            value,
            u32::try_from(self.aggregate_value_element(&fields, 1)?)
                .map_err(|_| CodegenFailure::ResourceExhausted)?,
        )?
        .into_int_value();

        Ok((data, length))
    }

    pub(super) fn string_outputs(
        &self,
        pointer: PointerType<'context>,
    ) -> Result<
        (
            PointerValue<'context>,
            PointerValue<'context>,
            PointerValue<'context>,
        ),
        CodegenFailure,
    > {
        let data = llvm(self.builder.build_alloca(pointer, "string.result.data"))?;

        let length = llvm(
            self.builder
                .build_alloca(self.pointer_integer_type(), "string.result.length"),
        )?;

        let owner = llvm(self.builder.build_alloca(pointer, "string.result.owner"))?;

        llvm(self.builder.build_store(data, pointer.const_null()))?;

        llvm(
            self.builder
                .build_store(length, self.pointer_integer_type().const_zero()),
        )?;

        llvm(self.builder.build_store(owner, pointer.const_null()))?;

        Ok((data, length, owner))
    }

    pub(super) fn load_owned_string(
        &mut self,
        operation: &MirTextOperation,
        data: PointerValue<'context>,
        length: PointerValue<'context>,
        owner: PointerValue<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let data = llvm(self.builder.build_load(
            data.get_type(),
            data,
            "string.result.data.value",
        ))?
        .into_pointer_value();

        let length = llvm(self.builder.build_load(
            self.pointer_integer_type(),
            length,
            "string.result.length.value",
        ))?
        .into_int_value();

        let owner = llvm(self.builder.build_load(
            owner.get_type(),
            owner,
            "string.result.owner.value",
        ))?
        .into_pointer_value();

        let result = operation
            .result_type()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if self.string_type(result) {
            return self.string_value(result, data, length, owner);
        }

        let string = self.result_string_type(result)?;

        self.string_value(string, data, length, owner)
    }

    fn string_value(
        &mut self,
        ty: bray_symbols::TypeId,
        data: PointerValue<'context>,
        length: IntValue<'context>,
        owner: PointerValue<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self.text_aggregate_fields(ty, 3)?;
        let mut value = self.types.map(ty)?.const_zero();

        for (index, field) in [data.into(), owner.into(), length.into()]
            .into_iter()
            .enumerate()
        {
            value = insert_value(
                &self.builder,
                value,
                field,
                self.aggregate_value_element(&fields, index)?,
            )?;
        }

        Ok(value)
    }

    pub(super) fn nullable_value(
        &mut self,
        result: Option<bray_symbols::TypeId>,
        present: IntValue<'context>,
        scalar: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = result.ok_or(CodegenFailure::GeneratedModuleInvariant)?;
        let fields = self.text_aggregate_fields(result, 2)?;
        let mut value = self.types.map(result)?.const_zero();

        value = insert_value(
            &self.builder,
            value,
            present.into(),
            self.aggregate_value_element(&fields, 0)?,
        )?;

        value = insert_value(
            &self.builder,
            value,
            scalar,
            self.aggregate_value_element(&fields, 1)?,
        )?;

        Ok(value)
    }

    fn utf8_result(
        &mut self,
        result: Option<bray_symbols::TypeId>,
        valid: IntValue<'context>,
        string: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = result.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mapping = self
            .type_mapping(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let success = variants
            .iter()
            .find(|variant| {
                variant
                    .fields()
                    .iter()
                    .any(|field| self.string_type(field.ty()))
            })
            .cloned()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let failure = variants
            .iter()
            .find(|variant| variant.variant() != success.variant())
            .cloned()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let tag = *tag;
        let success = self.union_value(result, tag, &success, Some(string))?;
        let failure = self.union_value(result, tag, &failure, None)?;

        llvm(
            self.builder
                .build_select(valid, success, failure, "string.utf8.result"),
        )
    }

    fn union_value(
        &mut self,
        result: bray_symbols::TypeId,
        tag: bray_symbols::TypeId,
        variant: &bray_codegen::CodegenUnionVariantLayout,
        payload: Option<BasicValueEnum<'context>>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let llvm_type = self.types.map(result)?;
        let storage = llvm(self.builder.build_alloca(llvm_type, "string.result"))?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        llvm(
            self.builder
                .build_store(storage, integer_constant(tag_type, variant.tag())),
        )?;

        if let Some(payload) = payload {
            let field = variant
                .fields()
                .iter()
                .find(|field| self.string_type(field.ty()))
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let destination = self.constant_offset_pointer(storage, field.offset_bytes())?;

            llvm(self.builder.build_store(destination, payload))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "string.result.value"),
        )
    }

    fn result_string_type(
        &self,
        result: bray_symbols::TypeId,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        let mapping = self
            .type_mapping(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        variants
            .iter()
            .flat_map(|variant| variant.fields())
            .map(bray_codegen::CodegenFieldLayout::ty)
            .find(|ty| self.string_type(*ty))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    fn string_type(&self, ty: bray_symbols::TypeId) -> bool {
        self.type_mapping(ty)
            .is_some_and(|mapping| mapping.behavior() == Some(CodegenTypeBehavior::String))
    }

    fn text_aggregate_fields(
        &self,
        ty: bray_symbols::TypeId,
        count: usize,
    ) -> Result<std::sync::Arc<[bray_codegen::CodegenFieldLayout]>, CodegenFailure> {
        self.type_mapping(ty)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) if fields.len() == count => Some(fields.clone()),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn text_function(
        &self,
        name: &str,
        result: Option<BasicTypeEnum<'context>>,
        parameters: &[BasicMetadataTypeEnum<'context>],
    ) -> FunctionValue<'context> {
        self.module.get_function(name).unwrap_or_else(|| {
            let ty = result.map_or_else(
                || self.types.context().void_type().fn_type(parameters, false),
                |result| result.fn_type(parameters, false),
            );

            self.module.add_function(name, ty, None)
        })
    }

    pub(super) fn call_value(
        &self,
        function: FunctionValue<'context>,
        arguments: &[BasicMetadataValueEnum<'context>],
        name: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        llvm(self.builder.build_call(function, arguments, name))?
            .try_as_basic_value()
            .basic()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}

pub(super) fn zeroed_scalar_output<'context>(
    builder: &Builder<'context>,
    ty: IntType<'context>,
    name: &str,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let output = llvm(builder.build_alloca(ty, name))?;

    llvm(builder.build_store(output, ty.const_zero()))?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;

    use super::zeroed_scalar_output;

    #[test]
    fn nullable_native_outputs_are_initialized_before_the_call_can_report_absence() {
        let context = Context::create();
        let module = context.create_module("nullable-output");

        let function = module.add_function(
            "nullable_output",
            context.void_type().fn_type(&[], false),
            None,
        );

        let builder = context.create_builder();
        let entry = context.append_basic_block(function, "entry");

        builder.position_at_end(entry);

        zeroed_scalar_output(&builder, context.i32_type(), "scalar")
            .unwrap_or_else(|error| panic!("nullable output must initialize: {error:?}"));

        builder
            .build_return(None)
            .unwrap_or_else(|error| panic!("test function must return: {error:?}"));

        let ir = module.print_to_string().to_string();

        assert!(ir.contains("store i32 0, ptr %scalar"), "{ir}");
        assert!(module.verify().is_ok(), "{ir}");
    }
}
