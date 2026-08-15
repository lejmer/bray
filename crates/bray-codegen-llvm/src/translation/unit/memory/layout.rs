use bray_bound_tree::MemoryLayoutQueryKind;
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{insert_value, int_value, integer_constant, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_layout_query(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        ty: bray_symbols::TypeId,
        kind: MemoryLayoutQueryKind,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let layout = self.memory_layout(ty)?;

        match kind {
            MemoryLayoutQueryKind::Size | MemoryLayoutQueryKind::Stride => {
                self.layout_integer_result(operation, layout.size())
            }
            MemoryLayoutQueryKind::Alignment => {
                self.layout_integer_result(operation, layout.alignment().get())
            }
            MemoryLayoutQueryKind::Layout => {
                self.translate_allocation_layout(operation, memory, layout)
            }
        }
    }

    fn layout_integer_result(
        &mut self,
        operation: &MirOperation,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation)?;

        let BasicTypeEnum::IntType(result) = self.types.map(result)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(result.const_int(value, false).into())
    }

    #[expect(
        clippy::manual_checked_ops,
        reason = "LLVM IR emits a runtime zero-stride guard before unsigned division"
    )]
    fn translate_allocation_layout(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        layout: bray_target::TargetValueLayout,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [count] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let count = self
            .operand(count)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let count = self.pointer_sized_integer(count.into())?;
        let integer = self.pointer_integer_type();
        let result = self.operation_result_type(operation)?;

        let maximum_alignment = self
            .request
            .target()
            .profile()
            .properties()
            .alignments()
            .max_allocation()
            .get();

        if layout.alignment().get() > maximum_alignment {
            return self.allocation_layout_error(result, 1);
        }

        let stride = layout.size();

        let bytes = llvm(self.builder.build_int_mul(
            count,
            integer.const_int(stride, false),
            "memory.layout.bytes",
        ))?;

        let overflow = if stride == 0 {
            self.types.context().bool_type().const_zero()
        } else {
            let width = integer.get_bit_width();

            if width > 64 {
                return Err(CodegenFailure::UnsupportedTarget);
            }

            let maximum = u64::MAX >> (64 - width);
            let limit = integer.const_int(maximum / stride, false);

            llvm(self.builder.build_int_compare(
                IntPredicate::UGT,
                count,
                limit,
                "memory.layout.overflow",
            ))?
        };

        let layout_type = self.union_payload_type(result, 0)?;

        let layout_value = self.construct_positional_product(
            layout_type,
            &[
                bytes.into(),
                integer.const_int(layout.alignment().get(), false).into(),
            ],
        )?;

        let success = self.construct_positional_union(result, 0, &[layout_value])?;
        let error = self.allocation_layout_error(result, 0)?;

        llvm(
            self.builder
                .build_select(overflow, error, success, "memory.layout.result"),
        )
    }

    fn allocation_layout_error(
        &mut self,
        result: bray_symbols::TypeId,
        error_ordinal: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let error_type = self.union_payload_type(result, 1)?;
        let error = self.construct_positional_union(error_type, error_ordinal, &[])?;

        self.construct_positional_union(result, 1, &[error])
    }

    fn union_payload_type(
        &self,
        ty: bray_symbols::TypeId,
        variant: usize,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        let mapping = self
            .type_mapping(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [field] = variants
            .get(variant)
            .map(bray_codegen::CodegenUnionVariantLayout::fields)
            .unwrap_or_default()
        else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(field.ty())
    }

    pub(super) fn construct_positional_product(
        &mut self,
        ty: bray_symbols::TypeId,
        values: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self
            .type_mapping(ty)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) => Some(fields.clone()),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if fields.len() != values.len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let mut result = self.types.map(ty)?.const_zero();

        for (index, value) in values.iter().copied().enumerate() {
            result = insert_value(
                &self.builder,
                result,
                value,
                self.aggregate_value_element(&fields, index)?,
            )?;
        }

        Ok(result)
    }

    fn construct_positional_union(
        &mut self,
        ty: bray_symbols::TypeId,
        ordinal: usize,
        values: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (tag, variant) = self
            .type_mapping(ty)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Union { tag, variants } => variants
                    .get(ordinal)
                    .cloned()
                    .map(|variant| (*tag, variant)),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if variant.fields().len() != values.len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let llvm_type = self.types.map(ty)?;
        let storage = llvm(self.builder.build_alloca(llvm_type, "memory.union"))?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        llvm(
            self.builder
                .build_store(storage, integer_constant(tag_type, variant.tag())),
        )?;

        for (field, value) in variant.fields().iter().zip(values.iter().copied()) {
            let destination = self.constant_offset_pointer(storage, field.offset_bytes())?;

            llvm(self.builder.build_store(destination, value))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "memory.union.value"),
        )
    }
}
