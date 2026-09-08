use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, llvm};
use super::support::LoadedMemoryAggregate;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_raw_buffer_field(
        &mut self,
        memory: &MirMemoryOperation,
        field: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let element = self.aggregate_element(&fields, field)?;

        extract_value(&self.builder, value, element)
    }

    pub(super) fn translate_raw_buffer_slice(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let result = self.operation_result_type(operation)?;

        self.construct_positional_product(result, &[pointer.into(), initialized.into()])
    }

    pub(super) fn translate_raw_buffer_spare_pointer(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let stride = self.memory_layout(element)?.size();

        self.dynamic_offset_pointer(pointer, initialized, stride)
    }

    pub(super) fn translate_raw_buffer_set_initialized_count(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [buffer, count] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type, _] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (buffer, llvm_type, _, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let count = self.operand(count)?;

        self.store_raw_buffer_initialized_count(
            buffer,
            llvm_type,
            &fields,
            count,
            "memory.buffer.initialized",
        )
    }

    pub(super) fn translate_raw_buffer_relocate(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        let [source, destination] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [source_type, destination_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (source, source_llvm_type, source_value, source_fields) =
            self.load_raw_buffer(source, *source_type)?;

        let (destination, destination_llvm_type, destination_value, destination_fields) =
            self.load_raw_buffer(destination, *destination_type)?;

        let source_pointer = self.memory_aggregate_pointer(source_value, &source_fields, 0)?;
        let initialized = self.memory_aggregate_integer(source_value, &source_fields, 2)?;

        let destination_pointer =
            self.memory_aggregate_pointer(destination_value, &destination_fields, 0)?;

        let layout = self.memory_layout(element)?;

        let bytes = llvm(self.builder.build_int_mul(
            initialized,
            self.pointer_integer_type().const_int(layout.size(), false),
            "memory.buffer.relocate.bytes",
        ))?;

        let alignment =
            crate::conversion::target_value(layout.alignment().get(), "memory_buffer_alignment")?;

        llvm(self.builder.build_memcpy(
            destination_pointer,
            alignment,
            source_pointer,
            alignment,
            bytes,
        ))?;

        self.observe_memory_copy(bytes)?;

        self.store_raw_buffer_initialized_count(
            destination,
            destination_llvm_type,
            &destination_fields,
            initialized.into(),
            "memory.buffer.relocate.destination",
        )?;

        self.store_raw_buffer_initialized_count(
            source,
            source_llvm_type,
            &source_fields,
            self.pointer_integer_type().const_zero().into(),
            "memory.buffer.relocate.source",
        )
    }

    fn store_raw_buffer_initialized_count(
        &self,
        buffer: PointerValue<'context>,
        llvm_type: BasicTypeEnum<'context>,
        fields: &[bray_codegen::CodegenFieldLayout],
        initialized: BasicValueEnum<'context>,
        name: &str,
    ) -> Result<(), CodegenFailure> {
        let BasicTypeEnum::StructType(llvm_type) = llvm_type else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let initialized_field = self.aggregate_element(fields, 2)?;

        let initialized_pointer =
            llvm(
                self.builder
                    .build_struct_gep(llvm_type, buffer, initialized_field, name),
            )?;

        llvm(self.builder.build_store(initialized_pointer, initialized))?;

        Ok(())
    }

    fn load_raw_buffer(
        &mut self,
        buffer: &bray_ir::MirOperand,
        buffer_type: bray_symbols::TypeId,
    ) -> Result<LoadedMemoryAggregate<'context>, CodegenFailure> {
        self.load_owned_memory(buffer, buffer_type, 3, "memory.buffer")
    }
}
