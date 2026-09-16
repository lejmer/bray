use bray_bound_tree::MemoryCopyKind;
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_byte_buffer_copy(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [source, destination, count] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let source = self.memory_pointer(source)?;
        let destination = self.memory_pointer(destination)?;
        let bytes = self.pointer_sized_memory_operand(count)?;

        llvm(self.builder.build_memcpy(destination, 1, source, 1, bytes))?;

        self.observe_memory_copy(bytes)?;

        Ok(())
    }

    pub(super) fn translate_memory_copy(
        &mut self,
        destination: &bray_ir::MirOperand,
        source: &bray_ir::MirOperand,
        count: &bray_ir::MirOperand,
        pointee: bray_symbols::TypeId,
        kind: MemoryCopyKind,
    ) -> Result<(), CodegenFailure> {
        let destination = self.memory_pointer(destination)?;
        let source = self.memory_pointer(source)?;

        let count = self.operand(count).map(|value| {
            int_value(value)
                .expect("checked MIR memory translation requires an established mapping or value")
        })?;

        let layout = self.memory_layout(pointee);

        let bytes = llvm(self.builder.build_int_mul(
            self.pointer_sized_integer(count.into())?,
            self.pointer_integer_type().const_int(layout.size(), false),
            "memory.copy.bytes",
        ))?;

        let alignment =
            crate::conversion::target_value(layout.alignment().get(), "memory_copy_alignment")?;

        match kind {
            MemoryCopyKind::NonOverlapping => {
                llvm(
                    self.builder
                        .build_memcpy(destination, alignment, source, alignment, bytes),
                )?;
            }
            MemoryCopyKind::Overlapping => {
                llvm(
                    self.builder
                        .build_memmove(destination, alignment, source, alignment, bytes),
                )?;
            }
        }

        self.observe_memory_copy(bytes)?;

        Ok(())
    }

    pub(super) fn translate_byte_buffer_read(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [pointer, index] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let pointer = self.memory_pointer(pointer)?;
        let index = self.pointer_sized_memory_operand(index)?;
        let pointer = self.dynamic_offset_pointer(pointer, index, 1)?;
        let result = self.operation_result_type(operation);

        llvm(
            self.builder
                .build_load(self.types.map(result)?, pointer, "byte.buffer.read"),
        )
    }

    pub(super) fn translate_sequence_length(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [slice] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [operand_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let mapping = self
            .type_mapping(*operand_type)
            .expect("checked MIR memory translation requires an established mapping or value");

        if let CodegenTypeKind::Pointer { target, .. } = mapping.kind()
            && let Some(CodegenTypeKind::Array { length, .. }) =
                self.type_mapping(*target).map(|mapping| mapping.kind())
        {
            let result = memory
                .result_type()
                .expect("checked MIR memory translation requires an established mapping or value");

            return Ok(self
                .types
                .map(result)?
                .into_int_type()
                .const_int(*length, false)
                .into());
        }

        let slice = self.operand(slice)?.into_struct_value();

        llvm(self.builder.build_extract_value(slice, 1, "slice.length"))
    }

    pub(super) fn translate_byte_buffer_fill(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [destination, value, count] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let destination = self.memory_pointer(destination)?;

        let value = self.operand(value).map(|value| {
            int_value(value)
                .expect("checked MIR memory translation requires an established mapping or value")
        })?;

        let count = self.pointer_sized_memory_operand(count)?;

        llvm(self.builder.build_memset(destination, 1, value, count))?;

        Ok(())
    }
}
