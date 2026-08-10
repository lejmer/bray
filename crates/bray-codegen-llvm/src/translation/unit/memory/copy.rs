use bray_bound_tree::MemoryCopyKind;
use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
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

        let count = self
            .operand(count)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let layout = self.memory_layout(pointee)?;

        let bytes = llvm(self.builder.build_int_mul(
            self.pointer_sized_integer(count.into())?,
            self.pointer_integer_type().const_int(layout.size(), false),
            "memory.copy.bytes",
        ))?;

        let alignment = u32::try_from(layout.alignment().get())
            .map_err(|_| CodegenFailure::UnsupportedTarget)?;

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

        Ok(())
    }

    pub(super) fn translate_byte_buffer_copy(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [source, destination, count] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let source = self.memory_pointer(source)?;
        let destination = self.memory_pointer(destination)?;

        let count = self
            .operand(count)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let bytes = self.pointer_sized_integer(count.into())?;

        llvm(self.builder.build_memcpy(destination, 1, source, 1, bytes))?;

        Ok(())
    }

    pub(super) fn translate_byte_buffer_read(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [pointer, index] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.memory_pointer(pointer)?;
        let index = self.pointer_sized_memory_operand(index)?;
        let pointer = self.dynamic_offset_pointer(pointer, index, 1)?;
        let result = self.operation_result_type(operation)?;

        llvm(
            self.builder
                .build_load(self.types.map(result)?, pointer, "byte.buffer.read"),
        )
    }

    pub(super) fn translate_slice_length(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [slice] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let slice = self.operand(slice)?.into_struct_value();

        llvm(self.builder.build_extract_value(slice, 1, "slice.length"))
    }

    pub(super) fn translate_byte_buffer_fill(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [destination, value, count] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let destination = self.memory_pointer(destination)?;

        let value = self
            .operand(value)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let count = self.pointer_sized_memory_operand(count)?;

        llvm(self.builder.build_memset(destination, 1, value, count))?;

        Ok(())
    }
}
