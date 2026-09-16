use bray_codegen::{CodegenFailure, CodegenTypeBehavior, CodegenTypeKind};
use bray_ir::{
    MirHelperReference, MirOperationId, MirStandardLibraryHelper, MirTextOperation,
    MirTextOperationKind,
};
use inkwell::AtomicOrdering;
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, insert_value, integer_constant, llvm, next_helper};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in super::super) fn translate_text(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let result = match operation.kind() {
            MirTextOperationKind::ScalarCount => self.text_scalar_count(operation_id, operation)?,
            MirTextOperationKind::IsEmpty => self.text_is_empty(operation)?,
            MirTextOperationKind::Equals => self.text_equals(operation_id, operation)?,
            MirTextOperationKind::ScalarAt => self.text_scalar_at(operation_id, operation)?,
            MirTextOperationKind::ScalarSlice => self.text_scalar_slice(operation_id, operation)?,
            MirTextOperationKind::Utf8 => self.text_utf8(operation)?,
            MirTextOperationKind::FromUtf8 => self.text_from_utf8(operation_id, operation)?,
            MirTextOperationKind::CharacterScalarValue => {
                self.character_scalar_value(operation_id, operation)?
            }
            MirTextOperationKind::CharacterFromScalarValue => {
                self.character_from_scalar_value(operation_id, operation)?
            }
            MirTextOperationKind::CharacterUtf8Length => {
                self.character_utf8_length(operation_id, operation)?
            }
            MirTextOperationKind::CharacterUtf8Byte => {
                self.character_utf8_byte(operation_id, operation)?
            }
            MirTextOperationKind::CharacterIsAlphabetic => self.character_predicate(
                operation_id,
                operation,
                MirStandardLibraryHelper::CharacterIsAlphabetic,
            )?,
            MirTextOperationKind::CharacterIsNumeric => self.character_predicate(
                operation_id,
                operation,
                MirStandardLibraryHelper::CharacterIsNumeric,
            )?,
            MirTextOperationKind::CharacterIsWhitespace => self.character_predicate(
                operation_id,
                operation,
                MirStandardLibraryHelper::CharacterIsWhitespace,
            )?,
            MirTextOperationKind::Release => {
                self.release_string(operation_id, operation)?;

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
            .expect("checked MIR text translation requires an established mapping or value");

        let fields = self.text_aggregate_fields(result, 2);
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
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(bytes, bytes_type)] = operands.as_slice() else {
            panic!("checked MIR text translation violated an established compiler contract");
        };

        let (data, length) = self.slice_parts_value(*bytes, *bytes_type)?;

        let (validated, validated_type) = self.invoke_text_helper_result(
            operation_id,
            MirStandardLibraryHelper::StringFromUtf8,
            &[data.into(), length.into()],
        )?;

        let fields = match self
            .type_mapping(validated_type)
            .map(bray_codegen::CodegenTypeMapping::kind)
        {
            Some(CodegenTypeKind::Aggregate(fields)) => fields.clone(),
            unexpected => panic!("checked MIR text translation violated an established compiler contract: {unexpected:?}"),
        };

        let valid = extract_value(
            &self.builder,
            validated,
            self.aggregate_element(&fields, 0)?,
        )?
        .into_int_value();

        let text = extract_value(
            &self.builder,
            validated,
            self.aggregate_element(&fields, 1)?,
        )?;

        let text_type = fields
            .get(1)
            .map(bray_codegen::CodegenFieldLayout::ty)
            .expect("checked MIR text translation requires an established mapping or value");

        let string = self.owned_text(operation, text, text_type)?;

        self.utf8_result(operation.result_type(), valid, string)
    }

    fn release_string(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<(), CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(value, ty)] = operands.as_slice() else {
            panic!("checked MIR text translation violated an established compiler contract");
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
        let helpers = self.operation_helpers(operation_id)?;
        let mut helpers = helpers.iter();

        let helper = next_helper(
            &mut helpers,
            &MirHelperReference::StandardLibrary(MirStandardLibraryHelper::MemoryDeallocate),
        );

        if self
            .invoke_helper(helper, &[owner.into(), bytes.into(), alignment.into()])?
            .is_some()
        {
            panic!("checked MIR text translation violated an established compiler contract");
        }

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
            panic!("checked MIR text translation violated an established compiler contract");
        };

        let (data, length, _) = self.string_view_parts(*value, *ty)?;

        Ok((data, length))
    }

    pub(super) fn text_operands(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<Vec<(BasicValueEnum<'context>, bray_symbols::TypeId)>, CodegenFailure> {
        if operation.operands().len() != operation.operand_types().len() {
            panic!("checked MIR text translation violated an established compiler contract");
        }

        operation
            .operands()
            .iter()
            .zip(operation.operand_types())
            .map(|(operand, ty)| self.operand(operand).map(|value| (value, *ty)))
            .collect()
    }

    pub(in super::super) fn string_view_parts(
        &mut self,
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
        let mapping = self
            .type_mapping(ty)
            .expect("checked MIR text translation requires an established mapping or value");

        let CodegenTypeKind::Pointer { target, .. } = mapping.kind() else {
            return self.string_parts(value, ty);
        };

        let target = *target;

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
        let fields = self.text_aggregate_fields(ty, 3);

        let data = extract_value(
            &self.builder,
            value,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 0)?,
                "text_value_field_index",
            )?,
        )?
        .into_pointer_value();

        let owner = extract_value(
            &self.builder,
            value,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 1)?,
                "text_value_field_index",
            )?,
        )?
        .into_pointer_value();

        let length = extract_value(
            &self.builder,
            value,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 2)?,
                "text_value_field_index",
            )?,
        )?
        .into_int_value();

        Ok((data, length, owner))
    }

    fn slice_parts_value(
        &self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<(PointerValue<'context>, IntValue<'context>), CodegenFailure> {
        let fields = self.text_aggregate_fields(ty, 2);

        let data = extract_value(
            &self.builder,
            value,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 0)?,
                "text_value_field_index",
            )?,
        )?
        .into_pointer_value();

        let length = extract_value(
            &self.builder,
            value,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 1)?,
                "text_value_field_index",
            )?,
        )?
        .into_int_value();

        Ok((data, length))
    }

    pub(super) fn string_value(
        &mut self,
        ty: bray_symbols::TypeId,
        data: PointerValue<'context>,
        length: IntValue<'context>,
        owner: PointerValue<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self.text_aggregate_fields(ty, 3);
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

    fn utf8_result(
        &mut self,
        result: Option<bray_symbols::TypeId>,
        valid: IntValue<'context>,
        string: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = result.expect("checked MIR text translation requires an established mapping or value");

        let mapping = self
            .type_mapping(result)
            .expect("checked MIR text translation requires an established mapping or value");

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            panic!("checked MIR text translation violated an established compiler contract");
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
            .expect("checked MIR text translation requires an established mapping or value");

        let failure = variants
            .iter()
            .find(|variant| variant.variant() != success.variant())
            .cloned()
            .expect("checked MIR text translation requires an established mapping or value");

        let tag = (*tag).expect("checked MIR text translation requires an established mapping or value");
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
        let storage = self.allocate_temporary(llvm_type, "string.result")?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            panic!("checked MIR text translation violated an established compiler contract");
        };

        llvm(
            self.builder.build_store(
                storage,
                integer_constant(
                    tag_type,
                    variant
                        .tag()
                        .expect("checked MIR text translation requires an established mapping or value"),
                ),
            ),
        )?;

        if let Some(payload) = payload {
            let field = variant
                .fields()
                .iter()
                .find(|field| self.string_type(field.ty()))
                .expect("checked MIR text translation requires an established mapping or value");

            let destination = self.constant_offset_pointer(storage, field.offset_bytes())?;

            llvm(self.builder.build_store(destination, payload))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "string.result.value"),
        )
    }

    pub(super) fn result_string_type(
        &self,
        result: bray_symbols::TypeId,
    ) -> bray_symbols::TypeId {
        let mapping = self
            .type_mapping(result)
            .unwrap_or_else(|| panic!("text result type {result:?} has no codegen mapping"));

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            panic!(
                "text result type {result:?} must be a union, got {:?}",
                mapping.kind()
            );
        };

        variants
            .iter()
            .flat_map(|variant| variant.fields())
            .map(bray_codegen::CodegenFieldLayout::ty)
            .find(|ty| self.string_type(*ty))
            .expect("text result union must contain its string payload type")
    }

    pub(super) fn string_type(&self, ty: bray_symbols::TypeId) -> bool {
        self.type_mapping(ty)
            .is_some_and(|mapping| mapping.behavior() == Some(CodegenTypeBehavior::String))
    }

    fn text_aggregate_fields(
        &self,
        ty: bray_symbols::TypeId,
        count: usize,
    ) -> std::sync::Arc<[bray_codegen::CodegenFieldLayout]> {
        let mapping = self
            .type_mapping(ty)
            .unwrap_or_else(|| panic!("text aggregate type {ty:?} has no codegen mapping"));

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            panic!("text aggregate type {ty:?} maps to {:?}", mapping.kind());
        };

        assert_eq!(
            fields.len(),
            count,
            "text aggregate type {ty:?} must have {count} fields"
        );

        fields.clone()
    }
}
