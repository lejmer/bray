use bray_bound_tree::MemoryLayoutQueryKind;
use bray_codegen::{CodegenFailure, CodegenTypeBehavior, CodegenTypeKind};
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};

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
                self.translate_allocation_layout(operation, memory, layout, false)
            }
            MemoryLayoutQueryKind::LayoutStatus => {
                self.translate_allocation_layout(operation, memory, layout, true)
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
        status_only: bool,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.translate_layout_components(
            operation,
            memory,
            0,
            layout.size(),
            layout.alignment().get(),
            status_only,
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
            false,
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
        status_only: bool,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [count] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let count = self.operand(count).map(|value| {
            int_value(value)
                .expect("checked MIR memory translation requires an established mapping or value")
        })?;

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
            if status_only {
                return self.layout_integer_result(operation, 2);
            }

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

        let overflow =
            if let Some(limit) = bray_target::allocation_count_limit(base, stride, maximum) {
                llvm(self.builder.build_int_compare(
                    IntPredicate::UGT,
                    count,
                    integer.const_int(limit, false),
                    "memory.layout.overflow",
                ))?
            } else {
                self.types.context().bool_type().const_int(1, false)
            };

        let bytes = llvm(self.builder.build_int_add(
            tail,
            integer.const_int(base, false),
            "memory.layout.bytes",
        ))?;

        if status_only {
            let BasicTypeEnum::IntType(status_type) = self.types.map(result)? else {
                panic!("checked allocation layout status has an integer result");
            };

            return llvm(self.builder.build_select(
                overflow,
                status_type.const_int(1, false),
                status_type.const_zero(),
                "memory.layout.status",
            ));
        }

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
            let destination = self.representation_offset_pointer(storage, field.offset_bytes())?;

            self.store_value(destination, value, field.ty())?;
        }

        self.place_value(storage, ty, "memory.union.value")
    }
}
