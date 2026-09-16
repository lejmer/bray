use bray_bound_tree::MemoryLayoutQueryKind;
use bray_codegen::{CodegenFailure, CodegenTypeBehavior, CodegenTypeKind};
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{insert_value, int_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_layout_query(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        ty: bray_symbols::TypeId,
        kind: MemoryLayoutQueryKind,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let layout = self.memory_layout(ty);

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
            MemoryLayoutQueryKind::Trailing => {
                self.translate_trailing_layout(operation, memory, ty, layout)
            }
        }
    }

    fn layout_integer_result(
        &mut self,
        operation: &MirOperation,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation);

        let BasicTypeEnum::IntType(result) = self.types.map(result)? else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        Ok(result.const_int(value, false).into())
    }

    fn translate_allocation_layout(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        layout: bray_target::TargetValueLayout,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.translate_layout_components(
            operation,
            memory,
            0,
            layout.size(),
            layout.alignment().get(),
        )
    }

    fn translate_trailing_layout(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        ty: bray_symbols::TypeId,
        layout: bray_target::TargetValueLayout,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let Some(CodegenTypeBehavior::FlexibleAggregate { element, offset }) =
            self.type_mapping(ty).and_then(|mapping| mapping.behavior())
        else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let stride = self.memory_layout(element).size();

        self.translate_layout_components(
            operation,
            memory,
            offset,
            stride,
            layout.alignment().get(),
        )
    }

    #[expect(
        clippy::manual_checked_ops,
        reason = "LLVM IR emits runtime multiplication and addition overflow guards"
    )]
    fn translate_layout_components(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        base: u64,
        stride: u64,
        alignment: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [count] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let count = self
            .operand(count)
            .map(|value| int_value(value).expect("checked MIR memory translation requires an established mapping or value"))?;

        let count = self.pointer_sized_integer(count.into())?;
        let integer = self.pointer_integer_type();
        let result = self.operation_result_type(operation);

        let maximum_alignment = self
            .request
            .target()
            .profile()
            .properties()
            .alignments()
            .max_allocation()
            .get();

        if alignment > maximum_alignment {
            return self.allocation_layout_error(result, 1);
        }

        let tail = llvm(self.builder.build_int_mul(
            count,
            integer.const_int(stride, false),
            "memory.layout.tail",
        ))?;

        let width = integer.get_bit_width();

        if width > 64 {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        let maximum = u64::MAX >> (64 - width);

        let base_overflow = self
            .types
            .context()
            .bool_type()
            .const_int(u64::from(base > maximum), false);

        let multiply_overflow = if stride == 0 {
            self.types.context().bool_type().const_zero()
        } else {
            let limit = integer.const_int(maximum.saturating_sub(base) / stride, false);

            llvm(self.builder.build_int_compare(
                IntPredicate::UGT,
                count,
                limit,
                "memory.layout.overflow",
            ))?
        };

        let bytes = llvm(self.builder.build_int_add(
            tail,
            integer.const_int(base, false),
            "memory.layout.bytes",
        ))?;

        let addition_overflow = llvm(self.builder.build_int_compare(
            IntPredicate::ULT,
            bytes,
            tail,
            "memory.layout.addition_overflow",
        ))?;

        let arithmetic_overflow = llvm(self.builder.build_or(
            multiply_overflow,
            addition_overflow,
            "memory.layout.overflow",
        ))?;

        let overflow = llvm(self.builder.build_or(
            arithmetic_overflow,
            base_overflow,
            "memory.layout.representable",
        ))?;

        let layout_type = self.union_payload_type(result, 0)?;

        let layout_value = self.construct_positional_product(
            layout_type,
            &[bytes.into(), integer.const_int(alignment, false).into()],
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
            .expect("checked MIR memory translation requires an established mapping or value");

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [field] = variants
            .get(variant)
            .map(bray_codegen::CodegenUnionVariantLayout::fields)
            .unwrap_or_default()
        else {
            panic!("checked MIR memory translation violated an established compiler contract");
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
            .expect("checked MIR memory translation requires an established mapping or value");

        if fields.len() != values.len() {
            panic!("checked MIR memory translation violated an established compiler contract");
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
            .expect("checked MIR memory translation requires an established mapping or value");

        if variant.fields().len() != values.len() {
            panic!("checked MIR memory translation violated an established compiler contract");
        }

        let llvm_type = self.types.map(ty)?;
        let storage = self.allocate_temporary(llvm_type, "memory.union")?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        self.store_union_tag(storage, tag, variant.tag())?;

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
